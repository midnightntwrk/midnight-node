// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! An indexer-backed [`BuilderContext`] that answers wallet queries from the Midnight indexer's
//! GraphQL API instead of replaying every block into a local [`super::super::LedgerState`]
//! (see issue #1186).
//!
//! This PR (#1) wires up only the read-only `show-wallet` path: [`IndexerContext::init_wallets`]
//! connects to the indexer, drains the shielded / unshielded / dust subscriptions to the chain tip
//! and builds a fully-synced [`Wallet`]. The three [`BuilderContext`] methods that `show-wallet`
//! needs — [`with_wallet_from_seed`](BuilderContext::with_wallet_from_seed),
//! [`with_wallets_from_seeds`](BuilderContext::with_wallets_from_seeds) and
//! [`unshielded_utxos`](BuilderContext::unshielded_utxos) — read that synced state. The remaining
//! methods (used only when *building* transactions) stay `todo!()` for the follow-up.
//!
//! Only the latest ledger version (v9 / [`DefaultDB`]) is supported: the indexer's `schema-v4`
//! tracks the current protocol, and the blob decoding below pins to those types.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures::future::try_join_all;
use tokio::time::timeout;

use super::super::{
	BindingKind, BlockContext, ContractAddress, ContractState, DB, DefaultDB, DustWallet, Event,
	HashOutput, IntentHash, IntoWalletAddress, LedgerParameters, LedgerState,
	MerkleTreeCollapsedUpdate, Offer, PedersenDowngradeable, ProofKind, ProofMarker,
	PureGeneratorPedersen, Resolver, Serializable, ShieldedWallet, Signature, SignatureKind,
	Storable, Tagged, Timestamp, Transaction, UnshieldedTokenType, UnshieldedWallet, Utxo, Wallet,
	WalletSeed, ZswapChainState, deserialize,
};
use super::BuilderContext;
use super::indexer_client::{
	IndexerClient, IndexerClientError, ShieldedEvent, TransactionResultKind, UnshieldedEvent,
	UnshieldedUtxoData,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Dead-connection backstop for the progress-bearing subscriptions (shielded, unshielded).
///
/// These streams tell us we've caught up via their progress sentinels, not via silence: the
/// indexer emits a progress heartbeat every `progress_update_interval` (30s server-side, see the
/// indexer's `config.yaml`) even when there are no relevant transactions. That heartbeat is the
/// only traffic on a sparse wallet's subscription, so this timeout MUST stay comfortably above the
/// server's interval — set it equal (as a prior version did) and the local timer races the
/// heartbeat (which also has to cross the network and run a DB query), wins, and we wrongly
/// conclude a sparse-but-live wallet is drained. Kept at a small multiple of the 30s heartbeat so
/// only a genuinely stalled connection trips it.
const PROGRESS_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Idle timeout for the dust ledger-events subscription, which has no progress heartbeat. Here
/// silence genuinely means "no more events" (e.g. a chain with no dust events at all), and there
/// is no heartbeat to race, so this needs no margin over the server's progress interval.
const DUST_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// How often [`SyncProgress::log_until_done`] emits a one-line sync-progress heartbeat while
/// [`IndexerContext::init_wallets`] drains the subscriptions.
const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(5);

/// An indexer-backed [`BuilderContext`].
///
/// Holds the synced wallet state keyed by seed. The wallet map mirrors
/// [`super::LedgerContext`]'s, including the `get_disjoint_mut` guard in
/// [`with_wallets_from_seeds`](BuilderContext::with_wallets_from_seeds).
pub struct IndexerContext<D: DB + Clone> {
	client: IndexerClient,
	/// Network id used to derive viewing keys / addresses (the indexer has no network field).
	network_id: String,
	/// Synced wallets, populated by [`IndexerContext::init_wallets`].
	wallets: Mutex<HashMap<WalletSeed, Wallet<D>>>,
	/// Synced unshielded UTXOs per seed (created-minus-spent), with creation time.
	unshielded: Mutex<HashMap<WalletSeed, Vec<(Utxo, Timestamp)>>>,
}

impl<D: DB + Clone> IndexerContext<D> {
	/// Build a context targeting `indexer_url` (an `api/v4` base, e.g.
	/// `http://127.0.0.1:8088/api/v4`). `network_id` (e.g. `undeployed`) is used for viewing-key
	/// and address derivation.
	pub fn new(
		indexer_url: &str,
		network_id: impl Into<String>,
	) -> Result<Self, IndexerClientError> {
		Ok(Self {
			client: IndexerClient::new(indexer_url)?,
			network_id: network_id.into(),
			wallets: Mutex::new(HashMap::new()),
			unshielded: Mutex::new(HashMap::new()),
		})
	}

	/// Get or panic on a missing wallet within an existing lock (mirrors `LedgerContext`).
	fn wallet_for_seed<'a>(
		wallets: &'a mut HashMap<WalletSeed, Wallet<D>>,
		seed: &WalletSeed,
	) -> &'a mut Wallet<D> {
		wallets.get_mut(seed).unwrap_or_else(|| {
			panic!("Wallet with seed {seed:?} does not exist in the `IndexerContext`")
		})
	}
}

impl IndexerContext<DefaultDB> {
	/// Connect to the indexer and sync each seed's wallet to the chain tip.
	///
	/// Seeds are synced concurrently, and within each seed the shielded / unshielded / dust
	/// subscriptions are drained concurrently too: the work is network-bound (indexer WS round-trips
	/// and server-side scanning), so overlapping the waits collapses the wall-clock time from the
	/// sum of the three streams down to roughly the slowest one. `IndexerClient` supports this —
	/// every method takes `&self`, uses a cloneable `reqwest::Client`, and opens a fresh independent
	/// WebSocket per subscription. A background ticker logs a progress heartbeat every
	/// [`PROGRESS_LOG_INTERVAL`] while the drain runs.
	///
	/// Each seed derives the Zswap viewing key, opens a session (`connect`), drains the
	/// subscriptions applying their updates, then releases the session (`disconnect`). The resulting
	/// synced [`Wallet`]s and their unshielded UTXOs are stored for the [`BuilderContext`] reads.
	/// There is no toolkit-side cache in this PR — each call re-drains to tip (fast, since the
	/// indexer pre-filters relevant transactions).
	pub async fn init_wallets(&self, seeds: &[WalletSeed]) -> Result<(), BoxError> {
		let block = self.client.latest_block().await?;
		// The indexer serves the chain's `LedgerParameters` as a tagged blob; fall back to the
		// network defaults if it cannot be decoded so dust syncing can still proceed.
		let params: LedgerParameters =
			deserialize(&block.ledger_parameters[..]).unwrap_or_else(|e| {
				log::warn!("indexer: could not decode ledger parameters ({e}); using defaults");
				(*LedgerState::<DefaultDB>::new(self.network_id.clone()).parameters).clone()
			});
		let tip_time = Timestamp::from_secs(block.timestamp);

		let progress = SyncProgress { wallets: seeds.len(), ..Default::default() };

		// Drain every seed (and its three streams) concurrently, racing the work against the
		// progress ticker. `log_until_done` loops forever; it is dropped the moment the work arm
		// resolves (on success or the first error), so the `select!` yields the work's result.
		let synced = {
			let work = try_join_all(
				seeds.iter().map(|seed| self.sync_wallet(seed, &params, tip_time, &progress)),
			);
			tokio::select! {
				result = work => result?,
				_ = progress.log_until_done() => unreachable!("progress ticker loops until dropped"),
			}
		};

		// Locks are taken only for the quick inserts, never held across the network work above.
		let mut wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
		let mut unshielded =
			self.unshielded.lock().expect("IndexerContext unshielded lock poisoned");
		for (seed, wallet, utxos) in synced {
			wallets.insert(seed.clone(), wallet);
			unshielded.insert(seed, utxos);
		}

		Ok(())
	}

	/// Build the wallet for `seed` and sync its shielded / unshielded / dust streams concurrently.
	///
	/// The three sub-syncs borrow disjoint fields of the freshly-built [`Wallet`]
	/// (`&mut shielded`, `&unshielded`, `&mut dust`), so [`tokio::try_join!`] can overlap their
	/// network waits on this single task. Returns the seed, the synced wallet, and its reconciled
	/// unshielded UTXOs for the caller to store.
	async fn sync_wallet(
		&self,
		seed: &WalletSeed,
		params: &LedgerParameters,
		tip_time: Timestamp,
		progress: &SyncProgress,
	) -> Result<(WalletSeed, Wallet<DefaultDB>, Vec<(Utxo, Timestamp)>), BoxError> {
		let mut wallet = Wallet {
			root_seed: Some(seed.clone()),
			shielded: ShieldedWallet::default(seed.clone()),
			unshielded: UnshieldedWallet::default(seed.clone()),
			dust: DustWallet::default(seed.clone(), Some(params)),
		};

		// Disjoint field borrows are what let the three sub-syncs run concurrently on one task.
		let Wallet { shielded, unshielded, dust, .. } = &mut wallet;
		let (_, unshielded_utxos, _) = tokio::try_join!(
			self.sync_shielded(shielded, progress),
			self.sync_unshielded(unshielded, progress),
			self.sync_dust(dust, tip_time, progress),
		)?;

		Ok((seed.clone(), wallet, unshielded_utxos))
	}

	/// Drain `shieldedTransactions`, fast-forwarding the wallet's zswap merkle tree with each
	/// gap-filling collapsed update and applying every relevant transaction's offers, until the
	/// indexer reports it has checked all known state for this wallet.
	async fn sync_shielded(
		&self,
		shielded: &mut ShieldedWallet<DefaultDB>,
		progress: &SyncProgress,
	) -> Result<(), BoxError> {
		let viewing_key = shielded.viewing_key(&self.network_id);
		let session_id = self.client.connect(&viewing_key, None).await?;

		let result = self.drain_shielded(shielded, &session_id, progress).await;

		// Always release the session, even on error, to avoid leaking indexer sessions.
		if let Err(e) = self.client.disconnect(&session_id).await {
			log::warn!("indexer: disconnect failed: {e}");
		}
		result?;
		progress.shielded.finish();
		Ok(())
	}

	async fn drain_shielded(
		&self,
		shielded: &mut ShieldedWallet<DefaultDB>,
		session_id: &str,
		progress: &SyncProgress,
	) -> Result<(), BoxError> {
		let mut stream = self.client.shielded_transactions(session_id, 0).await?;
		let mut last_scanned = 0u64;
		let mut last_target = 0u64;
		loop {
			let event = match timeout(PROGRESS_IDLE_TIMEOUT, stream.next()).await {
				Ok(Some(Ok(event))) => event,
				Ok(Some(Err(e))) => return Err(e.into()),
				Ok(None) => break,
				Err(_) => {
					// Past the 30s progress heartbeat: the connection is stalled, not drained.
					log::warn!("indexer: shielded subscription stalled (no progress heartbeat)");
					break;
				},
			};

			match event {
				ShieldedEvent::Relevant { raw_transaction, result, collapsed_update, .. } => {
					if let Some(update_bytes) = collapsed_update {
						let update: MerkleTreeCollapsedUpdate = deserialize(&update_bytes[..])?;
						shielded.state = shielded
							.state
							.apply_collapsed_update(&update)
							.map_err(|e| format!("apply zswap collapsed update: {e:?}"))?;
					}

					let tx: MnTx = deserialize(&raw_transaction[..])?;
					let offers = relevant_offers(&tx, result);
					shielded.apply_offers(&offers);
				},
				ShieldedEvent::Progress {
					highest_end_index, highest_checked_end_index, ..
				} => {
					progress.shielded.advance_scanned(&mut last_scanned, highest_checked_end_index);
					progress.shielded.advance_target(&mut last_target, highest_end_index);
					// Caught up once the indexer has checked every known output for relevance.
					if highest_checked_end_index >= highest_end_index {
						break;
					}
				},
			}
		}
		Ok(())
	}

	/// Drain `unshieldedTransactions` for the wallet's address, reconciling created vs spent UTXOs.
	async fn sync_unshielded(
		&self,
		unshielded: &UnshieldedWallet,
		progress: &SyncProgress,
	) -> Result<Vec<(Utxo, Timestamp)>, BoxError> {
		let address = unshielded.address(&self.network_id).to_bech32();
		let mut stream = self.client.unshielded_transactions(&address, 0).await?;

		// Keyed by (intent_hash, output_index) so a later spend removes the matching created UTXO.
		let mut utxos: HashMap<(Vec<u8>, u32), (Utxo, Timestamp)> = HashMap::new();
		// The indexer merges the backlog with a progress heartbeat whose first tick is immediate,
		// so a progress sentinel (carrying `highest_transaction_id`) usually arrives *before* the
		// backlog is fully streamed. Track the highest transaction id we've applied and stop once it
		// reaches that target (mirrors the shielded `highest_checked >= highest` guard). We test the
		// target in both branches: in `Transaction`, so a backlog that catches up to an already-known
		// target stops immediately; and in `Progress`, as the fallback for when the target isn't known
		// yet (or later grows) — without the `Transaction`-branch check a drained wallet would idle
		// until the next 30s heartbeat. Ids are 1-based, so an address with no transactions reports
		// `highest_transaction_id == 0` and we stop on the first progress.
		let mut highest_applied_transaction_id = 0u64;
		let mut last_scanned = 0u64;
		let mut last_target = 0u64;
		loop {
			let event = match timeout(PROGRESS_IDLE_TIMEOUT, stream.next()).await {
				Ok(Some(Ok(event))) => event,
				Ok(Some(Err(e))) => return Err(e.into()),
				Ok(None) => break,
				Err(_) => {
					// Past the 30s progress heartbeat: the connection is stalled, not drained.
					log::warn!("indexer: unshielded subscription stalled (no progress heartbeat)");
					break;
				},
			};

			match event {
				UnshieldedEvent::Transaction { transaction_id, created, spent } => {
					highest_applied_transaction_id =
						highest_applied_transaction_id.max(transaction_id);
					progress
						.unshielded
						.advance_scanned(&mut last_scanned, highest_applied_transaction_id);
					for u in &created {
						let utxo = build_utxo(u, unshielded.user_address)?;
						let ctime = Timestamp::from_secs(u.ctime.unwrap_or(0));
						utxos.insert((u.intent_hash.clone(), u.output_index), (utxo, ctime));
					}
					for u in &spent {
						utxos.remove(&(u.intent_hash.clone(), u.output_index));
					}
					// Backlog has caught up to a target an earlier heartbeat already reported: stop
					// now instead of idling until the next 30s heartbeat re-runs the check below.
					if last_target > 0 && highest_applied_transaction_id >= last_target {
						break;
					}
				},
				// Caught up once we've applied every transaction the indexer knows for this address.
				UnshieldedEvent::Progress { highest_transaction_id } => {
					progress.unshielded.advance_target(&mut last_target, highest_transaction_id);
					if highest_applied_transaction_id >= highest_transaction_id {
						break;
					}
				},
			}
		}

		let mut utxos: Vec<(Utxo, Timestamp)> = utxos.into_values().collect();
		utxos.sort_by(|a, b| a.0.cmp(&b.0));
		progress.unshielded.finish();
		Ok(utxos)
	}

	/// Drain `dustLedgerEvents` and replay them into the wallet's dust state, then process TTLs up
	/// to the chain tip. The events are the chain-wide ledger events; `replay_events` filters them
	/// to this wallet by secret key, exactly as the local replay path does.
	async fn sync_dust(
		&self,
		dust: &mut DustWallet<DefaultDB>,
		tip_time: Timestamp,
		progress: &SyncProgress,
	) -> Result<(), BoxError> {
		let mut stream = self.client.dust_ledger_events(1).await?;
		let mut events: Vec<Event<DefaultDB>> = Vec::new();
		let mut last_scanned = 0u64;
		let mut last_target = 0u64;
		loop {
			let item = match timeout(DUST_IDLE_TIMEOUT, stream.next()).await {
				Ok(Some(Ok(item))) => item,
				Ok(Some(Err(e))) => return Err(e.into()),
				Ok(None) => break,
				Err(_) => break,
			};
			progress.dust.advance_scanned(&mut last_scanned, item.id);
			progress.dust.advance_target(&mut last_target, item.max_id);
			let event: Event<DefaultDB> = deserialize(&item.raw[..])?;
			events.push(event);
			// `id`/`maxId` are 1-based and `maxId` is the highest known event: stop once reached.
			if item.id >= item.max_id {
				break;
			}
		}

		dust.replay_events(&events).map_err(|e| format!("replay dust events: {e:?}"))?;
		dust.process_ttls(tip_time);
		progress.dust.finish();
		Ok(())
	}
}

/// Shared, thread-safe sync-progress counters for one [`IndexerContext::init_wallets`] run.
///
/// Every stream folds its own monotonically-increasing frontier into these aggregate totals (see
/// [`StreamProgress::advance_scanned`]), so the numbers sum across all concurrent seeds without any
/// per-wallet bookkeeping. [`log_until_done`](Self::log_until_done) snapshots them on a timer.
#[derive(Default)]
struct SyncProgress {
	/// Number of wallets (seeds) being synced; `3 * wallets` is the total stream count.
	wallets: usize,
	shielded: StreamProgress,
	unshielded: StreamProgress,
	dust: StreamProgress,
}

/// Aggregated frontier for one stream kind (shielded / unshielded / dust) across all seeds.
#[derive(Default)]
struct StreamProgress {
	/// Sum of each seed's latest scanned frontier.
	scanned: AtomicU64,
	/// Sum of each seed's latest reported target (0 for a stream until the indexer reports one).
	target: AtomicU64,
	/// Number of this stream's seeds that have finished draining.
	done: AtomicU64,
}

impl StreamProgress {
	/// Fold a newly-observed scanned frontier into the shared total.
	///
	/// `last` is this stream's previously-contributed value; only the positive delta `now - *last`
	/// is added, so repeated reports and concurrent seeds sum correctly and never double-count.
	/// Non-increasing values are ignored (the frontiers are monotonic).
	fn advance_scanned(&self, last: &mut u64, now: u64) {
		if now > *last {
			self.scanned.fetch_add(now - *last, Ordering::Relaxed);
			*last = now;
		}
	}

	/// As [`advance_scanned`](Self::advance_scanned), but for the target frontier.
	fn advance_target(&self, last: &mut u64, now: u64) {
		if now > *last {
			self.target.fetch_add(now - *last, Ordering::Relaxed);
			*last = now;
		}
	}

	/// Mark one seed's stream of this kind as fully drained.
	fn finish(&self) {
		self.done.fetch_add(1, Ordering::Relaxed);
	}

	/// Render `scanned/target`, showing `…` for the target until the indexer reports one.
	fn display(&self) -> String {
		let scanned = self.scanned.load(Ordering::Relaxed);
		let target = self.target.load(Ordering::Relaxed);
		if target == 0 { format!("{scanned}/…") } else { format!("{scanned}/{target}") }
	}
}

impl SyncProgress {
	/// Log a one-line progress heartbeat every [`PROGRESS_LOG_INTERVAL`] until the future is
	/// dropped. Loops forever by design — the caller races it against the sync work in a `select!`
	/// and drops it once the work finishes.
	async fn log_until_done(&self) {
		let mut ticker = tokio::time::interval(PROGRESS_LOG_INTERVAL);
		// The first `interval` tick is immediate; skip it so the first line lands one interval in
		// (and fast syncs that finish under `PROGRESS_LOG_INTERVAL` log nothing at all).
		ticker.tick().await;
		loop {
			ticker.tick().await;
			let n = self.wallets;
			let streams = 3 * n;
			let done = self.shielded.done.load(Ordering::Relaxed)
				+ self.unshielded.done.load(Ordering::Relaxed)
				+ self.dust.done.load(Ordering::Relaxed);
			// `info` under the toolkit's log namespace: the default toolkit filter shows
			// `midnight_node_toolkit=info` but only `warn` for other crates, so this target keeps
			// the heartbeat visible by default without promoting it to a (misleading) warning.
			log::info!(
				target: "midnight_node_toolkit::indexer",
				"indexer: syncing {n} wallet(s) — shielded {}, unshielded {}, dust {} \
				 ({done}/{streams} streams caught up)",
				self.shielded.display(),
				self.unshielded.display(),
				self.dust.display(),
			);
		}
	}
}

/// The latest-version midnight transaction, matching `fork::apply_block_9`.
type MnTx = Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

/// Extract the zswap offers to apply for a relevant transaction, honouring its result.
///
/// Mirrors `LedgerContext::successful_shielded_offers`. On `Success` the guaranteed offer plus all
/// fallible offers are applied; on `PartialSuccess` only the guaranteed offer is applied (the
/// indexer's union does not carry per-segment success here, and the next transaction's collapsed
/// update re-aligns the merkle index regardless); on `Failure` nothing is applied.
fn relevant_offers<S, P>(
	tx: &Transaction<S, P, PureGeneratorPedersen, DefaultDB>,
	result: TransactionResultKind,
) -> Vec<Offer<P::LatestProof, DefaultDB>>
where
	S: SignatureKind<DefaultDB>,
	P: ProofKind<DefaultDB>,
{
	if matches!(result, TransactionResultKind::Failure) {
		return vec![];
	}
	let Transaction::Standard(stx) = tx else {
		return vec![];
	};
	let mut offers = vec![];
	if let Some(guaranteed) = &stx.guaranteed_coins {
		offers.push((**guaranteed).clone());
	}
	if matches!(result, TransactionResultKind::Success) {
		for entry in stx.fallible_coins.iter() {
			let fallible = &entry.1;
			offers.push((**fallible).clone());
		}
	}
	offers
}

/// Build a [`Utxo`] from an indexer `UnshieldedUtxo`. The owner is the queried wallet's address.
fn build_utxo(u: &UnshieldedUtxoData, owner: super::super::UserAddress) -> Result<Utxo, BoxError> {
	Ok(Utxo {
		value: u.value,
		owner,
		type_: UnshieldedTokenType(HashOutput(to_hash(&u.token_type)?)),
		intent_hash: IntentHash(HashOutput(to_hash(&u.intent_hash)?)),
		output_no: u.output_index,
	})
}

fn to_hash(bytes: &[u8]) -> Result<[u8; 32], BoxError> {
	bytes
		.try_into()
		.map_err(|_| format!("expected 32-byte hash, got {} bytes", bytes.len()).into())
}

#[async_trait]
impl<D: DB + Clone> BuilderContext<D> for IndexerContext<D> {
	fn with_wallet_from_seed<F, R>(&self, seed: WalletSeed, f: F) -> R
	where
		F: FnOnce(&mut Wallet<D>) -> R,
	{
		let mut wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
		let wallet = Self::wallet_for_seed(&mut wallets, &seed);
		f(wallet)
	}

	fn with_wallets_from_seeds<F, R>(
		&self,
		origin_seed: WalletSeed,
		destination_seed: WalletSeed,
		f: F,
	) -> R
	where
		F: FnOnce(&mut Wallet<D>, &mut Wallet<D>) -> R,
	{
		assert!(
			origin_seed != destination_seed,
			"with_wallets_from_seeds: origin_seed and destination_seed must differ \
			 (cannot produce two disjoint &mut to the same wallet)"
		);

		let mut wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
		let [origin_opt, destination_opt] =
			wallets.get_disjoint_mut([&origin_seed, &destination_seed]);
		let origin = origin_opt.unwrap_or_else(|| {
			panic!("Wallet with seed {origin_seed:?} does not exist in the `IndexerContext`")
		});
		let destination = destination_opt.unwrap_or_else(|| {
			panic!("Wallet with seed {destination_seed:?} does not exist in the `IndexerContext`")
		});
		f(origin, destination)
	}

	async fn latest_block_context(&self) -> BlockContext {
		todo!("indexer: R6 — block() query (PR #2, transaction building)")
	}

	async fn ledger_parameters(&self) -> LedgerParameters {
		todo!("indexer: R1 — Block.ledgerParameters blob (PR #2, transaction building)")
	}

	async fn network_id(&self) -> String {
		self.network_id.clone()
	}

	async fn unshielded_utxos(&self, seed: WalletSeed) -> Vec<(Utxo, Timestamp)> {
		self.unshielded
			.lock()
			.expect("IndexerContext unshielded lock poisoned")
			.get(&seed)
			.cloned()
			.unwrap_or_else(|| {
				panic!("Unshielded UTXOs for seed {seed:?} not synced in the `IndexerContext`")
			})
	}

	async fn zswap_state(&self) -> ZswapChainState<D> {
		todo!("indexer: R4 — merkle update stream (PR #2, transaction building)")
	}

	async fn contract_state(&self, _address: ContractAddress) -> Option<ContractState<D>> {
		todo!("indexer: R5 — contractAction(address).state blob (PR #2, transaction building)")
	}

	async fn resolver(&self) -> &'static Resolver {
		todo!("indexer: client-side resolver (PR #2, transaction building)")
	}

	async fn update_resolver(&self, _resolver: &'static Resolver) {
		todo!("indexer: client-side resolver (PR #2, transaction building)")
	}

	fn well_formed<S, P, B>(
		&self,
		_tx: &Transaction<S, P, B, D>,
		_now: Timestamp,
	) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
	where
		S: SignatureKind<D>,
		P: ProofKind<D> + Storable<D>,
		B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D> + Tagged,
	{
		// R7: an indexer has no full LedgerState to validate against; the node re-validates on
		// submission, so the indexer-backed builder treats the tx as well-formed here.
		todo!("indexer: R7 — no local state; node re-validates on submit (PR #2)")
	}
}

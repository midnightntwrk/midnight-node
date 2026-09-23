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
//! GraphQL API instead of replaying every block into a local [`crate::ledger_9::LedgerState`]
//! (see issue #1186).
//!
//! Only the read-only `show-wallet` path is implemented: [`IndexerContext::init_wallets`] drains
//! the shielded / unshielded / dust subscriptions to the chain tip, and the three
//! [`BuilderContext`] methods that reads need serve that synced state. The methods used only when
//! *building* transactions are `todo!()`.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::stream::{self, StreamExt, TryStreamExt};
use tokio::time::{Instant, sleep_until, timeout};

use super::BuilderContext;
use crate::indexer_client::{
	DUST_IDLE_TIMEOUT, IndexerClient, IndexerClientError, PROGRESS_IDLE_TIMEOUT,
	SHIELDED_PROGRESS_PROBE_INTERVAL, ShieldedCatchUp, ShieldedEvent, SyncProgress,
	TransactionResultKind, UnshieldedEvent, UnshieldedUtxoData,
};
use crate::ledger_9::{
	BindingKind, BlockContext, ContractAddress, ContractState, DB, DefaultDB, DustWallet, Event,
	HashOutput, IntentHash, IntoWalletAddress, LedgerParameters, LedgerState,
	MerkleTreeCollapsedUpdate, Offer, PedersenDowngradeable, ProofKind, ProofMarker,
	PureGeneratorPedersen, Resolver, Serializable, ShieldedWallet, Signature, SignatureKind,
	Storable, Tagged, Timestamp, Transaction, UnshieldedTokenType, UnshieldedWallet, Utxo, Wallet,
	WalletSeed, ZswapChainState, deserialize,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// An indexer-backed [`BuilderContext`].
///
/// Holds the synced wallet state keyed by seed. The wallet map mirrors
/// [`super::LedgerContext`]'s, including the `get_disjoint_mut` guard in
/// [`with_wallets_from_seeds`](BuilderContext::with_wallets_from_seeds).
pub struct IndexerContext<D: DB + Clone> {
	client: IndexerClient,
	/// Network id used to derive viewing keys / addresses (the indexer has no network field).
	network_id: String,
	/// How many wallets [`IndexerContext::init_wallets`] drains at once.
	wallet_sync_concurrency: NonZeroUsize,
	/// Synced wallets, populated by [`IndexerContext::init_wallets`].
	wallets: Mutex<HashMap<WalletSeed, Wallet<D>>>,
	/// Synced unshielded UTXOs per seed (created-minus-spent), with creation time.
	unshielded: Mutex<HashMap<WalletSeed, Vec<(Utxo, Timestamp)>>>,
}

impl<D: DB + Clone> IndexerContext<D> {
	/// Build a context targeting `indexer_url`, an `api/v4` base such as
	/// `http://127.0.0.1:8088/api/v4`. `network_id` (e.g. `undeployed`) derives viewing keys and
	/// addresses; `wallet_sync_concurrency` caps the seed fan-out in
	/// [`init_wallets`](Self::init_wallets).
	pub fn new(
		indexer_url: &str,
		network_id: impl Into<String>,
		wallet_sync_concurrency: NonZeroUsize,
	) -> Result<Self, IndexerClientError> {
		Ok(Self {
			client: IndexerClient::new(indexer_url)?,
			network_id: network_id.into(),
			wallet_sync_concurrency,
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
	/// Seeds sync `wallet_sync_concurrency` at a time, and each seed's shielded / unshielded / dust
	/// subscriptions drain concurrently, each on its own WebSocket.
	///
	/// No toolkit-side cache: each call re-drains to tip.
	pub async fn init_wallets(&self, seeds: &[WalletSeed]) -> Result<(), BoxError> {
		let block = self.client.latest_block().await?;
		// Fall back to network defaults if the blob won't decode, so dust syncing still proceeds.
		let params: LedgerParameters =
			deserialize(&block.ledger_parameters[..]).unwrap_or_else(|e| {
				log::warn!("indexer: could not decode ledger parameters ({e}); using defaults");
				(*LedgerState::<DefaultDB>::new(self.network_id.clone()).parameters).clone()
			});
		let tip_time = Timestamp::from_secs(block.timestamp);

		let progress = SyncProgress { wallets: seeds.len(), ..Default::default() };

		// `log_until_done` loops forever; it is dropped the moment the work arm resolves, so the
		// `select!` yields the work's result.
		let synced = {
			let work = stream::iter(
				seeds.iter().map(|seed| self.sync_wallet(seed, &params, tip_time, &progress)),
			)
			.buffer_unordered(self.wallet_sync_concurrency.get())
			.try_collect::<Vec<_>>();
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

	/// Stops per [`ShieldedCatchUp`], probing for fresh progress while the heartbeat is too slow.
	async fn drain_shielded(
		&self,
		shielded: &mut ShieldedWallet<DefaultDB>,
		session_id: &str,
		progress: &SyncProgress,
	) -> Result<(), BoxError> {
		let start_index = 0;
		let mut stream = self.client.shielded_transactions(session_id, start_index).await?;
		let mut catch_up = ShieldedCatchUp::new(start_index);
		let mut first_tick = true;
		let mut highest = start_index;
		let mut next_probe = Instant::now() + SHIELDED_PROGRESS_PROBE_INTERVAL;
		let mut stall_at = Instant::now() + PROGRESS_IDLE_TIMEOUT;
		let mut last_scanned = 0u64;
		let mut last_target = 0u64;
		loop {
			// The indexer caches progress per wallet across sessions, so only the subscription's
			// first tick can predate this session; later ticks and probes are past the cache TTL.
			let (event, fresh) = tokio::select! {
				event = stream.next() => match event {
					Some(Ok(event)) => {
						stall_at = Instant::now() + PROGRESS_IDLE_TIMEOUT;
						let fresh = !(matches!(event, ShieldedEvent::Progress { .. })
							&& std::mem::take(&mut first_tick));
						(event, fresh)
					},
					Some(Err(e)) => return Err(e.into()),
					None => break,
				},
				_ = sleep_until(next_probe) => {
					next_probe = Instant::now() + SHIELDED_PROGRESS_PROBE_INTERVAL;
					let probe = timeout(
						SHIELDED_PROGRESS_PROBE_INTERVAL,
						self.client.shielded_progress(session_id, highest),
					);
					match probe.await {
						Ok(Ok(Some(event))) => (event, true),
						Ok(Ok(None)) => continue,
						Ok(Err(e)) => {
							log::debug!("indexer: shielded progress probe failed: {e}");
							continue;
						},
						Err(_) => {
							log::debug!("indexer: shielded progress probe timed out");
							continue;
						},
					}
				},
				_ = sleep_until(stall_at) => {
					// Past the 30s progress heartbeat: the connection is stalled, not drained.
					log::warn!("indexer: shielded subscription stalled (no progress heartbeat)");
					break;
				},
			};

			let done = match event {
				ShieldedEvent::Relevant {
					raw_transaction,
					result,
					zswap_end_index,
					collapsed_update,
					..
				} => {
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
					catch_up.relevant(zswap_end_index)
				},
				ShieldedEvent::Progress {
					highest_end_index,
					highest_checked_end_index,
					highest_relevant_end_index,
				} => {
					highest = highest.max(highest_end_index);
					next_probe = Instant::now() + SHIELDED_PROGRESS_PROBE_INTERVAL;
					progress.shielded.advance_scanned(&mut last_scanned, highest_checked_end_index);
					progress.shielded.advance_target(&mut last_target, highest_end_index);
					catch_up.progress(
						highest_end_index,
						highest_checked_end_index,
						highest_relevant_end_index,
						fresh,
					)
				},
			};
			if done {
				break;
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
		// The heartbeat's first tick is immediate, so its `highest_transaction_id` usually arrives
		// *before* the backlog finishes. Stop once the highest applied id reaches it (checked in
		// both arms). Ids are 1-based: an address with no transactions reports 0.
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

/// The midnight transaction for this ledger generation, matching `fork::apply_block_9`.
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

	async fn backs_dust_generation(&self, _utxo: &Utxo) -> bool {
		todo!("indexer: dust generation status for a UTXO")
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
		// An indexer has no full LedgerState to validate against; the node re-validates on
		// submission, so the builder treats the tx as well-formed here.
		todo!("indexer: R7 — no local state; node re-validates on submit (PR #2)")
	}
}

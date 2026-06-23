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
use std::time::Duration;

use async_trait::async_trait;
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
	/// For each seed this: derives the Zswap viewing key, opens a session (`connect`), drains the
	/// shielded / unshielded / dust subscriptions applying their updates, then releases the session
	/// (`disconnect`). The resulting synced [`Wallet`] and its unshielded UTXOs are stored for the
	/// [`BuilderContext`] reads. There is no toolkit-side cache in this PR — each call re-drains to
	/// tip (fast, since the indexer pre-filters relevant transactions).
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

		for seed in seeds {
			let mut wallet = Wallet {
				root_seed: Some(seed.clone()),
				shielded: ShieldedWallet::default(seed.clone()),
				unshielded: UnshieldedWallet::default(seed.clone()),
				dust: DustWallet::default(seed.clone(), Some(&params)),
			};

			self.sync_shielded(&mut wallet).await?;
			let unshielded_utxos = self.sync_unshielded(&wallet).await?;
			self.sync_dust(&mut wallet, tip_time).await?;

			self.wallets
				.lock()
				.expect("IndexerContext wallets lock poisoned")
				.insert(seed.clone(), wallet);
			self.unshielded
				.lock()
				.expect("IndexerContext unshielded lock poisoned")
				.insert(seed.clone(), unshielded_utxos);
		}

		Ok(())
	}

	/// Drain `shieldedTransactions`, fast-forwarding the wallet's zswap merkle tree with each
	/// gap-filling collapsed update and applying every relevant transaction's offers, until the
	/// indexer reports it has checked all known state for this wallet.
	async fn sync_shielded(&self, wallet: &mut Wallet<DefaultDB>) -> Result<(), BoxError> {
		let viewing_key = wallet.shielded.viewing_key(&self.network_id);
		let session_id = self.client.connect(&viewing_key, None).await?;

		let result = self.drain_shielded(wallet, &session_id).await;

		// Always release the session, even on error, to avoid leaking indexer sessions.
		if let Err(e) = self.client.disconnect(&session_id).await {
			log::warn!("indexer: disconnect failed: {e}");
		}
		result
	}

	async fn drain_shielded(
		&self,
		wallet: &mut Wallet<DefaultDB>,
		session_id: &str,
	) -> Result<(), BoxError> {
		let mut stream = self.client.shielded_transactions(session_id, 0).await?;
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
						wallet.shielded.state = wallet
							.shielded
							.state
							.apply_collapsed_update(&update)
							.map_err(|e| format!("apply zswap collapsed update: {e:?}"))?;
					}

					let tx: MnTx = deserialize(&raw_transaction[..])?;
					let offers = relevant_offers(&tx, result);
					wallet.update_state_from_offers(&offers);
				},
				ShieldedEvent::Progress {
					highest_end_index, highest_checked_end_index, ..
				} => {
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
		wallet: &Wallet<DefaultDB>,
	) -> Result<Vec<(Utxo, Timestamp)>, BoxError> {
		let address = wallet.unshielded.address(&self.network_id).to_bech32();
		let mut stream = self.client.unshielded_transactions(&address, 0).await?;

		// Keyed by (intent_hash, output_index) so a later spend removes the matching created UTXO.
		let mut utxos: HashMap<(Vec<u8>, u32), (Utxo, Timestamp)> = HashMap::new();
		// The indexer merges the backlog with a progress heartbeat whose first tick is immediate,
		// so a progress sentinel can arrive *before* the backlog is fully streamed. Don't stop on
		// the first progress; instead track the highest transaction id we've applied and stop only
		// once it reaches the sentinel's `highest_transaction_id` (mirrors the shielded
		// `highest_checked >= highest` guard). Ids are 1-based, so an address with no transactions
		// reports `highest_transaction_id == 0` and we stop on the first progress.
		let mut highest_applied_transaction_id = 0u64;
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
					for u in &created {
						let utxo = build_utxo(u, wallet.unshielded.user_address)?;
						let ctime = Timestamp::from_secs(u.ctime.unwrap_or(0));
						utxos.insert((u.intent_hash.clone(), u.output_index), (utxo, ctime));
					}
					for u in &spent {
						utxos.remove(&(u.intent_hash.clone(), u.output_index));
					}
				},
				// Caught up once we've applied every transaction the indexer knows for this address.
				UnshieldedEvent::Progress { highest_transaction_id } => {
					if highest_applied_transaction_id >= highest_transaction_id {
						break;
					}
				},
			}
		}

		let mut utxos: Vec<(Utxo, Timestamp)> = utxos.into_values().collect();
		utxos.sort_by(|a, b| a.0.cmp(&b.0));
		Ok(utxos)
	}

	/// Drain `dustLedgerEvents` and replay them into the wallet's dust state, then process TTLs up
	/// to the chain tip. The events are the chain-wide ledger events; `replay_events` filters them
	/// to this wallet by secret key, exactly as the local replay path does.
	async fn sync_dust(
		&self,
		wallet: &mut Wallet<DefaultDB>,
		tip_time: Timestamp,
	) -> Result<(), BoxError> {
		let mut stream = self.client.dust_ledger_events(1).await?;
		let mut events: Vec<Event<DefaultDB>> = Vec::new();
		loop {
			let item = match timeout(DUST_IDLE_TIMEOUT, stream.next()).await {
				Ok(Some(Ok(item))) => item,
				Ok(Some(Err(e))) => return Err(e.into()),
				Ok(None) => break,
				Err(_) => break,
			};
			let event: Event<DefaultDB> = deserialize(&item.raw[..])?;
			events.push(event);
			// `id`/`maxId` are 1-based and `maxId` is the highest known event: stop once reached.
			if item.id >= item.max_id {
				break;
			}
		}

		wallet
			.update_dust_from_tx(&events)
			.map_err(|e| format!("replay dust events: {e:?}"))?;
		wallet.dust.process_ttls(tip_time);
		Ok(())
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

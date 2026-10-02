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
//! GraphQL API instead of replaying every block into a local [`crate::ledger_8::LedgerState`]
//! (see issue #1186).
//!
//! [`IndexerContext::init_wallets`] drains the shielded / unshielded / dust subscriptions to the
//! chain tip, and the wallet methods serve that synced state. Block context and ledger parameters
//! are served from one tip snapshot (see [`IndexerContext::refresh_tip`]); contract state is
//! queried from the indexer per call. [`BuilderContext::zswap_state`] and
//! [`BuilderContext::backs_dust_generation`] are still `todo!()`.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::stream::{self, StreamExt, TryStreamExt};
use tokio::time::{Instant, sleep_until, timeout};

use super::{BuilderContext, DEFAULT_RESOLVER};
use crate::indexer_client::{
	BlockInfo, DUST_IDLE_TIMEOUT, DustGenerationsEventData, DustGenerationsForm, DustSnapshot,
	IndexerClient, IndexerClientError, PROGRESS_IDLE_TIMEOUT, SHIELDED_PROGRESS_PROBE_INTERVAL,
	ShieldedCatchUp, ShieldedEvent, SyncProgress, TransactionResultKind, UnshieldedEvent,
	UnshieldedUtxoData, WalletSyncState,
};
use crate::ledger_8::mn_ledger::events::EventDetails;
use crate::ledger_8::{
	BindingKind, BlockContext, ContractAddress, ContractState, DB, DefaultDB, DustGenerationInfo,
	DustLocalState, DustNullifier, DustOutput, DustSecretKey, DustWallet, Event, HashOutput,
	InitialNonce, IntentHash, IntoWalletAddress, LedgerParameters, LedgerState,
	MerkleTreeCollapsedUpdate, Offer, PedersenDowngradeable, ProofKind, ProofMarker,
	PureGeneratorPedersen, QualifiedDustOutput, Resolver, SerdeTransaction, Serializable,
	ShieldedWallet, Signature, SignatureKind, Sp, Storable, Tagged, Timestamp, Transaction,
	UnshieldedTokenType, UnshieldedWallet, Utxo, Wallet, WalletSeed, WalletState, ZswapChainState,
	deserialize, deserialize_untagged, make_block_context, serialize_untagged,
};
use crate::{DustFrontierRaw, DustLocalStateRaw, UnshieldedUtxoRaw, ZswapWalletStateRaw};

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
	resolver: Mutex<&'static Resolver>,
	/// One build must read block context and ledger parameters from the same block, or a tx can
	/// pair one block's time with another's parameters.
	tip: Mutex<Option<BlockInfo>>,
	/// Rebuild DUST from a per-wallet indexer snapshot where the indexer supports it, rather than
	/// replaying the chain-wide dust event log.
	fast_sync: bool,
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
			resolver: Mutex::new(&DEFAULT_RESOLVER),
			tip: Mutex::new(None),
			fast_sync: true,
		})
	}

	/// Turn DUST fast sync off (it is on by default) to always replay the dust event log.
	pub fn with_fast_sync(mut self, enabled: bool) -> Self {
		self.fast_sync = enabled;
		self
	}

	/// Re-read the chain tip that [`BuilderContext::latest_block_context`] and
	/// [`BuilderContext::ledger_parameters`] serve. [`IndexerContext::init_wallets`] sets it too.
	pub async fn refresh_tip(&self) -> Result<BlockInfo, IndexerClientError> {
		let block = self.client.latest_block().await?;
		*self.tip.lock().expect("IndexerContext tip lock poisoned") = Some(block.clone());
		Ok(block)
	}

	async fn tip(&self) -> BlockInfo {
		let cached = self.tip.lock().expect("IndexerContext tip lock poisoned").clone();
		match cached {
			Some(block) => block,
			None => self.refresh_tip().await.expect("indexer: query latest block"),
		}
	}

	/// Serve `wallet` and its unshielded `utxos` for `seed`, as synced state.
	pub fn insert_wallet(
		&self,
		seed: WalletSeed,
		wallet: Wallet<D>,
		utxos: Vec<(Utxo, Timestamp)>,
	) {
		let mut wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
		let mut unshielded =
			self.unshielded.lock().expect("IndexerContext unshielded lock poisoned");
		wallets.insert(seed.clone(), wallet);
		unshielded.insert(seed, utxos);
	}

	/// Advance every wallet's zswap tree to a common index, at least the tip's.
	///
	/// A drain stops a tree at its wallet's last relevant tx; left there, pending outputs get
	/// indices the chain won't assign and are unspendable. Trees stay level after the first call.
	async fn fast_forward_shielded(&self) -> Result<(), BoxError> {
		let tip_end = self.tip().await.zswap_end_index;
		let starts: Vec<u64> = {
			let wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
			wallets.values().map(|w| w.shielded.state.first_free).collect()
		};
		let target = starts.iter().copied().fold(tip_end, u64::max);
		let mut updates = HashMap::new();
		for start in starts.into_iter().filter(|start| *start < target) {
			if let std::collections::hash_map::Entry::Vacant(entry) = updates.entry(start) {
				let bytes = self.client.zswap_collapsed_update(start, target - 1).await?;
				let update: MerkleTreeCollapsedUpdate = deserialize(&bytes[..])?;
				entry.insert(update);
			}
		}

		let mut wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
		for wallet in wallets.values_mut() {
			if let Some(update) = updates.get(&wallet.shielded.state.first_free) {
				wallet.shielded.state = wallet
					.shielded
					.state
					.apply_collapsed_update(update)
					.map_err(|e| format!("apply zswap collapsed update: {e:?}"))?;
			}
		}
		Ok(())
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
	/// `resume` carries each seed's previously-synced state, and the returned map is what to
	/// persist for the next run; a seed absent from `resume` drains from the origin. Nothing here
	/// checks that a resume entry belongs to this chain or ledger generation — the blobs do not
	/// self-identify, so the caller's cache key must guarantee it.
	pub async fn init_wallets(
		&self,
		seeds: &[WalletSeed],
		resume: &HashMap<WalletSeed, WalletSyncState>,
	) -> Result<HashMap<WalletSeed, WalletSyncState>, BoxError> {
		let block = self.refresh_tip().await?;
		// Fall back to network defaults if the blob won't decode, so dust syncing still proceeds.
		let params = LedgerParameters::try_from(&block).unwrap_or_else(|e| {
			log::warn!("indexer: could not decode ledger parameters ({e}); using defaults");
			(*LedgerState::<DefaultDB>::new(self.network_id.clone()).parameters).clone()
		});
		let tip_time = Timestamp::from_secs(block.timestamp);
		let fast_sync = self.dust_fast_sync_target(&block).await;

		let progress = SyncProgress { wallets: seeds.len(), ..Default::default() };

		// `log_until_done` loops forever; it is dropped the moment the work arm resolves, so the
		// `select!` yields the work's result.
		let synced = {
			let work = stream::iter(seeds.iter().map(|seed| {
				self.sync_wallet(
					seed,
					resume.get(seed),
					&params,
					tip_time,
					fast_sync.as_ref(),
					&progress,
				)
			}))
			.buffer_unordered(self.wallet_sync_concurrency.get())
			.try_collect::<Vec<_>>();
			tokio::select! {
				result = work => result?,
				_ = progress.log_until_done() => unreachable!("progress ticker loops until dropped"),
			}
		};

		// Locks are taken only for the quick inserts, never held across the network work above.
		let mut next_resume = HashMap::with_capacity(synced.len());
		for (seed, wallet, utxos, sync_state) in synced {
			self.insert_wallet(seed.clone(), wallet, utxos);
			next_resume.insert(seed, sync_state);
		}

		Ok(next_resume)
	}

	/// The indexer's `dustGenerations` form and the snapshot block (the tip) for DUST fast sync, or
	/// `None` to replay dust events.
	async fn dust_fast_sync_target(
		&self,
		tip: &BlockInfo,
	) -> Option<(DustGenerationsForm, DustSnapshot)> {
		if !self.fast_sync {
			return None;
		}
		let target = async {
			let Some(form) = self.client.dust_generations_form().await? else {
				return Ok(None);
			};
			Ok::<_, IndexerClientError>(Some((form, self.client.dust_snapshot(tip.height).await?)))
		};
		match target.await {
			Ok(None) => {
				log::warn!(
					"indexer: no DUST fast-sync support; replaying every dust event for each wallet \
					 (slow)"
				);
				None
			},
			Ok(target) => target,
			Err(e) => {
				log::warn!("indexer: DUST fast-sync probe failed ({e}); replaying dust events");
				None
			},
		}
	}

	/// Build the wallet for `seed` and sync its shielded / unshielded / dust streams concurrently.
	///
	/// The wallet is always rebuilt from the seed, so keys stay derived rather than persisted;
	/// `resume` only reseeds the three accumulated states.
	async fn sync_wallet(
		&self,
		seed: &WalletSeed,
		resume: Option<&WalletSyncState>,
		params: &LedgerParameters,
		tip_time: Timestamp,
		fast_sync: Option<&(DustGenerationsForm, DustSnapshot)>,
		progress: &SyncProgress,
	) -> Result<(WalletSeed, Wallet<DefaultDB>, Vec<(Utxo, Timestamp)>, WalletSyncState), BoxError>
	{
		let mut wallet = Wallet {
			root_seed: Some(seed.clone()),
			shielded: ShieldedWallet::default(seed.clone()),
			unshielded: UnshieldedWallet::default(seed.clone()),
			dust: DustWallet::default(seed.clone(), Some(params)),
		};

		let Wallet { shielded, unshielded, dust, .. } = &mut wallet;
		let (
			shielded_state,
			(unshielded_utxos, unshielded_tx_id),
			(dust_state, dust_event_id, dust_frontier),
		) = tokio::try_join!(
			self.sync_shielded(shielded, resume, progress),
			self.sync_unshielded(unshielded, resume, progress),
			self.sync_dust(dust, resume, tip_time, fast_sync, progress),
		)?;

		let utxo_blobs = unshielded_utxos
			.iter()
			.map(|(utxo, ctime)| {
				Ok(UnshieldedUtxoRaw {
					utxo: serialize_untagged(utxo)?,
					ctime_secs: ctime.to_secs(),
				})
			})
			.collect::<Result<Vec<_>, std::io::Error>>()?;

		let next = WalletSyncState {
			shielded_state,
			unshielded_utxos: utxo_blobs,
			unshielded_tx_id,
			dust_state,
			dust_event_id,
			dust_frontier,
		};
		Ok((seed.clone(), wallet, unshielded_utxos, next))
	}

	/// Drain `shieldedTransactions`, fast-forwarding the wallet's zswap merkle tree with each
	/// gap-filling collapsed update and applying every relevant transaction's offers, until the
	/// indexer reports it has checked all known state for this wallet.
	///
	/// Returns the serialized `WalletState` to persist, or `None` when the drain ended misaligned
	/// (see [`IndexerContext::drain_shielded`]).
	async fn sync_shielded(
		&self,
		shielded: &mut ShieldedWallet<DefaultDB>,
		resume: Option<&WalletSyncState>,
		progress: &SyncProgress,
	) -> Result<Option<ZswapWalletStateRaw>, BoxError> {
		if let Some(ZswapWalletStateRaw(bytes)) = resume.and_then(|r| r.shielded_state.as_ref()) {
			shielded.state = deserialize_untagged::<WalletState<DefaultDB>>(&bytes[..])?;
		}
		let viewing_key = shielded.viewing_key(&self.network_id);
		// `ConnectOptions.startIndex` is a *transaction id*, not a zswap index, and the indexer
		// upserts it as a MIN, so it can neither carry the resume cursor nor narrow an existing
		// server-side scan. The resume cursor goes to `shielded_transactions` instead.
		let session_id = self.client.connect(&viewing_key, None).await?;

		let result = self.drain_shielded(shielded, &session_id, progress).await;

		// Always release the session, even on error, to avoid leaking indexer sessions.
		if let Err(e) = self.client.disconnect(&session_id).await {
			log::warn!("indexer: disconnect failed: {e}");
		}
		let aligned = result?;
		progress.shielded.finish();
		if !aligned {
			log::debug!("indexer: shielded tail misaligned; not caching this wallet's zswap state");
			return Ok(None);
		}
		Ok(Some(ZswapWalletStateRaw(serialize_untagged(&shielded.state)?)))
	}

	/// Stops per [`ShieldedCatchUp`], probing for fresh progress while the heartbeat is too slow.
	///
	/// Returns whether the merkle tree ended aligned with the last relevant transaction's
	/// `zswapEndIndex` — i.e. whether `state.first_free` is a sound resume cursor.
	///
	/// `relevant_offers` applies only the guaranteed offer of a `PartialSuccess` and nothing of a
	/// `Failure`, so `first_free` can end *below* that transaction's `zswapEndIndex`. Resuming
	/// there re-delivers the same transaction (the server selects `zswap_start_index >= index`)
	/// with no collapsed update (it omits one whenever `index >= zswap_start_index`), and its
	/// outputs get applied onto the tree twice. The live drain tolerates the gap because the next
	/// transaction's collapsed update realigns the tree; a persisted cursor has no such rescue.
	//
	// ponytail: skip-persist on misalignment. If PartialSuccess tails turn out to be common,
	// realign with `zswapMerkleTreeCollapsedUpdate(first_free, last_end_index - 1)`.
	async fn drain_shielded(
		&self,
		shielded: &mut ShieldedWallet<DefaultDB>,
		session_id: &str,
		progress: &SyncProgress,
	) -> Result<bool, BoxError> {
		// The cursor is the tree's own next free index, so state and cursor cannot drift.
		let mut last_end_index = shielded.state.first_free;
		let mut stream = self.client.shielded_transactions(session_id, last_end_index).await?;
		let mut catch_up = ShieldedCatchUp::new(last_end_index);
		let mut first_tick = true;
		let mut highest = last_end_index;
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
					log::warn!(
						"indexer: shielded subscription stalled (no progress heartbeat); shielded \
						 coins may be incomplete"
					);
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
					last_end_index = zswap_end_index;
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
		Ok(shielded.state.first_free == last_end_index)
	}

	/// Drain `unshieldedTransactions` for the wallet's address, reconciling created vs spent UTXOs.
	///
	/// Returns the reconciled set and the highest applied `transactionId`. The fold is
	/// order-independent, so seeding it from a previous run is equivalent to replaying from the
	/// origin.
	async fn sync_unshielded(
		&self,
		unshielded: &UnshieldedWallet,
		resume: Option<&WalletSyncState>,
		progress: &SyncProgress,
	) -> Result<(Vec<(Utxo, Timestamp)>, u64), BoxError> {
		let address = unshielded.address(&self.network_id).to_bech32();

		// Keyed by (intent_hash, output_index) so a later spend removes the matching created UTXO.
		let mut utxos: HashMap<(Vec<u8>, u32), (Utxo, Timestamp)> = HashMap::new();
		// The heartbeat's first tick is immediate, so its `highest_transaction_id` usually arrives
		// *before* the backlog finishes. Stop once the highest applied id reaches it (checked in
		// both arms). Ids are 1-based: an address with no transactions reports 0.
		let mut highest_applied_transaction_id = 0u64;
		if let Some(r) = resume {
			for UnshieldedUtxoRaw { utxo, ctime_secs } in &r.unshielded_utxos {
				let utxo: Utxo = deserialize_untagged(&utxo[..])?;
				let key = (utxo.intent_hash.0.0.to_vec(), utxo.output_no);
				utxos.insert(key, (utxo, Timestamp::from_secs(*ctime_secs)));
			}
			highest_applied_transaction_id = r.unshielded_tx_id;
		}

		// The server's cursor is inclusive (`transaction_id >= $2`), so resume past the last
		// applied id. A cold run passes 1, which selects the same set as the 1-based ids' 0.
		let mut stream = self
			.client
			.unshielded_transactions(&address, highest_applied_transaction_id + 1)
			.await?;
		let mut last_scanned = 0u64;
		let mut last_target = 0u64;
		loop {
			let event = match timeout(PROGRESS_IDLE_TIMEOUT, stream.next()).await {
				Ok(Some(Ok(event))) => event,
				Ok(Some(Err(e))) => return Err(e.into()),
				Ok(None) => break,
				Err(_) => {
					// Past the 30s progress heartbeat: the connection is stalled, not drained.
					log::warn!(
						"indexer: unshielded subscription stalled (no progress heartbeat); \
						 unshielded UTXOs may be incomplete"
					);
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
		Ok((utxos, highest_applied_transaction_id))
	}

	/// Drain `dustLedgerEvents` and replay them into the wallet's dust state, then process TTLs up
	/// to the chain tip. The events are the chain-wide ledger events; `replay_events` filters them
	/// to this wallet by secret key, exactly as the local replay path does.
	///
	/// Returns the dust state as of the last applied event *before* `process_ttls`, plus that
	/// event's id. `replay_events` rejects a gap with `NonLinearInsertion`, so a wrong resume
	/// cursor fails loudly rather than producing a quietly wrong balance.
	///
	/// With `fast_sync`, the state is rebuilt by [`fast_sync_dust`](Self::fast_sync_dust) instead:
	/// resumed from the cached spend-chain frontier if there is one, else from scratch, and
	/// replayed only if both fail. A fast-synced state has no dust event id, so its frontier is
	/// returned for the cache instead.
	async fn sync_dust(
		&self,
		dust: &mut DustWallet<DefaultDB>,
		resume: Option<&WalletSyncState>,
		tip_time: Timestamp,
		fast_sync: Option<&(DustGenerationsForm, DustSnapshot)>,
		progress: &SyncProgress,
	) -> Result<(Option<DustLocalStateRaw>, u64, Option<DustFrontierRaw>), BoxError> {
		if let Some((form, snapshot)) = fast_sync {
			// An index-range snapshot can't be verified at a fixed block (see
			// `DustGenerationsForm`), so it neither resumes nor leaves a frontier.
			let resumable = *form == DustGenerationsForm::ByBlockHash;
			let mut synced = None;
			if let Some(frontier) =
				resume.and_then(|r| r.dust_frontier.as_ref()).filter(|_| resumable)
			{
				match self.fast_sync_dust(dust, *form, snapshot, Some(frontier), tip_time).await {
					Ok(state) => synced = Some(state),
					Err(e) => log::warn!(
						"indexer: resumed DUST fast sync failed ({e}); fast-syncing from scratch"
					),
				}
			}
			let synced = match synced {
				Some(state) => Ok(state),
				None => self.fast_sync_dust(dust, *form, snapshot, None, tip_time).await,
			};
			match synced {
				Ok((state, frontier)) => {
					dust.dust_local_state = Some(Sp::new(state));
					dust.process_ttls(tip_time);
					progress.dust.finish();
					return Ok((None, 0, resumable.then_some(frontier)));
				},
				Err(e) => log::warn!("indexer: DUST fast sync failed ({e}); replaying dust events"),
			}
		}

		let mut resume_id = 0u64;
		if let Some(r) = resume
			&& let Some(DustLocalStateRaw(bytes)) = &r.dust_state
		{
			let state = deserialize_untagged::<DustLocalState<DefaultDB>>(&bytes[..])?;
			dust.dust_local_state = Some(Sp::new(state));
			resume_id = r.dust_event_id;
		}

		// Resume *at* the last applied event, not past it. This stream has no progress heartbeat,
		// so a caught-up wallet subscribed at `id + 1` would receive nothing and stall for the
		// full `DUST_IDLE_TIMEOUT`; the re-delivered event carries `maxId`, which ends the drain
		// in one round trip. It is skipped below rather than replayed.
		let mut stream = self.client.dust_ledger_events(resume_id.max(1)).await?;
		let mut events: Vec<Event<DefaultDB>> = Vec::new();
		let mut applied_id = resume_id;
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
			if item.id > resume_id {
				let event: Event<DefaultDB> = deserialize(&item.raw[..])?;
				events.push(event);
				applied_id = item.id;
			}
			// `id`/`maxId` are 1-based and `maxId` is the highest known event: stop once reached.
			if item.id >= item.max_id {
				break;
			}
		}

		dust.replay_events(&events).map_err(|e| format!("replay dust events: {e:?}"))?;
		// Snapshot before the TTL projection: `process_ttls` expires UTXOs against *this* tip, so
		// persisting its output and resuming from it would expire against two different tips.
		let dust_state = dust
			.dust_local_state
			.as_ref()
			.map(|s| serialize_untagged(&**s).map(DustLocalStateRaw))
			.transpose()?;
		dust.process_ttls(tip_time);
		progress.dust.finish();
		Ok((dust_state, applied_id, None))
	}

	/// Rebuild the wallet's DUST state at `snapshot` from per-wallet indexer queries rather than
	/// the chain-wide event log: owned generations with the generation tree's gaps collapsed, each
	/// output's spend chain followed by nullifier, then the commitment tree around the unspent
	/// outputs. Returned, with its new frontier, only if both tree roots match the snapshot
	/// block's.
	///
	/// With a `frontier` from an earlier run, each chain resumes from its cached head, so only the
	/// spends made since are walked.
	async fn fast_sync_dust(
		&self,
		dust: &DustWallet<DefaultDB>,
		form: DustGenerationsForm,
		snapshot: &DustSnapshot,
		frontier: Option<&DustFrontierRaw>,
		tip_time: Timestamp,
	) -> Result<(DustLocalState<DefaultDB>, DustFrontierRaw), BoxError> {
		let (Some(sk), Some(local)) = (dust.secret_key(), dust.dust_local_state.as_ref()) else {
			return Err("watch-only dust wallet".into());
		};
		// Cached heads are searched for spends only after the frontier block, so a frontier block
		// this chain no longer has could hide their spends.
		if let Some(f) = frontier
			&& (f.height > snapshot.height
				|| self.client.block_hash_at(f.height).await? != Some(f.block_hash))
		{
			return Err(format!(
				"cached DUST frontier block {} is not on the indexer's chain up to block {}",
				f.height, snapshot.height
			)
			.into());
		}
		let mut state = DustLocalState::<DefaultDB>::new(local.params);
		let owner = dust.public_key;

		let address = dust.address(&self.network_id).to_bech32();
		let mut stream = self.client.dust_generations(form, &address, snapshot).await?;
		let mut entries = Vec::new();
		let mut dtimes = HashMap::new();
		let tail_update = loop {
			let event = timeout(PROGRESS_IDLE_TIMEOUT, stream.next())
				.await
				.map_err(|_| "dustGenerations stalled")?
				.ok_or("dustGenerations ended before its progress item")??;
			match event {
				DustGenerationsEventData::Entry(entry) => entries.push(entry),
				DustGenerationsEventData::DtimeUpdate { generation_mt_index, dtime_secs } => {
					dtimes.insert(generation_mt_index, Timestamp::from_secs(dtime_secs));
				},
				DustGenerationsEventData::Progress { collapsed_update } => break collapsed_update,
			}
		};

		let generations = entries.len();
		let mut first_outputs = Vec::with_capacity(generations);
		for entry in entries {
			if let Some(update) = &entry.collapsed_update {
				state = state
					.apply_generation_collapsed_update(&deserialize(&update[..])?)
					.map_err(|e| format!("apply dust generation update: {e:?}"))?;
			}
			let backing_night = InitialNonce(HashOutput(entry.backing_night));
			let info = DustGenerationInfo {
				value: entry.value,
				owner,
				nonce: backing_night,
				dtime: dtimes.get(&entry.generation_mt_index).copied().unwrap_or(Timestamp::MAX),
			};
			state = state
				.insert_generation_info(entry.generation_mt_index, info, Some(backing_night))
				.map_err(|e| format!("insert dust generation: {e:?}"))?;
			first_outputs.push(QualifiedDustOutput {
				initial_value: entry.initial_value,
				owner,
				nonce: dust_nonce(sk, backing_night, 0),
				seq: 0,
				ctime: Timestamp::from_secs(entry.ctime_secs),
				backing_night,
				mt_index: entry.commitment_mt_index,
			});
		}
		if let Some(update) = tail_update {
			state = state
				.apply_generation_collapsed_update(&deserialize(&update[..])?)
				.map_err(|e| format!("apply dust generation update: {e:?}"))?;
		}
		let mut heads = chain_starts(first_outputs, frontier)?;

		// Walk each output's spend chain, one `dustNullifierTransactions` call per round for all
		// current heads: a spent head is replaced by the change output its spend created, an
		// unspent head is final. A successor's nullifier commits to the value and ctime its spend
		// set, so a round can only look one spend further: rounds = the longest chain's length.
		let mut unspent = Vec::new();
		let (mut rounds, mut walked) = (0, 0);
		while !heads.is_empty() {
			rounds += 1;
			let by_nullifier: HashMap<Vec<u8>, (QualifiedDustOutput, u64)> = heads
				.drain(..)
				.map(|(qdo, from)| (nullifier_le(&qdo.nullifier(sk)), (qdo, from)))
				.collect();
			// Capped at the tip: a frontier taken there has nothing after it, and the indexer
			// rejects `fromBlock` > `toBlock`.
			let from_block = by_nullifier.values().map(|(_, from)| *from).min().unwrap_or(0);
			let mut spends = self
				.client
				.dust_nullifier_transactions(
					by_nullifier.keys().map(Vec::as_slice),
					from_block.min(snapshot.height),
					snapshot.height,
				)
				.await?;
			let mut successors = HashMap::new();
			while let Some(spend) = timeout(PROGRESS_IDLE_TIMEOUT, spends.next())
				.await
				.map_err(|_| "dustNullifierTransactions stalled")?
			{
				let spend = spend?;
				let Some((spent, _)) = by_nullifier.get(&spend.nullifier_le) else { continue };
				let next = spend_successor(&state, sk, spent, &spend.events)?;
				successors.insert(spend.nullifier_le, next);
			}
			walked += successors.len();
			for (nullifier, (qdo, from)) in by_nullifier {
				match successors.remove(&nullifier) {
					Some(next) => heads.push((next, from)),
					None => unspent.push(qdo),
				}
			}
		}

		unspent.sort_by_key(|qdo| qdo.mt_index);
		let mut first_free = 0;
		for qdo in &unspent {
			state = self.collapse_dust_commitments(state, first_free, qdo.mt_index).await?;
			state = state
				.insert_commitment(qdo.mt_index, *qdo, true)
				.map_err(|e| format!("insert dust commitment: {e:?}"))?;
			state = state
				.add_utxo(&qdo.nullifier(sk), qdo, None)
				.map_err(|e| format!("add dust utxo: {e:?}"))?;
			first_free = qdo.mt_index + 1;
		}
		state = self
			.collapse_dust_commitments(state, first_free, snapshot.commitment_end_index)
			.await?;

		let roots = [
			("commitment", state.commitment_tree.root(), &snapshot.commitment_root),
			("generation", state.generating_tree.root(), &snapshot.generation_root),
		];
		for (tree, root, expected) in roots {
			let root = root.ok_or_else(|| format!("dust {tree} tree is not rehashed"))?;
			if serialize_untagged(&root)? != *expected {
				return Err(
					format!("dust {tree} root differs from block {}'s", snapshot.height).into()
				);
			}
		}
		state.sync_time = tip_time;
		log::info!(
			"indexer: DUST fast sync at block {} ({}): {generations} generations, {walked} spends in \
			 {rounds} rounds, {} unspent outputs",
			snapshot.height,
			frontier.map_or("fresh".to_string(), |f| format!("resumed from block {}", f.height)),
			unspent.len(),
		);
		let next_frontier = DustFrontierRaw {
			height: snapshot.height,
			block_hash: snapshot.hash,
			heads: unspent.iter().map(serialize_untagged).collect::<Result<_, _>>()?,
		};
		Ok((state, next_frontier))
	}

	/// Fill the commitment tree over `start..end` with one collapsed update from the indexer.
	async fn collapse_dust_commitments(
		&self,
		state: DustLocalState<DefaultDB>,
		start: u64,
		end: u64,
	) -> Result<DustLocalState<DefaultDB>, BoxError> {
		if end <= start {
			return Ok(state);
		}
		let update = self.client.dust_commitment_collapsed_update(start, end - 1).await?;
		Ok(state
			.apply_commitment_collapsed_update(&deserialize(&update[..])?)
			.map_err(|e| format!("apply dust commitment update: {e:?}"))?)
	}
}

/// Where each generation's spend-chain walk starts, and the first block to search for its spends:
/// the frontier's head for that generation, unspent at the frontier block, or else the generation's
/// first output, from block 0.
fn chain_starts(
	first_outputs: Vec<QualifiedDustOutput>,
	frontier: Option<&DustFrontierRaw>,
) -> Result<Vec<(QualifiedDustOutput, u64)>, BoxError> {
	let mut cached = HashMap::new();
	let mut resume_from = 0;
	if let Some(f) = frontier {
		resume_from = f.height + 1;
		for raw in &f.heads {
			let head: QualifiedDustOutput = deserialize_untagged(&raw[..])?;
			cached.insert(head.backing_night.0.0, head);
		}
	}
	let starts = first_outputs
		.into_iter()
		.map(|first| match cached.remove(&first.backing_night.0.0) {
			Some(head) => (head, resume_from),
			None => (first, 0),
		})
		.collect();
	if !cached.is_empty() {
		return Err(
			format!("{} cached DUST outputs have no generation at the tip", cached.len()).into()
		);
	}
	Ok(starts)
}

/// The indexer's byte form of a nullifier.
fn nullifier_le(nullifier: &DustNullifier) -> Vec<u8> {
	nullifier.0.0.to_bytes_le().to_vec()
}

fn dust_nonce(sk: &DustSecretKey, backing_night: InitialNonce, seq: u32) -> crate::ledger_8::Fr {
	sk.nonces(backing_night)
		.nth(seq as usize)
		.expect("nonces is an endless sequence")
}

/// The output that spending `spent` created, from the `DustSpendProcessed` among `events`
/// (its transaction's) that reveals `spent`'s nullifier.
fn spend_successor(
	state: &DustLocalState<DefaultDB>,
	sk: &DustSecretKey,
	spent: &QualifiedDustOutput,
	events: &[Vec<u8>],
) -> Result<QualifiedDustOutput, BoxError> {
	let nullifier = spent.nullifier(sk);
	for raw in events {
		let event: Event<DefaultDB> = deserialize(&raw[..])?;
		let EventDetails::DustSpendProcessed {
			commitment,
			commitment_index,
			nullifier: revealed,
			v_fee,
			declared_time,
			..
		} = event.content
		else {
			continue;
		};
		if revealed != nullifier {
			continue;
		}
		let next = successor_output(state, sk, spent, commitment_index, v_fee, declared_time)?;
		// The value uses the tip's dust parameters; a `ParamChange` since this spend would make it
		// differ from what the chain committed to.
		if next.commitment() != commitment {
			return Err(format!(
				"recomputed DUST output {commitment_index} does not match the chain's commitment"
			)
			.into());
		}
		return Ok(next);
	}
	Err("the spending transaction has no DustSpendProcessed for this nullifier".into())
}

/// Mirrors `DustLocalState::replay_events`'s handling of `DustSpendProcessed`.
fn successor_output(
	state: &DustLocalState<DefaultDB>,
	sk: &DustSecretKey,
	spent: &QualifiedDustOutput,
	mt_index: u64,
	v_fee: u128,
	declared_time: Timestamp,
) -> Result<QualifiedDustOutput, BoxError> {
	let generation = state.generation_info(spent).ok_or("spent DUST output has no generation")?;
	let value = DustOutput::from(*spent).updated_value(&generation, declared_time, &state.params);
	Ok(QualifiedDustOutput {
		initial_value: value.saturating_sub(v_fee),
		owner: spent.owner,
		nonce: dust_nonce(sk, spent.backing_night, spent.seq + 1),
		seq: spent.seq + 1,
		ctime: declared_time,
		backing_night: spent.backing_night,
		mt_index,
	})
}

impl TryFrom<&BlockInfo> for LedgerParameters {
	type Error = std::io::Error;

	fn try_from(block: &BlockInfo) -> Result<Self, Self::Error> {
		deserialize(&block.ledger_parameters[..])
	}
}

impl From<&BlockInfo> for BlockContext {
	fn from(block: &BlockInfo) -> Self {
		make_block_context(
			Timestamp::from_secs(block.timestamp),
			HashOutput(block.parent_hash),
			Timestamp::from_secs(block.last_block_time),
		)
	}
}

/// The midnight transaction for this ledger generation, matching `fork::apply_block_8`.
type MnTx = Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

/// Extract the zswap offers to apply for a relevant transaction, honouring its result.
///
/// Mirrors `LedgerContext::successful_shielded_offers`. On `Success` the guaranteed offer plus all
/// fallible offers are applied; on `PartialSuccess` only the guaranteed offer is applied (the
/// indexer's union does not carry per-segment success here, and the next transaction's collapsed
/// update re-aligns the merkle index regardless); on `Failure` nothing is applied.
fn relevant_offers<S, P, D>(
	tx: &Transaction<S, P, PureGeneratorPedersen, D>,
	result: TransactionResultKind,
) -> Vec<Offer<P::LatestProof, D>>
where
	S: SignatureKind<D>,
	P: ProofKind<D>,
	D: DB,
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
		BlockContext::from(&self.tip().await)
	}

	async fn ledger_parameters(&self) -> LedgerParameters {
		LedgerParameters::try_from(&self.tip().await).expect("indexer: decode ledger parameters")
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
		todo!("indexer: dust generation status (ozgb-toolkit-indexer-dust-gen-flag)")
	}

	async fn zswap_state(&self) -> ZswapChainState<D> {
		// The indexer can't serve the chain-wide zswap state; the only caller needs one contract's,
		// so `ozgb-toolkit-indexer-contract-zswap` replaces this with `contract_zswap_state`.
		todo!("indexer: no chain-wide zswap state")
	}

	async fn contract_state(&self, address: ContractAddress) -> Option<ContractState<D>> {
		let address_bytes = serialize_untagged(&address).expect("serialize contract address");
		let state = self
			.client
			.contract_state(&address_bytes)
			.await
			.unwrap_or_else(|e| panic!("indexer: query contract state for {address:?}: {e}"))?;
		Some(
			deserialize(&state[..])
				.unwrap_or_else(|e| panic!("indexer: decode contract state for {address:?}: {e}")),
		)
	}

	async fn resolver(&self) -> &'static Resolver {
		*self.resolver.lock().expect("IndexerContext resolver lock poisoned")
	}

	async fn update_resolver(&self, resolver: &'static Resolver) {
		*self.resolver.lock().expect("IndexerContext resolver lock poisoned") = resolver;
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
		// The indexer keeps a full ledger state but exposes no way to validate against it, so the
		// node's check on submit is the only one; a node dry-run API (midnight-node#867) would
		// allow a pre-submit check.
		Ok(())
	}

	/// Applies shielded offers and unshielded spends/outputs as if the tx fully succeeds.
	/// Dust is skipped: building already marked the fee payer's dust spends, and replaying the
	/// change and new generation needs chain dust state.
	async fn apply_pending_tx<S, P>(
		&self,
		tx: &SerdeTransaction<S, P, D>,
		block_context: &BlockContext,
	) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
	where
		S: SignatureKind<D>,
		P: ProofKind<D> + std::fmt::Debug,
		Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
	{
		let SerdeTransaction::Midnight(tx) = tx else {
			return Ok(());
		};
		let offers = relevant_offers(tx, TransactionResultKind::Success);
		if !offers.is_empty() {
			self.fast_forward_shielded().await?;
			let mut wallets = self.wallets.lock().expect("IndexerContext wallets lock poisoned");
			for wallet in wallets.values_mut() {
				wallet.shielded.apply_offers(&offers);
			}
		}
		let Transaction::Standard(stx) = tx else {
			return Ok(());
		};
		let owners: HashMap<_, _> = self
			.wallets
			.lock()
			.expect("IndexerContext wallets lock poisoned")
			.iter()
			.map(|(seed, w)| (w.unshielded.user_address, seed.clone()))
			.collect();

		let mut unshielded =
			self.unshielded.lock().expect("IndexerContext unshielded lock poisoned");
		for entry in stx.intents.iter() {
			let (segment, intent) = (*entry.0, &entry.1);
			let spent: Vec<Utxo> = intent
				.guaranteed_inputs()
				.into_iter()
				.chain(intent.fallible_inputs())
				.map(Utxo::from)
				.collect();
			for utxos in unshielded.values_mut() {
				utxos.retain(|(utxo, _)| !spent.contains(utxo));
			}

			// Guaranteed outputs hash under segment 0, whatever the intent's segment.
			let erased = intent.erase_proofs().erase_signatures();
			let offers = [(0, intent.guaranteed_outputs()), (segment, intent.fallible_outputs())];
			for (hash_segment, outputs) in offers {
				let intent_hash = erased.intent_hash(hash_segment);
				for (output_no, output) in outputs.into_iter().enumerate() {
					let Some(utxos) = owners.get(&output.owner).and_then(|s| unshielded.get_mut(s))
					else {
						continue;
					};
					let utxo = Utxo {
						value: output.value,
						owner: output.owner,
						type_: output.type_,
						intent_hash,
						output_no: output_no as u32,
					};
					utxos.push((utxo, block_context.tblock));
				}
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ledger_8::{DustPublicKey, INITIAL_PARAMETERS, serialize};

	fn block_info(ledger_parameters: Vec<u8>) -> BlockInfo {
		BlockInfo {
			height: 2,
			protocol_version: 0,
			timestamp: 1_700_000_006,
			parent_hash: [0x11; 32],
			last_block_time: 1_700_000_000,
			zswap_end_index: 0,
			ledger_parameters,
		}
	}

	/// The indexer serves tagged `LedgerParameters`; an untagged decode would fail here.
	#[test]
	fn decodes_tagged_ledger_parameters() {
		let block = block_info(serialize(&INITIAL_PARAMETERS).unwrap());
		assert_eq!(LedgerParameters::try_from(&block).unwrap(), INITIAL_PARAMETERS);
	}

	#[test]
	fn block_context_matches_replay_construction() {
		let ctx = BlockContext::from(&block_info(vec![]));
		let expected = make_block_context(
			Timestamp::from_secs(1_700_000_006),
			HashOutput([0x11; 32]),
			Timestamp::from_secs(1_700_000_000),
		);
		assert_eq!(ctx.tblock, expected.tblock);
		assert_eq!(ctx.tblock_err, expected.tblock_err);
		assert_eq!(ctx.parent_block_hash, expected.parent_block_hash);
		assert_eq!(ctx.last_block_time, expected.last_block_time);
	}

	/// Fast sync must recompute exactly the output the ledger's own spend creates.
	#[test]
	fn successor_output_matches_the_ledger_spend() {
		let sk = DustSecretKey::derive_secret_key(&[7; 32]);
		let owner = DustPublicKey::from(sk.clone());
		let params = INITIAL_PARAMETERS.dust;
		let backing_night = InitialNonce(HashOutput([3; 32]));
		let info = DustGenerationInfo {
			value: 1_000_000_000,
			owner,
			nonce: backing_night,
			dtime: Timestamp::MAX,
		};
		let spent = QualifiedDustOutput {
			initial_value: 0,
			owner,
			nonce: dust_nonce(&sk, backing_night, 0),
			seq: 0,
			ctime: Timestamp::from_secs(1_700_000_000),
			backing_night,
			mt_index: 0,
		};
		let state = DustLocalState::<DefaultDB>::new(params)
			.insert_generation_info(0, info, Some(backing_night))
			.unwrap()
			.insert_commitment(0, spent, true)
			.unwrap()
			.add_utxo(&spent.nullifier(&sk), &spent, None)
			.unwrap();

		let declared_time = Timestamp::from_secs(1_700_003_600);
		let value = DustOutput::from(spent).updated_value(&info, declared_time, &params);
		assert!(value > 0, "the generation must have produced DUST to spend");
		let v_fee = value / 3;
		let (_, spend) = state.spend(&sk, &spent, v_fee, declared_time).unwrap();

		let next = successor_output(&state, &sk, &spent, 1, v_fee, declared_time).unwrap();
		assert_eq!(next.commitment(), spend.new_commitment);
		assert_eq!(next.seq, 1);
	}

	/// The frontier round-trips its heads, and only generations it cached resume past its block.
	#[test]
	fn chain_starts_resume_cached_generations_after_the_frontier() {
		let sk = DustSecretKey::derive_secret_key(&[7; 32]);
		let owner = DustPublicKey::from(sk.clone());
		let output = |night: u8, seq: u32| {
			let backing_night = InitialNonce(HashOutput([night; 32]));
			QualifiedDustOutput {
				initial_value: 10 * u128::from(seq),
				owner,
				nonce: dust_nonce(&sk, backing_night, seq),
				seq,
				ctime: Timestamp::from_secs(1_700_000_000 + u64::from(seq)),
				backing_night,
				mt_index: u64::from(night) * 100 + u64::from(seq),
			}
		};
		let frontier = |heads: &[QualifiedDustOutput]| DustFrontierRaw {
			height: 41,
			block_hash: [9; 32],
			heads: heads.iter().map(|head| serialize_untagged(head).unwrap()).collect(),
		};
		let firsts = vec![output(1, 0), output(2, 0)];

		assert_eq!(
			chain_starts(firsts.clone(), None).unwrap(),
			vec![(output(1, 0), 0), (output(2, 0), 0)],
		);
		assert_eq!(
			chain_starts(firsts.clone(), Some(&frontier(&[output(1, 3)]))).unwrap(),
			vec![(output(1, 3), 42), (output(2, 0), 0)],
		);
		assert!(
			chain_starts(firsts, Some(&frontier(&[output(1, 3), output(3, 0)]))).is_err(),
			"a cached head with no generation at the tip must fail the resume",
		);
	}

	/// Both reads come from the cached tip; the unroutable URL would fail any re-query.
	#[test]
	fn block_context_and_parameters_share_the_tip_snapshot() {
		let ctx = IndexerContext::<DefaultDB>::new(
			"http://127.0.0.1:1/api/v4",
			"undeployed",
			NonZeroUsize::MIN,
		)
		.unwrap();
		let block = block_info(serialize(&INITIAL_PARAMETERS).unwrap());
		*ctx.tip.lock().unwrap() = Some(block.clone());

		futures::executor::block_on(async {
			assert_eq!(ctx.ledger_parameters().await, INITIAL_PARAMETERS);
			assert_eq!(ctx.latest_block_context().await.tblock, BlockContext::from(&block).tblock);
		});
	}
}

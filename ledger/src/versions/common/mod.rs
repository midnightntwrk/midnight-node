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

#[cfg(feature = "std")]
pub(crate) use super::TransactionSignature;

#[cfg(feature = "std")]
use super::{
	base_crypto_local, coin_structure_local, helpers_local, ledger_storage_local,
	midnight_serialize_local, mn_ledger_local, onchain_runtime_local, transient_crypto_local,
	zswap_local,
};

#[cfg(feature = "std")]
use midnight_serialize_local::Tagged;
#[cfg(feature = "std")]
use sha2::digest::{OutputSizeUser, consts::U32};
#[cfg(feature = "std")]
use transient_crypto_local::commitment::PureGeneratorPedersen;

use alloc::vec::Vec;
use frame_support::{StorageHasher, Twox128};
use sp_externalities::{Externalities, ExternalitiesExt};

pub mod types;
use types::LedgerApiError;

#[cfg(feature = "std")]
pub mod storage;

#[cfg(feature = "std")]
pub mod api;

#[cfg(feature = "std")]
pub mod conversions;

#[cfg(feature = "std")]
pub mod utxo_ordering_override;

#[cfg(feature = "std")]
use {
	api::{
		ContractAddress, ContractState, Ledger, LedgerParameters, SystemTransaction, Transaction,
		TransactionAppliedStage, TransactionOperation,
	},
	base_crypto_local::{
		cost_model::NormalizedCost as LedgerNormalizedCost,
		hash::HashOutput,
		time::{Duration as DurationLedger, Timestamp},
	},
	coin_structure_local::coin::Nonce,
	ledger_storage_local::{
		Storage,
		arena::{ArenaKey, Sp, TypedArenaKey},
		db::{DB, ParityDb, paritydb::OwnedDb},
		storage::{default_storage, set_default_storage},
	},
	midnight_primitives_ledger::{LedgerMetricsExt, LedgerStorageDb, LedgerStorageExt},
	mn_ledger_local::{
		dust::InitialNonce,
		structure::{
			CNightGeneratesDustActionType, CNightGeneratesDustEvent, ClaimKind, ContractAction,
			MaintenanceUpdate, OutputInstructionUnshielded, ProofMarker, SignatureKind,
			SingleUpdate, Transaction as LedgerTransaction, VerifiedTransaction,
		},
	},
	std::{
		any::{Any, TypeId},
		sync::Arc,
		time::{Duration, Instant},
	},
};

use crate::common::types::{
	ContractCallsDetails, FallibleCoinsDetails, GasCost, GuaranteedCoinsDetails, Hash, Op,
	SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot, TransactionDetails, Tx,
	WrappedHash,
};

use super::BlockContext;

#[cfg(feature = "std")]
use {lazy_static::lazy_static, moka::ops::compute::Op as CacheOp, moka::sync::Cache};

pub const LOG_TARGET: &str = "midnight::ledger_v2";
pub const MINT_COINS_DOMAIN_SEPARATOR: &[u8; 10] = b"mint_coins";

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StrictTxValidationKey {
	state_hash: Hash,
	tx_hash: Hash,
	/// The timestamp the cached `VerifiedTransaction` was verified at — the output of
	/// [`well_formed_tblock`], not the block's own `tblock`. Host-function v1 and v2 skew a
	/// block's first transaction differently, and both can run in one process; keying on what
	/// `well_formed` was actually given keeps them from reusing each other's entries.
	well_formed_tblock: u64,
}
#[derive(PartialEq, Eq, Hash)]
pub struct SoftTxValidationKey {
	tx_hash: Hash,
}

/// Set this high to ensure that even large mempool sizes don't cause performance issues due to
/// unnecessary revalidation.
#[cfg(feature = "std")]
const SOFT_TX_VALIDATION_CACHE_CAPACITY: u64 = 2000;

/// This should be set to no more than the max expected txs per block
/// 600 txs/block allows for 100 TPS (considerable higher than our real max at the time of writing)
#[cfg(feature = "std")]
const STRICT_TX_VALIDATION_CACHE_CAPACITY: u64 = 600;

/// Time-to-idle for transaction validation cache entries.
/// Entries not accessed within this duration are evicted, preventing stale VerifiedTransaction
/// objects (which contain ZK proof data and can be 50-200 KiB each) from persisting indefinitely
/// on low-traffic networks. Without this TTL, the cache only evicts by count — on quiet chains
/// entries live forever and contribute to steady-state memory growth.
#[cfg(feature = "std")]
const TX_VALIDATION_CACHE_TTI: Duration = Duration::from_secs(300);

/// Time-to-live for soft validation cache entries.
/// Unlike TTI, TTL evicts entries unconditionally after this duration regardless of access.
/// This is critical for relay nodes (non-block-producers) where soft cache entries are never
/// invalidated by block authoring — without a TTL, revalidation keeps accessing entries and
/// resetting the TTI timer, so invalid transactions persist in the mempool indefinitely.
/// Set to 60s (~10 blocks at 6s/block) to balance eviction latency against revalidation cost.
#[cfg(feature = "std")]
const SOFT_TX_VALIDATION_CACHE_TTL: Duration = Duration::from_secs(60);

#[cfg(feature = "std")]
lazy_static! {
	/// Strict cache: stores VerifiedTransaction for reuse in validate_guaranteed_execution.
	///
	/// We use `Arc<dyn Any + Send + Sync>` for type erasure because:
	/// - Bridge<S, D> is generic over Signature and Database types
	/// - Multiple signature types exist across ledger versions (e.g., Signature, SignatureHF)
	/// - Database type may vary (ParityDb, etc.)
	/// - A single static cache must store VerifiedTransaction for all type combinations
	///
	/// When retrieving, we downcast to the concrete VerifiedTransaction type.
	static ref STRICT_TX_VALIDATION_CACHE: Cache<StrictTxValidationKey, Arc<dyn Any + Send + Sync>> =
		Cache::builder()
			.max_capacity(STRICT_TX_VALIDATION_CACHE_CAPACITY)
			.time_to_idle(TX_VALIDATION_CACHE_TTI)
			.build();

	/// Soft cache: stores validation result for mempool revalidation.
	/// No type erasure needed since Result<(), LedgerApiError> is not generic.
	static ref SOFT_TX_VALIDATION_CACHE: Cache<SoftTxValidationKey, Result<(), LedgerApiError>> =
		Cache::builder()
			.max_capacity(SOFT_TX_VALIDATION_CACHE_CAPACITY)
			.time_to_idle(TX_VALIDATION_CACHE_TTI)
			.time_to_live(SOFT_TX_VALIDATION_CACHE_TTL)
			.build();
}

/// Capacity of the intra-block keep-alive cache.
///
/// Deliberately ~1000x the resident set of one: moka's TinyLFU *admission* filter can
/// reject a just-inserted entry when the cache is at capacity, and for this cache a
/// rejected insert is a fatal mid-block miss. At 1024 against a resident set of one the
/// filter can never engage. [`TRANSIENT_STATE_CACHE_TTI`] is the real bound.
#[cfg(feature = "std")]
const TRANSIENT_STATE_CACHE_CAPACITY: u64 = 1024;

/// Leak bound for the intra-block keep-alive cache (~10 blocks). Entries are normally
/// released explicitly by the successor call; this only catches an execution that
/// abandoned its tail (a proposal that was never sealed, a failed import).
#[cfg(feature = "std")]
const TRANSIENT_STATE_CACHE_TTI: Duration = Duration::from_secs(60);

/// Room for the current tip, a fork sibling, and the finalized tip an RPC may target.
#[cfg(feature = "std")]
const ANCHORED_STATE_CACHE_CAPACITY: u64 = 4;

#[cfg(feature = "std")]
const ANCHORED_STATE_CACHE_TTI: Duration = Duration::from_secs(300);

/// Key for the ledger-state keep-alive caches.
///
/// The `db` discriminator matters: `DbSeparate` and `DbUnified` are distinct arenas
/// that produce identical content hashes, so without it a release on one would drop
/// the other's keep-alive.
#[cfg(feature = "std")]
#[derive(PartialEq, Eq, Hash, Clone)]
struct StateCacheKey {
	db: TypeId,
	state_key: Vec<u8>,
}

#[cfg(feature = "std")]
impl StateCacheKey {
	fn new<D: DB>(state_key: &[u8]) -> Self {
		Self { db: TypeId::of::<D>(), state_key: state_key.to_vec() }
	}
}

/// A retained intra-block ledger state.
///
/// Refcounted because two block executions can be in flight at once (authoring
/// alongside import) and two forks off the same parent applying the same transaction
/// produce the *same* content hash. Without a count the first release would kill the
/// second execution's keep-alive and it would fail with `NoLedgerState`. This is the
/// node-side home for what the arena's root counter used to do for the persisted
/// intermediates.
///
/// The `Sp` itself is never read back — holding it is the entire point (see
/// [`retain_transient`]).
#[cfg(feature = "std")]
#[derive(Clone)]
struct RetainedState {
	_sp: Arc<dyn Any + Send + Sync>,
	refs: usize,
}

#[cfg(feature = "std")]
lazy_static! {
	/// Intra-block intermediate ledger states. These are NOT persisted — this cache is
	/// the only thing keeping them addressable in the arena, so entries are released
	/// explicitly by the successor call.
	///
	/// The stored value is an `Arc<Sp<Ledger<D>, D>>` upcast to `Arc<dyn Any + Send +
	/// Sync>`, the same type erasure the tx-validation caches use because `Bridge<S, D>`
	/// is generic. Here the erasure only has to *store*: nothing reads the value back,
	/// so there is no downcast on any hot path. It must be the whole `Sp` — that is what
	/// holds the arena metadata refcount above zero (keeping `uncache` from firing) and
	/// keeps the inner `Arc` alive for the arena's `sp_cache` to upgrade.
	static ref TRANSIENT_LEDGER_STATES: Cache<StateCacheKey, RetainedState> =
		Cache::builder()
			.max_capacity(TRANSIENT_STATE_CACHE_CAPACITY)
			.time_to_idle(TRANSIENT_STATE_CACHE_TTI)
			.build();

	/// Post-block tips. These ARE persisted, so a miss just falls back to the arena and
	/// costs one re-materialisation — no explicit release, no refcount, and eviction is
	/// always safe.
	static ref ANCHORED_LEDGER_STATES: Cache<StateCacheKey, Arc<dyn Any + Send + Sync>> =
		Cache::builder()
			.max_capacity(ANCHORED_STATE_CACHE_CAPACITY)
			.time_to_idle(ANCHORED_STATE_CACHE_TTI)
			.build();
}

/// Keep an intra-block intermediate state materialised past the end of this host call.
///
/// Without this the `Sp` is dropped when the Bridge method returns, and `Sp::drop` ->
/// `decrement_ref` -> `backend.uncache` removes a non-persisted `CacheValue::Create`
/// from the arena entirely, leaving the state unaddressable for the successor call.
/// Holding the `Sp` instead of rooting it also means the successor (and the mempool /
/// weight paths that read the same tip) find the already-deserialized `Arc` in the
/// arena's `sp_cache` rather than rebuilding the working set from binary.
#[cfg(feature = "std")]
fn retain_transient<D: DB>(state_key: &[u8], sp: &Sp<Ledger<D>, D>) {
	let sp: Arc<dyn Any + Send + Sync> = Arc::new(sp.clone());
	// `and_compute_with` is the read-modify-write under moka's per-key lock, so the
	// refcount needs no `Mutex` of its own.
	TRANSIENT_LEDGER_STATES
		.entry(StateCacheKey::new::<D>(state_key))
		.and_compute_with(|entry| match entry {
			Some(entry) => {
				let prev = entry.into_value();
				CacheOp::Put(RetainedState { refs: prev.refs + 1, ..prev })
			},
			None => CacheOp::Put(RetainedState { _sp: sp, refs: 1 }),
		});
}

/// Drop this execution's claim on an intra-block intermediate state.
///
/// Unconditional by design: an anchored input (a post-block tip, or genesis) is never in
/// the transient cache, so the release is a no-op for it. An absent key is likewise a
/// no-op — either an anchored input, or an entry the TTI already reclaimed.
///
/// Note this is a keep-alive release, not an un-root: it cannot disturb a `persist()`ed
/// state even on a (practically impossible) content-hash collision.
#[cfg(feature = "std")]
fn release_transient<D: DB>(state_key: &[u8]) {
	TRANSIENT_LEDGER_STATES
		.entry(StateCacheKey::new::<D>(state_key))
		.and_compute_with(|entry| match entry {
			Some(entry) => {
				let prev = entry.into_value();
				if prev.refs > 1 {
					CacheOp::Put(RetainedState { refs: prev.refs - 1, ..prev })
				} else {
					CacheOp::Remove
				}
			},
			None => CacheOp::Nop,
		});
	// moka's removal only takes the entry out of the map synchronously; dropping the
	// value is queued. Drain it here, because that drop is what runs `Sp::drop` ->
	// `uncache` and takes the released state's `Create` nodes back out of the backend's
	// write cache. Left until some arbitrary later cache operation, they would still be
	// there at the block's `flush_all_changes_to_db` and be written to disk as
	// unreferenced nodes.
	TRANSIENT_LEDGER_STATES.run_pending_tasks();
}

/// Keep a post-block tip materialised so the next block's first read doesn't have to
/// re-deserialize it.
///
/// Pure optimisation, unlike [`retain_transient`]: anchored states are `persist()`ed, so
/// an eviction costs one re-materialisation from the arena and nothing more. Hence no
/// refcount and no release.
#[cfg(feature = "std")]
fn retain_anchored<D: DB>(state_key: &[u8], sp: &Sp<Ledger<D>, D>) {
	let sp: Arc<dyn Any + Send + Sync> = Arc::new(sp.clone());
	ANCHORED_LEDGER_STATES.insert(StateCacheKey::new::<D>(state_key), sp);
}

/// Release every retained ledger state.
///
/// Each retained `Sp` holds an `Arena<D>`, i.e. `Arc`s into `Storage<D>` and therefore
/// the parity-db handle and its exclusive file lock. Storage teardown must go through
/// here or dropping the default storage doesn't actually close the database.
/// `run_pending_tasks` is what performs the deferred drops.
#[cfg(feature = "std")]
pub(crate) fn clear_ledger_state_caches() {
	// Per-key `invalidate`, not `invalidate_all`: the latter only stamps a
	// `valid_after` marker and leaves the values in place until an eviction sweep
	// reaches them, which is not good enough when the point is to release the
	// database handle right now. `run_pending_tasks` then drains the write queue that
	// still holds the removed entries.
	for (key, _) in TRANSIENT_LEDGER_STATES.iter() {
		TRANSIENT_LEDGER_STATES.invalidate(&*key);
	}
	for (key, _) in ANCHORED_LEDGER_STATES.iter() {
		ANCHORED_LEDGER_STATES.invalidate(&*key);
	}
	TRANSIENT_LEDGER_STATES.run_pending_tasks();
	ANCHORED_LEDGER_STATES.run_pending_tasks();
}

#[cfg(feature = "std")]
pub struct Bridge<S: SignatureKind<D>, D: DB> {
	_phantom: core::marker::PhantomData<(S, D)>,
}

#[cfg(feature = "std")]
impl<S: SignatureKind<D> + std::fmt::Debug, D: DB> Bridge<S, D>
where
	mn_ledger_local::structure::Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Tagged,
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	pub fn set_default_storage(mut externalities: &mut dyn Externalities) {
		let maybe_storage = externalities.extension::<LedgerStorageExt>();
		if let Some(storage) = maybe_storage {
			match &storage.db {
				LedgerStorageDb::UnifiedDb(db) => {
					let res = set_default_storage(|| {
						let db =
                            ParityDb::<sha2::Sha256, _, { LedgerStorageExt::COLUMN_OFFSET }>::from_existing_db(OwnedDb(db.clone()));
						Storage::new(storage.cache_size, db)
					});
					if res.is_err() {
						log::warn!(
							target: LOG_TARGET,
							"Warning: Failed to set default storage, already initialized (UnifiedDb)"
						);
					}
				},
				LedgerStorageDb::SeparateDb(db_path) => {
					let res = set_default_storage(|| {
						let db = ParityDb::<sha2::Sha256>::open(db_path.as_path());
						Storage::new(storage.0.cache_size, db)
					});
					if res.is_err() {
						log::warn!(
							target: LOG_TARGET,
							"Warning: Failed to set default storage, already initialized (SeparateDb)"
						);
					}
				},
			};
		} else {
			log::error!(
				target: LOG_TARGET,
				"Ledger Storage Externality should be always present!!",
			);
		}
	}

	pub fn flush_storage(mut externalities: &mut dyn Externalities) {
		// Before the flush, not after: this settles any queued release or eviction, so
		// the states it drops are uncached out of the write cache instead of being
		// written to disk. It also makes `entry_count` below accurate — it is eventually
		// consistent, and a stale zero would hide exactly the leak the gauge exists to
		// catch.
		TRANSIENT_LEDGER_STATES.run_pending_tasks();
		ANCHORED_LEDGER_STATES.run_pending_tasks();

		let now = std::time::Instant::now();
		default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
		let elapsed = now.elapsed().as_secs_f64();

		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			metrics.observe_storage_flush_time(elapsed, "ledger_state");
			// Sampled here, after `apply_post_block_update` released the block's last
			// intermediate, so a non-zero "transient" reading is unambiguously a leak.
			metrics.set_ledger_state_cache_size("transient", TRANSIENT_LEDGER_STATES.entry_count());
			metrics.set_ledger_state_cache_size("anchored", ANCHORED_LEDGER_STATES.entry_count());
		}
	}

	/// Apply the post-block transformation and produce the post-block ledger
	/// state.
	///
	/// # Persist / keep-alive contract
	///
	/// The post-block tip is the one state per block that is `persist()`ed, so it is a
	/// GC root at rc=1 — retained for RPC and history, and safe across sibling forks
	/// (the root count is a count, so two forks persisting the same tip each hold one
	/// reference and nothing here ever unpersists). It is additionally kept in the
	/// anchored keep-alive cache, which is pure optimisation: an eviction costs one
	/// re-materialisation from the arena.
	///
	/// The input is released from the transient keep-alive cache unconditionally. A
	/// post-block tip or genesis input was never in that cache, so the release is a
	/// no-op for it.
	///
	/// On error, nothing is retained or released.
	pub fn post_block_update(
		mut _externalities: &mut dyn Externalities,
		state_key: &[u8],
		block_context: BlockContext,
	) -> Result<Vec<u8>, LedgerApiError> {
		let start_tx_processing_time = Instant::now();
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Initializing API (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let api = api::new();
		log::trace!(
			target: LOG_TARGET,
			"⏱️  API ready (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let ledger = Self::get_ledger(&api, state_key)?;

		log::trace!(
			target: LOG_TARGET,
			"⏱️  Post block update start (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let mut ledger = Ledger::post_block_update(ledger, block_context).inspect_err(|e| {
			log::error!(
				target: LOG_TARGET,
				"Post Block Update error: {e:?}"
			);
		})?;
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Post block update done (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		let state_root = api.tagged_serialize(&ledger.as_typed_key())?;

		// Only update state after no errors
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Persisting ledger (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		ledger.persist();
		retain_anchored(&state_root, &ledger);
		release_transient::<D>(state_key);
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ledger persisted (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		Ok(state_root)
	}

	/// The end-of-block ledger transition cannot fail on block limits (the limit check runs
	/// per-transaction via prevalidation, and fullness is clamped before applying), so this is
	/// suitable for `on_finalize`. Loading the ledger state and serializing the resulting key
	/// remain fallible — those represent genuine bugs rather than block-content conditions.
	///
	/// # Persist / keep-alive contract
	///
	/// Same contract as [`Self::post_block_update`].
	pub fn apply_post_block_update(
		mut _externalities: &mut dyn Externalities,
		state_key: &[u8],
		block_context: BlockContext,
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let mut ledger = Ledger::apply_post_block_update(ledger, block_context);
		let state_root = api.tagged_serialize(&ledger.as_typed_key())?;
		ledger.persist();
		retain_anchored(&state_root, &ledger);
		release_transient::<D>(state_key);
		Ok(state_root)
	}

	pub fn get_version() -> Vec<u8> {
		crate::utils::find_crate_version(super::CRATE_NAME).unwrap_or(b"unknown".into())
	}

	/// Apply a user transaction and produce the resulting ledger state.
	///
	/// # Persist / keep-alive contract
	///
	/// The resulting intra-block state is *not* persisted. It is held live in the
	/// transient keep-alive cache instead, which is what keeps it addressable: nothing
	/// roots it, so it never becomes GC-visible and never reaches the DB. The successor
	/// call releases it.
	///
	/// The input is released unconditionally. If it was the prior intra-block
	/// intermediate, that drops this execution's claim on it (and the last claim drops
	/// the state); if it was a post-block tip or genesis, it was never in the cache and
	/// the release is a no-op.
	///
	/// On error, nothing is retained or released.
	pub fn apply_transaction(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		should_skip_failed_segments: bool,
		runtime_version: u32,
		skew_tblock: bool,
	) -> Result<TransactionAppliedStateRoot, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		// Gather metrics for Prometheus
		let start_tx_processing_time = Instant::now();
		let tx_size = tx_serialized.len();

		log::trace!(
			target: LOG_TARGET,
			"⏱️  Starting tx processing (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let api = api::new();
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Deserializing tx (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let tx_hash = tx.hash();
		log::info!(
			target: LOG_TARGET,
			"📥 Applying transaction {}",
			hex::encode(tx_hash)
		);
		let ledger = Self::get_ledger(&api, state_key)?;
		utxo_ordering_override::set_network_id(&ledger.state.network_id);
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ledger loaded (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let initial_utxos_size = ledger.state.utxo.utxos.size();

		// Use cached VerifiedTransaction if available
		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);
		let verified_tx =
			Self::get_verified_transaction(&ledger, &tx, &block_context, &cache_key, skew_tblock)?;
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Building tx context (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		// Apply the verified transaction
		let tx_ctx = ledger.get_transaction_context(block_context.clone())?;
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Tx context ready (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let (new_ledger, applied_stage) =
			Ledger::apply_verified_transaction(ledger, &api, &tx, &verified_tx, &tx_ctx)?;
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ledger applied (stage={applied_stage:?}, elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		let all_applied = matches!(applied_stage, TransactionAppliedStage::AllApplied);

		log::trace!(
			target: LOG_TARGET,
			"⏱️  Building unshielded UTXOs (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let mut utxos = tx.unshielded_utxos();

		let failed_segments =
			if let TransactionAppliedStage::PartialSuccess(segments) = applied_stage {
				// Remove from `utxos` the `segments` that failed
				utxos.remove_failed_segments(&segments);
				Some(segments.keys().copied().collect())
			} else {
				None
			};
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Unshielded UTXOs ready (failed_segments={}, elapsed_ms={})",
			failed_segments.as_ref().map(|segments: &Vec<u16>| segments.len()).unwrap_or(0),
			start_tx_processing_time.elapsed().as_millis()
		);

		let operations =
			tx.calls_and_deploys(should_skip_failed_segments.then_some(failed_segments).flatten());
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ops built (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		// Capture segment counts before flattening — the HashMap→BTreeMap fix
		// only changes ordering between segments, not within a single segment.
		let output_segments = utxos.outputs.len();
		let input_segments = utxos.inputs.len();

		let (mut utxo_outputs, mut utxo_inputs) =
			utxos.check_utxos_response_integrity(initial_utxos_size, &new_ledger)?;

		// Apply ordering override for old blocks produced with HashMap ordering.
		// Only reorder lists that span multiple segments.
		if let Some(ordering) = utxo_ordering_override::get_override(&tx_hash) {
			ordering.apply(&mut utxo_outputs, output_segments, &mut utxo_inputs, input_segments);
		}

		log::trace!(
			target: LOG_TARGET,
			"⏱️  UTXO integrity ok (created={}, spent={}, elapsed_ms={})",
			utxo_outputs.len(),
			utxo_inputs.len(),
			start_tx_processing_time.elapsed().as_millis()
		);

		let mut event = TransactionAppliedStateRoot {
			state_root: api.tagged_serialize(&new_ledger.as_typed_key())?,
			tx_hash,
			all_applied,
			call_addresses: vec![],
			deploy_addresses: vec![],
			maintain_addresses: vec![],
			claim_rewards: vec![],
			unshielded_utxos_created: utxo_outputs,
			unshielded_utxos_spent: utxo_inputs,
		};
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Event built (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		for op in operations {
			match op {
				TransactionOperation::Call { address, .. } => {
					event.call_addresses.push(api.tagged_serialize(&address)?);
					log::trace!(
						target: LOG_TARGET,
						"⏱️  Tx op: Call (elapsed_ms={})",
						start_tx_processing_time.elapsed().as_millis()
					);
				},
				TransactionOperation::Deploy { address } => {
					event.deploy_addresses.push(api.tagged_serialize(&address)?);
					log::trace!(
						target: LOG_TARGET,
						"⏱️  Tx op: Deploy (elapsed_ms={})",
						start_tx_processing_time.elapsed().as_millis()
					);
				},
				TransactionOperation::Maintain { address } => {
					event.maintain_addresses.push(api.tagged_serialize(&address)?);
					log::trace!(
						target: LOG_TARGET,
						"⏱️  Tx op: Maintain (elapsed_ms={})",
						start_tx_processing_time.elapsed().as_millis()
					);
				},
				TransactionOperation::ClaimRewards { value } => {
					event.claim_rewards.push(value);
					log::trace!(
						target: LOG_TARGET,
						"⏱️  Tx op: ClaimRewards (elapsed_ms={})",
						start_tx_processing_time.elapsed().as_millis()
					);
				},
				TransactionOperation::ClaimBridgeTransfer { value } => {
					event.claim_rewards.push(value);
					log::trace!(
						target: LOG_TARGET,
						"⏱️  Tx op: ClaimBridgeTransfer (elapsed_ms={})",
						start_tx_processing_time.elapsed().as_millis()
					);
				},
			}
		}

		// Only update state after no errors
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Retaining ledger (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		retain_transient(&event.state_root, &new_ledger);
		release_transient::<D>(state_key);
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ledger retained (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		// Write Prometheus metrics
		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			let tx_type = Self::get_tx_type(&tx);
			let elapsed_time = start_tx_processing_time.elapsed().as_secs_f64();

			metrics.observe_txs_processing_time(elapsed_time, tx_type);
			metrics.observe_txs_size(tx_size as f64, tx_type);
		}
		log::trace!(
			target: LOG_TARGET,
			"✅ Tx applied (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		Ok(event)
	}

	/// Apply a system transaction and produce the resulting ledger state.
	///
	/// # Persist / keep-alive contract
	///
	/// Same contract as [`Self::apply_transaction`].
	pub fn apply_system_transaction(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
	) -> Result<SystemTransactionAppliedStateRoot, LedgerApiError> {
		// Gather metrics for Prometheus
		let start_system_tx_processing_time = Instant::now();
		let tx_size = tx_serialized.len();

		let api = api::new();
		let tx = api.tagged_deserialize::<SystemTransaction>(tx_serialized)?;
		let tx_type = Self::get_system_tx_type(&tx)?;
		log::info!(
			target: LOG_TARGET,
			"⚙️  Processing SystemTx {tx:?}"
		);
		let tx_hash = tx.transaction_hash().0.0;
		let ledger = Self::get_ledger(&api, state_key)?;

		let ledger =
			Ledger::apply_system_tx(ledger, &tx, Timestamp::from_secs(block_context.tblock))?;

		let event = SystemTransactionAppliedStateRoot {
			state_root: api.tagged_serialize(&ledger.as_typed_key())?,
			tx_hash,
			tx_type: tx_type.to_string(),
		};

		// Only update state after no errors
		retain_transient(&event.state_root, &ledger);
		release_transient::<D>(state_key);

		// Write Prometheus metrics
		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			let elapsed_time = start_system_tx_processing_time.elapsed().as_secs_f64();

			metrics.observe_system_txs_processing_time(elapsed_time, tx_type);
			metrics.observe_txs_size(tx_size as f64, tx_type);
		}

		Ok(event)
	}

	pub fn validate_transaction(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
		// The runtime's max weight as of now
		max_weight: u64,
		get_tx_details: bool,
	) -> Result<(Hash, Option<TransactionDetails>), LedgerApiError> {
		// Gather metrics for Prometheus
		let start_tx_validation_time = Instant::now();

		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let wrapped_cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// No `tblock` correction on the mempool path: `validate_unsigned` already skews the
		// block context it passes here by `slot_duration * (1 + MaxSkippedSlots)`.
		let was_cached =
			Self::do_validate_transaction(&ledger, &tx, &block_context, &wrapped_cache_key)?;

		let tx_details = if get_tx_details {
			let tx_gas_cost =
				Self::get_transaction_cost(state_key, tx_serialized, &block_context, max_weight)?;

			Some(Self::get_transaction_details(&tx, &ledger, tx_gas_cost)?)
		} else {
			None
		};

		// Write Prometheus metrics
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			// Record cache hit/miss metrics
			if was_cached {
				metrics.inc_tx_validation_cache_hit("soft");
			} else {
				metrics.inc_tx_validation_cache_miss();
				// Only record validation time on cache miss (when actual work was done)
				let tx_type = Self::get_tx_type(&tx);
				let elapsed_time = start_tx_validation_time.elapsed().as_secs_f64();
				metrics.observe_txs_validating_time(elapsed_time, tx_type);
			}

			// Report current cache sizes
			metrics
				.set_tx_validation_cache_size("strict", STRICT_TX_VALIDATION_CACHE.entry_count());
			metrics.set_tx_validation_cache_size("soft", SOFT_TX_VALIDATION_CACHE.entry_count());
		}

		Ok((wrapped_cache_key.0, tx_details))
	}

	/// Validates that applying a transaction will succeed.
	///
	/// Used by `pre_dispatch` to reject transactions whose application
	/// would fail - this keeps the block free of failed transactions.
	///
	/// This function checks the strict cache for a cached `VerifiedTransaction`
	/// (populated by `validate_unsigned(strict=true)`) to avoid redundant ZK
	/// proof verification via `well_formed()`.
	pub fn validate_guaranteed_execution(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
		skew_tblock: bool,
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// Perform dry-run validation with caching
		let was_cached = Self::do_validate_guaranteed_execution(
			&ledger,
			&tx,
			&block_context,
			&cache_key,
			skew_tblock,
		)?;

		// Write Prometheus metrics
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			if was_cached {
				metrics.inc_tx_validation_cache_hit("strict");
			} else {
				metrics.inc_tx_validation_cache_miss();
			}

			// Report current cache sizes
			metrics
				.set_tx_validation_cache_size("strict", STRICT_TX_VALIDATION_CACHE.entry_count());
			metrics.set_tx_validation_cache_size("soft", SOFT_TX_VALIDATION_CACHE.entry_count());
		}

		Ok(())
	}

	pub fn get_decoded_transaction(transaction_bytes: &[u8]) -> Result<Tx, LedgerApiError> {
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(transaction_bytes)?;
		let hash = tx.hash();
		let operations = tx.calls_and_deploys(None).try_fold(Vec::new(), |mut acc, cd| {
			let a = match cd {
				TransactionOperation::Call { address, entry_point } => {
					Op::Call { address: api.tagged_serialize(&address)?, entry_point }
				},
				TransactionOperation::Deploy { address } => {
					Op::Deploy { address: api.tagged_serialize(&address)? }
				},
				TransactionOperation::Maintain { address } => {
					Op::Maintain { address: api.tagged_serialize(&address)? }
				},
				TransactionOperation::ClaimRewards { value } => Op::ClaimRewards { value },
				TransactionOperation::ClaimBridgeTransfer { value } => {
					Op::ClaimBridgeTransfer { value }
				},
			};
			acc.push(a);
			Ok::<_, LedgerApiError>(acc)
		})?;

		let identifiers = tx.identifiers().try_fold(Vec::new(), |mut acc, i| {
			acc.push(api.tagged_serialize(&i)?);
			Ok::<_, LedgerApiError>(acc)
		})?;

		Ok(Tx {
			hash,
			operations,
			identifiers,
			has_fallible_coins: tx.has_fallible_coins(),
			has_guaranteed_coins: tx.has_guaranteed_coins(),
		})
	}

	fn do_get_contract_state<F>(
		api: &api::Api,
		state_key: &[u8],
		contract_address: &[u8],
		f: F,
	) -> Result<Vec<u8>, LedgerApiError>
	where
		F: FnOnce(ContractState<D>) -> Result<Vec<u8>, LedgerApiError>,
	{
		let addr = api.deserialize::<ContractAddress>(contract_address)?;
		let ledger = Self::get_ledger(api, state_key)?;

		ledger
			.get_contract_state(addr)
			.map_or(Err(LedgerApiError::ContractNotPresent), f)
	}

	pub fn get_contract_state(
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();

		let f = |contract_state| api.tagged_serialize(&contract_state);

		Self::do_get_contract_state(&api, state_key, contract_address, f)
	}

	pub fn get_zswap_chain_state(
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let addr = api.deserialize::<ContractAddress>(contract_address)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		api.tagged_serialize(&ledger.get_zswap_state(Some(addr)))
	}

	pub fn get_zswap_state_root(state_key: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;

		api.serialize(&ledger.get_zswap_state_root())
	}

	pub fn get_ledger_state_root(state_key: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ledger_state = default_storage::<D>().arena.alloc(ledger.state.clone());
		api.serialize(&ledger_state.as_typed_key())
	}

	/// Serialize the full ledger arena snapshot at `state_key` into the canonical, `Ledger`-rooted
	/// transfer blob used by trustless warp ledger-sync: `derived_tag_prefix ‖
	/// TopoSortedNodes(Ledger DAG)`.
	///
	/// Mirrors the single-pass technique of the toolkit's `serialize_ledger_state_fast`, but roots
	/// at `Ledger` (the `Sp` from `get_ledger` is an `Sp<Ledger>`) rather than `LedgerState`.
	/// Because the blob is rooted at `Ledger`, its recomputed content-address root key equals the
	/// on-chain `pallet_midnight::StateKey`, which is exactly what the client verifies against. The
	/// tag prefix is **derived** (`GLOBAL_TAG ‖ <Ledger as Tagged>::tag()`), never hardcoded, so it
	/// stays in lockstep with the ledger serialization format.
	pub fn serialize_ledger_snapshot(state_key: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
		use ledger_storage_local::arena::TopoSortedNodes;
		use midnight_serialize_local::{GLOBAL_TAG, Serializable};
		use types::SerializationError;

		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;

		// One `serialize_to_node_list()` pass (the derived `Serializable` impl would do two — once
		// for `serialized_size`, once for `serialize` — each a full topo-sort of a multi-million
		// node DAG), written directly. Byte-identical to the default impl's output.
		let nodes: TopoSortedNodes = ledger.serialize_to_node_list();
		let tag_prefix = format!("{}{}:", GLOBAL_TAG, <Ledger<D> as Tagged>::tag());
		let mut bytes = Vec::with_capacity(tag_prefix.len() + nodes.serialized_size());
		bytes.extend_from_slice(tag_prefix.as_bytes());
		nodes.serialize(&mut bytes).map_err(|e| {
			log::error!(target: LOG_TARGET, "Failed to serialize ledger snapshot: {e:?}");
			LedgerApiError::Serialization(SerializationError::LedgerState)
		})?;
		Ok(bytes)
	}

	/// Import a verified, `Ledger`-rooted warp snapshot `blob` into the already-open arena backend,
	/// binding it to the trie anchor `expected_state_key` (the on-chain `pallet_midnight::StateKey`
	/// the warp-recovered trie already holds).
	///
	/// Reconstruction uses the arena's **native multi-pass deserializer**
	/// (`Arena::deserialize_sp`, designed for untrusted wire input — it re-hashes every node), then
	/// asserts the reconstructed root key equals `expected_state_key` before persisting. So a
	/// malicious or faulty peer can at worst cause a rejected import (→ peer report + retry by the
	/// caller), never state corruption.
	///
	/// Persists + flushes into the live `default_storage` so `get_lazy(StateKey)` resolves —
	/// in-process, no restart, via the same `alloc`/`persist`/`flush` path live block execution
	/// uses. The caller (warp client driver) MUST hold the authoring/import gate so no block
	/// executes against the arena concurrently — the arena is single-writer.
	pub fn import_verified_ledger_snapshot(
		blob: &[u8],
		expected_state_key: &[u8],
	) -> Result<(), crate::SnapshotImportError> {
		use crate::SnapshotImportError;

		let api = api::new();
		let expected: TypedArenaKey<Ledger<D>, D::Hasher> = api
			.tagged_deserialize(expected_state_key)
			.map_err(|e| SnapshotImportError::StateKeyDecode(format!("{e:?}")))?;

		// Native verifying (untrusted-safe) deserialize of the `Ledger`-rooted blob into the live
		// arena; re-allocating the loaded value yields the persistable `Sp`.
		let ledger: Ledger<D> =
			helpers_local::deserialize(blob).map_err(SnapshotImportError::Deserialize)?;
		let mut sp = default_storage::<D>().arena.alloc(ledger);

		// Cryptographic bind to the trie anchor: the reconstructed root must equal the on-chain
		// `StateKey`. This is the whole security argument — reject anything else.
		let computed: TypedArenaKey<Ledger<D>, D::Hasher> = sp.as_typed_key();
		if computed != expected {
			return Err(SnapshotImportError::RootMismatch);
		}

		sp.persist();
		default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
		log::info!(target: LOG_TARGET, "Imported verified ledger snapshot ({} bytes)", blob.len());
		Ok(())
	}

	pub fn get_unclaimed_amount(
		state_key: &[u8],
		beneficiary: &[u8],
	) -> Result<u128, LedgerApiError> {
		let api = api::new();

		let night_addr = api.night_address(beneficiary)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		ledger
			.get_unclaimed_amount(night_addr)
			.copied()
			.ok_or(LedgerApiError::BeneficiaryNotFound)
	}

	pub fn get_bridge_receiving_amount(
		state_key: &[u8],
		beneficiary: &[u8],
	) -> Result<u128, LedgerApiError> {
		let api = api::new();

		let night_addr = api.night_address(beneficiary)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		ledger
			.get_bridge_receiving_amount(night_addr)
			.copied()
			.ok_or(LedgerApiError::BeneficiaryNotFound)
	}

	pub fn get_ledger_parameters(state_key: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ledger_parameters = Self::get_deserialized_ledger_parameters(&ledger);
		api.tagged_serialize(&ledger_parameters)
	}

	pub fn get_c_to_m_bridge_min_amount(state_key: &[u8]) -> Result<u128, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ledger_parameters = Self::get_deserialized_ledger_parameters(&ledger);
		Ok(ledger_parameters.c_to_m_bridge_min_amount)
	}

	pub fn get_transaction_cost(
		state_key: &[u8],
		tx: &[u8],
		_block_context: &BlockContext,
		max_weight: u64,
	) -> Result<GasCost, LedgerApiError> {
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let cost =
			tx.0.cost(&ledger.state.parameters, true)
				.map_err(|_| LedgerApiError::FeeCalculationError)?;

		log::trace!(target: LOG_TARGET, "⏱️  Estimated cost: {cost:?}");

		let limits = ledger.state.parameters.limits.block_limits;
		let normalized = cost.normalize(limits).ok_or(LedgerApiError::BlockLimitExceededError)?;

		log::trace!(target: LOG_TARGET, "⏱️  Normalized cost: {normalized:?}");

		let gas_cost = scale_normalized_cost(&normalized, max_weight);

		Ok(gas_cost)
	}

	/// The cost of either a `Transaction` or a [`SystemTransaction`], dispatched on
	/// the serialized header tag.
	///
	/// `get_transaction_cost` above only ever priced user transactions; its
	/// semantics are frozen by finalized blocks, so the widening lives here.
	pub fn get_any_transaction_cost(
		state_key: &[u8],
		tx: &[u8],
		block_context: &BlockContext,
		max_weight: u64,
	) -> Result<GasCost, LedgerApiError> {
		if is_system_transaction(tx) {
			Self::get_system_transaction_cost(state_key, tx, max_weight)
		} else {
			Self::get_transaction_cost(state_key, tx, block_context, max_weight)
		}
	}

	/// `SystemTransaction::cost` is pure and infallible — it takes only the ledger
	/// parameters, with no `enforce_time_to_dismiss` flag and no apply — so a system
	/// transaction can be priced ahead of being applied. `Ledger::apply_system_tx`
	/// computes the same figure on the way in.
	///
	/// Note that `From<RunningCost> for SyntheticCost` hardcodes `block_usage: 0`, so
	/// a system transaction never reports that dimension; the binding one is
	/// whichever of the remaining four normalizes highest.
	fn get_system_transaction_cost(
		state_key: &[u8],
		tx: &[u8],
		max_weight: u64,
	) -> Result<GasCost, LedgerApiError> {
		let api = api::new();
		let tx = api.tagged_deserialize::<SystemTransaction>(tx)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let cost = tx.cost(&ledger.state.parameters);

		log::trace!(target: LOG_TARGET, "⏱️  Estimated system tx cost: {cost:?}");

		let limits = ledger.state.parameters.limits.block_limits;
		let normalized = cost.normalize(limits).ok_or(LedgerApiError::BlockLimitExceededError)?;

		log::trace!(target: LOG_TARGET, "⏱️  Normalized system tx cost: {normalized:?}");

		Ok(scale_normalized_cost(&normalized, max_weight))
	}

	fn get_deserialized_ledger_parameters(state: &Ledger<D>) -> LedgerParameters {
		state.get_parameters()
	}

	/// Load the ledger state `state_key` addresses.
	///
	/// Every read and write path funnels through here, including the raw-`&[u8]` callers
	/// that run mid-block against an intermediate tip (`pre_dispatch` and the weight
	/// macro). A state retained by the keep-alive caches costs nothing to "load": the
	/// lazy `Sp` this returns resolves its first deref through the arena's `sp_cache`
	/// straight to the retained `Arc`, skipping deserialization of the whole working
	/// set. The temporary handle is refcount-balanced (`Sp::lazy` increments,
	/// `Sp::drop` decrements), so it cannot uncache a state we are still retaining.
	fn get_ledger(api: &api::Api, state_key: &[u8]) -> Result<Sp<Ledger<D>, D>, LedgerApiError> {
		let key: TypedArenaKey<Ledger<D>, D::Hasher> = api.tagged_deserialize(state_key)?;
		let storage = default_storage::<D>();
		// `get_lazy` returns `Ok` even for a key that isn't in the arena at all; the
		// failure only surfaces later, as a panic in `force_as_arc` ("root should be in
		// the arena"). Probe the root node first — one node lookup, no DAG traversal —
		// so an unresolvable state key is a clean `NoLedgerState`. `has_ledger_state`
		// relies on this. Valid for never-persisted states too: the backend's `get`
		// consults the in-memory write cache, `Create` entries included.
		//
		// Only for a `Ref` key. A `Direct` one carries the node inline (small roots are
		// embedded rather than stored), so it is never in the arena by hash and there is
		// nothing to probe.
		if let ArenaKey::Ref(hash) = &key.key
			&& !storage.with_backend(|backend| backend.get(hash).is_some())
		{
			log::error!(target: LOG_TARGET, "Ledger State {} not in arena", hex::encode(hash.0));
			return Err(LedgerApiError::NoLedgerState);
		}
		storage.arena.get_lazy(&key).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error loading Ledger State: {e:?}");
			LedgerApiError::NoLedgerState
		})
	}

	fn get_transaction_details(
		tx: &Transaction<S, D>,
		_ledger: &Ledger<D>,
		tx_gas_cost: GasCost,
	) -> Result<TransactionDetails, LedgerApiError> {
		let ledger_tx = &tx.0;

		match ledger_tx {
			LedgerTransaction::Standard(tx) => {
				let guaranteed_coins = GuaranteedCoinsDetails::new(
					tx.guaranteed_inputs().count() as u32,
					tx.guaranteed_outputs().count() as u32,
					tx.guaranteed_transients().count() as u32,
				);

				let fallible_coins_details = FallibleCoinsDetails::new(
					tx.fallible_inputs().count() as u32,
					tx.fallible_outputs().count() as u32,
					tx.fallible_transients().count() as u32,
				);

				let mut contract_calls = tx.actions().try_fold(
					ContractCallsDetails::default(),
					|mut cd, (_segment, action)| {
						match action {
							ContractAction::Call(_) => {
								cd.inc_calls();
							},
							ContractAction::Deploy(_) => {
								cd.inc_deploys();
							},
							ContractAction::Maintain(MaintenanceUpdate { updates, .. }) => {
								for update in updates.iter() {
									match *update {
										SingleUpdate::ReplaceAuthority(..) => {
											cd.inc_replace_authority();
										},
										SingleUpdate::VerifierKeyInsert(..) => {
											cd.inc_verifier_key_insert();
										},
										SingleUpdate::VerifierKeyRemove(..) => {
											cd.inc_verifier_key_remove();
										},
										// Ledger 9+ adds IrInsert/IrRemove (on-chain IR maintenance).
										// This match is shared across ledger versions, so the variants
										// can't be named here (they don't exist in L8's SingleUpdate);
										// they're not yet broken out in ContractCallsDetails telemetry.
										// TODO: support IrInsert/IrRemove
										#[allow(unreachable_patterns)]
										_ => {},
									}
								}
							},
						};
						Ok(cd)
					},
				)?;

				contract_calls.set_gas_cost(tx_gas_cost);

				Ok(TransactionDetails::Standard {
					guaranteed_coins,
					fallible_coins: fallible_coins_details,
					contract_calls,
				})
			},
			LedgerTransaction::ClaimRewards(_) => Ok(TransactionDetails::ClaimRewards),
		}
	}

	/// Calculate tx hash to be used in the `TX_VALIDATION_CACHE`
	/// `runtime_version` is prepended to differentiate tx validity between versions
	fn tx_validation_cache_key(runtime_version: u32, tx_serialized: &[u8]) -> WrappedHash {
		let to_hash = [&runtime_version.to_le_bytes(), tx_serialized].concat();
		Twox128::hash(&to_hash).into()
	}

	fn get_tx_type(tx: &Transaction<S, D>) -> &'static str {
		match tx.0 {
			mn_ledger_local::structure::Transaction::Standard(_) => "standard",
			mn_ledger_local::structure::Transaction::ClaimRewards(_) => "claim_rewards",
		}
	}

	fn get_system_tx_type(tx: &SystemTransaction) -> Result<&'static str, LedgerApiError> {
		get_system_tx_type(tx)
	}

	/// Gets a VerifiedTransaction, using the strict cache when possible.
	///
	/// - Checks the strict cache (keyed by state_hash + tx_hash)
	/// - On hit: returns cached VerifiedTransaction
	/// - On miss: calls well_formed(), caches result in both caches, returns it
	fn get_verified_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		skew_tblock: bool,
	) -> Result<VerifiedTransaction<D>, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let strict_key = strict_cache_key(ledger, block_context, tx_hash, skew_tblock);

		// Check strict cache
		if let Some(cached) = STRICT_TX_VALIDATION_CACHE.get(&strict_key) {
			if let Some(vt) = cached.downcast_ref::<VerifiedTransaction<D>>() {
				return Ok(vt.clone());
			}
			// Downcast failed - fall through to recompute
			log::warn!(target: LOG_TARGET, "VerifiedTransaction cache downcast failed");
		}

		// Cache miss: compute VerifiedTransaction
		let ctx = ledger.get_transaction_context(block_context.clone())?;

		let verified_tx =
			tx.0.well_formed(
				&ctx.ref_state,
				mn_ledger_local::verify::WellFormedStrictness::default(),
				Timestamp::from_secs(strict_key.well_formed_tblock),
			)
			.map_err(|e| {
				log::warn!(
					target: LOG_TARGET,
					"Transaction malformed: {e}",
				);
				LedgerApiError::Transaction(types::TransactionError::Malformed(e.into()))
			})?;

		// Cache in strict cache (soft cache is managed by do_validate_transaction)
		STRICT_TX_VALIDATION_CACHE.insert(strict_key, Arc::new(verified_tx.clone()));

		Ok(verified_tx)
	}

	/// Validates a transaction for the mempool using the soft cache.
	///
	/// Uses `tx_hash` only for quick revalidation of transactions already in the pool.
	/// The soft cache prevents redundant ZK proof verification for mempool housekeeping.
	///
	/// Returns `true` if the validation was served from cache, `false` if validation was performed.
	fn do_validate_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
	) -> Result<bool, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let soft_key = SoftTxValidationKey { tx_hash: tx_hash.0 };

		// Check soft cache first (quick tx_hash-only lookup for mempool revalidation)
		if let Some(cached) = SOFT_TX_VALIDATION_CACHE.get(&soft_key) {
			return cached.map(|_| true);
		}

		// Cache miss: transaction is entering the mempool or being re-validated
		let tx_hash_hex = hex::encode(tx.hash());
		let verified_tx =
			match Self::get_verified_transaction(ledger, tx, block_context, tx_hash, false) {
				Ok(vt) => vt,
				Err(e) => {
					log::warn!(
						target: LOG_TARGET,
						"🚫 Rejected transaction {} from mempool: {e}",
						tx_hash_hex
					);
					return Err(e);
				},
			};

		// Dry-run the guaranteed segment against the current state.
		let ctx = ledger.get_transaction_context(block_context.clone())?;

		match super::guaranteed_validation::validate_guaranteed_execution(
			&ledger.state,
			verified_tx,
			&ctx,
		) {
			Ok(()) => {
				log::info!(
					target: LOG_TARGET,
					"📋 Validated transaction {} for mempool",
					tx_hash_hex
				);
				// Cache the success (only successes are cached)
				SOFT_TX_VALIDATION_CACHE.insert(soft_key, Ok(()));
				Ok(false)
			},
			Err(reason) => {
				log::warn!(
					target: LOG_TARGET,
					"🚫 Rejected transaction {} from mempool: guaranteed execution would fail: {reason:?}",
					tx_hash_hex
				);
				// Do NOT cache failures — tx will be fully re-checked on next revalidation
				Err(LedgerApiError::Transaction(types::TransactionError::Invalid(reason.into())))
			},
		}
	}

	/// Validates transaction application, with caching.
	///
	/// Uses `get_verified_transaction` to get a cached or freshly computed
	/// `VerifiedTransaction`, then dry-runs guaranteed execution (via the
	/// version-specific `guaranteed_validation` module) to validate that the
	/// transaction can enter a block.
	///
	/// Returns `true` if validation was served from the strict cache, `false` otherwise.
	fn do_validate_guaranteed_execution(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		skew_tblock: bool,
	) -> Result<bool, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		// Invalidate soft cache — tx must re-validate after a block authoring attempt
		SOFT_TX_VALIDATION_CACHE.invalidate(&SoftTxValidationKey { tx_hash: tx_hash.0 });

		// Check strict cache to determine if this is a cache hit
		let strict_key = strict_cache_key(ledger, block_context, tx_hash, skew_tblock);
		let was_cached = STRICT_TX_VALIDATION_CACHE.get(&strict_key).is_some();

		let verified_tx =
			Self::get_verified_transaction(ledger, tx, block_context, tx_hash, skew_tblock)?;

		let ctx = ledger.get_transaction_context(block_context.clone())?;

		match super::guaranteed_validation::validate_guaranteed_execution(
			&ledger.state,
			verified_tx,
			&ctx,
		) {
			Ok(()) => Ok(was_cached),
			Err(reason) => {
				log::warn!(
					target: LOG_TARGET,
					"🚫 Rejecting transaction {} at pre-dispatch: guaranteed execution would fail: {reason:?}",
					hex::encode(tx.hash())
				);
				Err(LedgerApiError::Transaction(types::TransactionError::Invalid(reason.into())))
			},
		}
	}

	pub fn construct_cnight_generates_dust_event(
		value: u128,
		owner: &[u8],
		time: u64,
		action: u8,
		nonce: [u8; 32],
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let event = CNightGeneratesDustEvent {
			value,
			owner: api.deserialize(owner)?,
			time: Timestamp::from_secs(time),
			action: match action {
				0 => Ok(CNightGeneratesDustActionType::Create),
				1 => Ok(CNightGeneratesDustActionType::Destroy),
				_ => Err(LedgerApiError::Deserialization(
					api::DeserializationError::CNightGeneratesDustActionType,
				)),
			}?,
			nonce: InitialNonce(HashOutput(nonce)),
		};
		api.tagged_serialize(&event)
	}

	pub fn is_governance_allowed_system_tx(tx_serialized: &[u8]) -> bool {
		let api = api::new();
		let Ok(tx) = api.tagged_deserialize::<SystemTransaction>(tx_serialized) else {
			return false;
		};
		matches!(tx, SystemTransaction::OverwriteParameters(_))
	}

	pub fn construct_cnight_generates_dust_system_tx(
		events: Vec<Vec<u8>>,
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let events: Result<Vec<CNightGeneratesDustEvent>, LedgerApiError> =
			events.iter().map(|e| api.tagged_deserialize(e)).collect();
		let system_tx = SystemTransaction::CNightGeneratesDustUpdate { events: events? };
		api.tagged_serialize(&system_tx)
	}

	pub fn construct_distribute_night_cardano_bridge_system_tx(
		amount: u128,
		target_address_bytes: &[u8],
		nonce_bytes: [u8; 32],
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let target_address = api.night_address(target_address_bytes)?;
		let output = OutputInstructionUnshielded {
			amount,
			target_address,
			nonce: Nonce(HashOutput(nonce_bytes)),
		};
		let system_tx = SystemTransaction::DistributeNight(ClaimKind::CardanoBridge, vec![output]);
		api.tagged_serialize(&system_tx)
	}

	pub fn construct_distribute_reserve_system_tx(amount: u128) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let system_tx = super::system_tx::distribute_reserve_system_tx(amount);
		api.tagged_serialize(&system_tx)
	}

	pub fn construct_unlock_to_treasury_system_tx(amount: u128) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let system_tx = super::system_tx::unlock_to_treasury_system_tx(amount)?;
		api.tagged_serialize(&system_tx)
	}

	pub fn construct_distribute_treasury_system_tx(
		amount: u128,
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let system_tx = super::system_tx::distribute_treasury_system_tx(amount)?;
		api.tagged_serialize(&system_tx)
	}
}

/// True when `bytes` is a tagged-serialized [`SystemTransaction`] rather than a
/// `Transaction`. Mirrors `crate::is_ledger_8_state_key`: `peek_tag` reads the
/// serialized header tag without deserializing the body.
///
/// Tested *for* `SystemTransaction` rather than against `Transaction`'s tag because
/// the latter is generic-instantiated over signature and proof markers. The tag is
/// taken from `Tagged`, never written out as a literal — it carries a version
/// (`system-transaction[vN]`) that a literal would silently outlive.
#[cfg(feature = "std")]
fn is_system_transaction(bytes: &[u8]) -> bool {
	let expected = <SystemTransaction as Tagged>::tag();
	match midnight_serialize_local::peek_tag(&mut std::io::Cursor::new(bytes)) {
		Ok(tag) => tag.as_str() == expected.as_ref(),
		Err(_) => false,
	}
}

#[cfg(feature = "std")]
fn get_system_tx_type(tx: &SystemTransaction) -> Result<&'static str, LedgerApiError> {
	match tx {
		SystemTransaction::OverwriteParameters(_) => Ok("overwrite_parameters"),
		SystemTransaction::DistributeNight(claim_kind, _) => match claim_kind {
			ClaimKind::Reward => Ok("distribute_night_reward"),
			ClaimKind::CardanoBridge => Ok("distribute_night_cardano_bridge"),
		},
		SystemTransaction::PayBlockRewardsToTreasury { .. } => Ok("pay_block_rewards_to_treasury"),
		SystemTransaction::PayFromTreasuryShielded { .. } => Ok("pay_from_treasury_shielded"),
		SystemTransaction::PayFromTreasuryUnshielded { .. } => Ok("pay_from_treasury_unshielded"),
		tx if super::system_tx::is_distribute_reserve_system_tx(tx) => Ok("distribute_reserve"),
		tx if super::system_tx::is_unlock_to_treasury_system_tx(tx) => Ok("unlock_to_treasury"),
		SystemTransaction::CNightGeneratesDustUpdate { .. } => Ok("cnight_generates_dust_update"),
		other => {
			log::error!(
				target: LOG_TARGET,
				"Unsupported system transaction type: {other:?}"
			);
			Err(LedgerApiError::Transaction(types::TransactionError::SystemTransaction(
				types::SystemTransactionError::UnknownError,
			)))
		},
	}
}

/// Creates a Nonce using BlakeTwo256; similar Hashing type set in the Runtime.
///
/// # Arguments
/// * `separator` - an indicator from which this nonce belongs to.
/// * `block_hash`
/// * `output_number` - its position in the list
#[cfg(feature = "std")]
#[allow(dead_code)]
fn create_nonce(separator: &[u8], block_hash: &[u8], output_number: u8) -> Nonce {
	use sp_runtime::traits::{BlakeTwo256, Hash};

	let concatenated = [block_hash, separator, &[output_number]].concat();

	let h256 = BlakeTwo256::hash(&concatenated);

	Nonce(HashOutput(h256.0))
}

/// `slot_duration_secs * (1 + MaxSkippedSlots)` = 6 * 2. Fixed for every chain this correction
/// can apply to: those blocks were produced under `SLOT_DURATION = 6s` and `MaxSkippedSlots = 1`.
#[cfg(feature = "std")]
const TBLOCK_CORRECTION_OFFSET_SECS: i128 = 12;

/// The `tblock` to run `well_formed` against.
///
/// Historical blocks can contain a *first* transaction whose `ctime` runs ahead of the block
/// timestamp: the producing node served that transaction's `well_formed` result from the strict
/// cache, where it had been verified during mempool ingress at
/// `ParentTimestamp + slot_duration * (1 + MaxSkippedSlots)` (see
/// `<pallet_midnight::Pallet as ValidateUnsigned>::validate_unsigned`). Reproduce that exact
/// timestamp — and only for the first ledger tx in a block, which is the only position where
/// that cache could hit — so those blocks still import.
///
/// A transaction only reaches a block through the producing node's own pool, so by the time that
/// node ran `pre_dispatch` the strict cache was always warm for it: the pool verified it at
/// `parent + offset` against the parent's post-block state, which is exactly the state and key
/// `pre_dispatch` then looked up. The first ledger tx in a block was therefore *always* verified
/// at `parent + offset`, never at the block's own timestamp — so this is a single unconditional
/// rule, a total function of `(block_context, is_block_start)` evaluated identically on every
/// node, with no try-then-retry branch for consensus to depend on.
///
/// The loophole is gated on the host-function version, not a date: version 1 of
/// `apply_transaction`/`validate_guaranteed_execution` passes `skew_tblock = true`, version 2
/// passes `false`. Historical blocks replay against whichever runtime was on-chain at that
/// height, so pre-upgrade wasm imports v1 and still corrects; from the `set_code` block onward
/// the new wasm imports v2 and the loophole is closed. No clock, no node config.
///
/// See <https://github.com/midnightntwrk/midnight-node/issues/1924>
#[cfg(feature = "std")]
fn well_formed_tblock<D: DB>(
	ledger: &Ledger<D>,
	block_context: &BlockContext,
	skew_tblock: bool,
) -> Timestamp {
	if skew_tblock
		&& ledger.is_block_start()
		&& let Some(parent_block_time) = block_context.parent_block_time()
	{
		Timestamp::from_secs(parent_block_time)
			+ DurationLedger::from_secs(TBLOCK_CORRECTION_OFFSET_SECS)
	} else {
		Timestamp::from_secs(block_context.tblock)
	}
}

/// Key for a `VerifiedTransaction` in the strict cache.
///
/// Keyed on the timestamp [`well_formed_tblock`] resolves to rather than `block_context.tblock`,
/// so a transaction verified under the correction can never be served to a caller that asked for
/// the uncorrected timestamp, or vice versa. When the correction is inert the two agree and the
/// entry is shared, which is exactly when sharing is sound.
#[cfg(feature = "std")]
fn strict_cache_key<D: DB>(
	ledger: &Ledger<D>,
	block_context: &BlockContext,
	tx_hash: &WrappedHash,
	skew_tblock: bool,
) -> StrictTxValidationKey
where
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	StrictTxValidationKey {
		state_hash: ledger.state.state_hash().0.into(),
		tx_hash: tx_hash.0,
		well_formed_tblock: well_formed_tblock(ledger, block_context, skew_tblock).to_secs(),
	}
}

#[cfg(feature = "std")]
fn scale_normalized_cost(normalized: &LedgerNormalizedCost, max_weight: u64) -> GasCost {
	let max_fp = *[
		normalized.read_time,
		normalized.compute_time,
		normalized.block_usage,
		normalized.bytes_written,
		normalized.bytes_churned,
	]
	.iter()
	.max()
	.expect("Hard-coded array should not be empty");

	max_fp.into_atomic_units(max_weight as u128).min(max_weight as u128) as u64
}

#[cfg(test)]
mod tests {
	use super::*;
	use base_crypto_local::cost_model::{FixedPoint, SyntheticCost};
	use coin_structure_local::coin::{ShieldedTokenType, UnshieldedTokenType};
	use ledger_storage_local::DefaultDB;
	use mn_ledger_local::structure::LedgerState;

	/// Preview #128537, the block whose first transaction motivated the correction.
	const BLOCK_TBLOCK: u64 = 1784987076;

	fn block_context() -> BlockContext {
		BlockContext { tblock: BLOCK_TBLOCK, ..Default::default() }
	}

	/// A ledger at the start of a block: nothing applied, `block_fullness` still zero.
	fn ledger_at_block_start() -> Ledger<DefaultDB> {
		Ledger::new(LedgerState::new("undeployed"))
	}

	/// A ledger mid-block: a transaction has already accrued `block_fullness`.
	fn ledger_mid_block() -> Ledger<DefaultDB> {
		let non_zero = SyntheticCost { block_usage: 1, ..SyntheticCost::ZERO };
		Ledger::new_with_block_fullness(LedgerState::new("undeployed"), non_zero)
	}

	#[test]
	fn well_formed_tblock_corrects_the_first_tx_in_a_historical_block() {
		let bc = block_context();
		let tblock = well_formed_tblock(&ledger_at_block_start(), &bc, true);

		// The tx is verified at the parent's timestamp plus the mempool skew, reproducing
		// what the producing node's warm strict-cache entry was verified at.
		let parent = bc.parent_block_time().expect("post-ledger-8 contexts always carry one");
		assert_eq!(
			tblock,
			Timestamp::from_secs(parent) + DurationLedger::from_secs(TBLOCK_CORRECTION_OFFSET_SECS)
		);
		assert_ne!(
			tblock,
			Timestamp::from_secs(bc.tblock),
			"the corrected timestamp must not be the block's own tblock"
		);
	}

	/// What host-function version 2 — the one the upgraded runtime imports — asks for.
	#[test]
	fn well_formed_tblock_is_uncorrected_when_the_correction_is_off() {
		let bc = block_context();
		assert_eq!(
			well_formed_tblock(&ledger_at_block_start(), &bc, false),
			Timestamp::from_secs(BLOCK_TBLOCK),
		);
	}

	#[test]
	fn well_formed_tblock_is_uncorrected_after_the_first_tx_in_a_block() {
		let bc = block_context();
		assert_eq!(
			well_formed_tblock(&ledger_mid_block(), &bc, true),
			Timestamp::from_secs(BLOCK_TBLOCK),
		);
	}

	/// The two host-function versions must not share a cache entry when they verify at different
	/// timestamps — whichever ran first would otherwise hand the other a `VerifiedTransaction`
	/// checked against the wrong `tblock`.
	#[test]
	fn strict_cache_key_separates_corrected_from_uncorrected() {
		let bc = block_context();
		let tx_hash = WrappedHash([7u8; 32]);
		let key = |ledger, skew| strict_cache_key::<DefaultDB>(ledger, &bc, &tx_hash, skew);

		// The correction fires for the first tx of a block, so the keys must differ.
		let at_block_start = ledger_at_block_start();
		assert_ne!(key(&at_block_start, true), key(&at_block_start, false));

		// Mid-block the correction is inert either way, so sharing the entry is sound.
		let mid_block = ledger_mid_block();
		assert_eq!(key(&mid_block, true), key(&mid_block, false));
	}

	fn normalized_all(value: FixedPoint) -> LedgerNormalizedCost {
		LedgerNormalizedCost {
			read_time: value,
			compute_time: value,
			block_usage: value,
			bytes_written: value,
			bytes_churned: value,
		}
	}

	#[test]
	fn scale_normalized_cost_bounds_and_monotonic() {
		let max_weight = 100u64;

		let zero = scale_normalized_cost(&normalized_all(FixedPoint::from(0.0f64)), max_weight);
		let half = scale_normalized_cost(&normalized_all(FixedPoint::from(0.5f64)), max_weight);
		let one = scale_normalized_cost(&normalized_all(FixedPoint::from(1.0f64)), max_weight);
		let over_one = scale_normalized_cost(&normalized_all(FixedPoint::from(1.5f64)), max_weight);
		let negative =
			scale_normalized_cost(&normalized_all(FixedPoint::from(-0.25f64)), max_weight);

		assert_eq!(zero, 0);
		assert_eq!(negative, 0);
		assert!(half >= max_weight / 2 && half <= max_weight);
		assert_eq!(one, max_weight);
		assert_eq!(over_one, max_weight);
		assert!(half >= zero);
		assert!(one >= half);
	}

	/// `pallet_cnight_observation::migrations::v2` prices its
	/// `CNightGeneratesDustUpdate` batches through `get_any_transaction_cost`, and
	/// divides the figure out per `Create` to estimate what the next batch will cost.
	/// Pin both halves: the tag dispatch is real (v1's `get_transaction_cost` cannot
	/// deserialize these bytes at all), and the gas is linear in the `Create` count.
	#[test]
	fn system_transaction_cost_is_linear_in_creates() {
		use mn_ledger_local::dust::DustPublicKey;
		use transient_crypto_local::curve::Fr;

		if super::super::CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}

		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let api = api::new();

		// The undeployed genesis `Ledger`, in the process-default (in-memory) arena:
		// `get_any_transaction_cost` reads the batch's price out of that state's
		// `parameters`, which is the whole point of asking the ledger rather than
		// hardcoding a figure.
		let genesis = midnight_node_res::networks::MidnightNetwork::genesis_state(
			&midnight_node_res::networks::UndeployedNetwork,
		);
		let state: LedgerState<DefaultDB> =
			midnight_serialize_local::tagged_deserialize(genesis).unwrap();
		let mut ledger = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		ledger.persist();
		let state_key = api.tagged_serialize(&ledger.as_typed_key()).unwrap();
		let batch = |creates: u8| -> Vec<u8> {
			let events = (0..creates)
				.map(|i| CNightGeneratesDustEvent {
					value: 1_000,
					owner: DustPublicKey(Fr::from(7u64)),
					time: Timestamp::from_secs(1_800_000_000),
					action: CNightGeneratesDustActionType::Create,
					nonce: InitialNonce(HashOutput([i; 32])),
				})
				.collect();
			api.tagged_serialize(&SystemTransaction::CNightGeneratesDustUpdate { events })
				.expect("serialize system tx")
		};

		// `res/cfg/default.toml`'s `max_block` ref_time, so the gas figures below are
		// in the units the MBM `WeightMeter` actually spends.
		const MAX_BLOCK: u64 = 2_000_000_000_000;
		let block_context = BlockContext::default();
		let gas = |creates: u8| {
			TestBridge::get_any_transaction_cost(
				&state_key,
				&batch(creates),
				&block_context,
				MAX_BLOCK,
			)
			.expect("system transaction must be priceable")
		};

		let one = gas(1);
		assert!(one > 0, "a dust `Create` must cost something");
		for n in [2u8, 5, 25] {
			// Linear to within the fixed-point rounding `into_atomic_units` does once
			// per call — at most 1ps per `Create`, against ~9e9ps each.
			assert!(
				gas(n).abs_diff(one * n as u64) <= n as u64,
				"batch gas must be linear in the `Create` count, so the migration can \
				 divide it out per nonce: {} creates cost {}, one costs {one}",
				n,
				gas(n),
			);
		}
		// Logged rather than asserted: which cost dimension binds is the live
		// parameters' call, and the migration paces itself off whatever they say.
		println!(
			"per-`Create` gas: {one} ref_time; a full batch of 25 costs {}; \
			 80% of a {MAX_BLOCK} block affords {} `Create`s",
			one * 25,
			(MAX_BLOCK / 100 * 80) / one,
		);

		// The dispatch is doing real work: these bytes are not a `Transaction`.
		assert!(
			matches!(
				TestBridge::get_transaction_cost(&state_key, &batch(1), &block_context, MAX_BLOCK),
				Err(LedgerApiError::Deserialization(_))
			),
			"v1 must still reject system transactions",
		);
	}

	#[test]
	fn get_system_tx_type_distribute_night_reward() {
		let tx = SystemTransaction::DistributeNight(ClaimKind::Reward, vec![]);
		assert_eq!(get_system_tx_type(&tx).unwrap(), "distribute_night_reward");
	}

	#[test]
	fn get_system_tx_type_distribute_night_cardano_bridge() {
		let tx = SystemTransaction::DistributeNight(ClaimKind::CardanoBridge, vec![]);
		assert_eq!(get_system_tx_type(&tx).unwrap(), "distribute_night_cardano_bridge");
	}

	#[test]
	fn get_system_tx_type_pay_block_rewards_to_treasury() {
		let tx = SystemTransaction::PayBlockRewardsToTreasury { amount: 0 };
		assert_eq!(get_system_tx_type(&tx).unwrap(), "pay_block_rewards_to_treasury");
	}

	#[test]
	fn get_system_tx_type_pay_from_treasury_shielded() {
		let tx = SystemTransaction::PayFromTreasuryShielded {
			outputs: vec![],
			nonce: HashOutput([0u8; 32]),
			token_type: ShieldedTokenType(HashOutput([0u8; 32])),
		};
		assert_eq!(get_system_tx_type(&tx).unwrap(), "pay_from_treasury_shielded");
	}

	#[test]
	fn get_system_tx_type_pay_from_treasury_unshielded() {
		let tx = SystemTransaction::PayFromTreasuryUnshielded {
			outputs: vec![],
			token_type: UnshieldedTokenType(HashOutput([0u8; 32])),
		};
		assert_eq!(get_system_tx_type(&tx).unwrap(), "pay_from_treasury_unshielded");
	}

	#[test]
	fn get_system_tx_type_distribute_reserve() {
		let tx = super::super::system_tx::distribute_reserve_system_tx(0);
		assert_eq!(get_system_tx_type(&tx).unwrap(), "distribute_reserve");
	}

	#[test]
	fn get_system_tx_type_cnight_generates_dust_update() {
		let tx = SystemTransaction::CNightGeneratesDustUpdate { events: vec![] };
		assert_eq!(get_system_tx_type(&tx).unwrap(), "cnight_generates_dust_update");
	}

	#[test]
	fn get_system_tx_type_unlock_to_treasury() {
		if let Ok(tx) = super::super::system_tx::unlock_to_treasury_system_tx(0) {
			assert_eq!(get_system_tx_type(&tx).unwrap(), "unlock_to_treasury");
		}
	}
}

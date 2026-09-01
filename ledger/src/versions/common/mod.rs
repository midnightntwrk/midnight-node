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
use sha2::digest::{OutputSizeUser, generic_array::typenum::U32};
#[cfg(feature = "std")]
use transient_crypto_local::commitment::PureGeneratorPedersen;

use alloc::vec::Vec;
#[cfg(feature = "std")]
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
	midnight_primitives_ledger::{
		LedgerMetricsExt, LedgerStorageDb, LedgerStorageExt, TBlockCorrection, TBlockCorrectionExt,
	},
	mn_ledger_local::{
		dust::InitialNonce,
		semantics::TransactionContext,
		structure::{
			CNightGeneratesDustActionType, CNightGeneratesDustEvent, ClaimKind, ContractAction,
			LedgerState, MaintenanceUpdate, OutputInstructionUnshielded, ProofMarker,
			SignatureKind, SingleUpdate, Transaction as LedgerTransaction, VerifiedTransaction,
		},
		verify::StateReference,
	},
	std::{
		any::{Any, TypeId},
		sync::Arc,
		time::{Duration, Instant},
	},
};

#[cfg(feature = "std")]
use crate::common::batch::BatchVerifyFailure;
use crate::common::types::{
	ContractCallsDetails, FallibleCoinsDetails, GasCost, GuaranteedCoinsDetails, Hash, Op,
	SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot, TransactionDetails, Tx,
};

use super::BlockContext;

#[cfg(feature = "std")]
use {lazy_static::lazy_static, moka::ops::compute::Op as CacheOp, moka::sync::Cache};

pub const LOG_TARGET: &str = "midnight::ledger_v2";
pub const MINT_COINS_DOMAIN_SEPARATOR: &[u8; 10] = b"mint_coins";

#[cfg(feature = "std")]
#[derive(PartialEq, Eq, Hash, Clone)]
struct TxValidationKey {
	runtime_version: u32,
	tx_hash: Hash,
}

#[cfg(feature = "std")]
struct TxValidationValue<D: DB> {
	verified_tx: VerifiedTransaction<D>,
	state: Sp<LedgerState<D>, D>,
	/// The timestamp [`Self::verified_tx`] was checked against — the effective `well_formed`
	/// tblock, which is not always `block_context.tblock` (see [`well_formed_tblock`]).
	///
	/// Deliberately *not* part of [`TxValidationKey`]: the mempool re-validates at a fresh
	/// timestamp on every block, so keying on it would turn every revalidation into a full
	/// cache miss. Kept here instead so a tblock change routes through
	/// `Bridge::revalidate_transaction`, which re-runs exactly the two time-dependent checks
	/// (intent TTL and the dust validity window — both under the ledger's
	/// `param_check(always = true)`) and skips the expensive stateless work.
	tblock: Timestamp,
}

#[cfg(feature = "std")]
enum TxValidationCacheOutcome {
	/// Found a valid cached VerifiedTransaction with reference to the current state.
	StrictCacheHit,
	/// Found a valid cached VerifiedTransaction with reference to the stale state.
	RevalidationHit,
	/// Full validation performed.
	CacheMiss,
}

#[cfg(feature = "std")]
impl TxValidationCacheOutcome {
	fn label(&self) -> &'static str {
		match self {
			Self::StrictCacheHit => "strict",
			Self::RevalidationHit => "revalidation",
			Self::CacheMiss => "miss",
		}
	}

	fn record_cache_metrics(&self, metrics: &mut LedgerMetricsExt) {
		match self {
			Self::StrictCacheHit | Self::RevalidationHit => {
				metrics.inc_tx_validation_cache_hit(self.label())
			},
			Self::CacheMiss => metrics.inc_tx_validation_cache_miss(),
		}
	}
}
/// Key for the proof-verification cache.
///
/// Uses only the state-independent (runtime_version, ledger tx hash) pair — the same fields as
/// [`TxValidationKey`] — so a batch-verified proof result survives any state or tblock drift
/// that would route the validation cache through revalidation or a full miss.
#[cfg(feature = "std")]
#[derive(PartialEq, Eq, Hash)]
pub struct ProofVerificationKey {
	runtime_version: u32,
	tx_hash: Hash,
}

/// Set this high to ensure that even large mempool sizes don't cause performance issues due to
/// unnecessary revalidation.
#[cfg(feature = "std")]
const TX_VALIDATION_CACHE_CAPACITY: u64 = 2000;

/// Capacity of the proof-verification cache.
/// Set at least as high as the validation cache (2000) so batch-verified proof results are never
/// evicted under mempool load before the downstream `verify_transaction` reads them.
#[cfg(feature = "std")]
const PROOF_VERIFICATION_CACHE_CAPACITY: u64 = 2000;

/// Time-to-idle for transaction validation cache entries.
/// Entries not accessed within this duration are evicted, preventing stale VerifiedTransaction
/// objects (which contain ZK proof data and can be 50-200 KiB each) from persisting indefinitely
/// on low-traffic networks. Without this, the cache only evicts by count — on quiet chains
/// entries live forever and contribute to steady-state memory growth.
#[cfg(feature = "std")]
const TX_VALIDATION_CACHE_TTI: Duration = Duration::from_secs(300);

#[cfg(feature = "std")]
lazy_static! {
	/// Cache: stores VerifiedTransaction for reuse across apply_transaction,
	/// validate_transaction, and validate_guaranteed_execution.
	///
	/// An entry records what `well_formed` proved about a transaction at a given state and
	/// timestamp — never a validity verdict. Nothing that happens to the transaction afterwards
	/// falsifies it, so nothing invalidates an entry: it is only ever evicted by capacity or TTI.
	/// Every read either strict-hits an identical state and timestamp, or re-runs the checks that
	/// can change (the revalidation delta and the `apply_guaranteed_only` dry-run).
	///
	/// Caching the verdict instead would be unsound: the same transaction is valid or not
	/// depending on the fork, the timestamp, and how much of the block is already built, and
	/// `pre_dispatch` reads this cache while *importing* blocks.
	///
	/// Rejected and already-applied transactions keep their entries for the same reason — a
	/// rejection is a fact about a state, not about the transaction, and a reorg that returns the
	/// transaction to the pool then revalidates it instead of re-verifying it from scratch.
	///
	/// We use `Arc<dyn Any + Send + Sync>` for type erasure because:
	/// - Bridge<S, D> is generic over Signature and Database types
	/// - Multiple signature types exist across ledger versions (e.g., Signature, SignatureHF)
	/// - Database type may vary (ParityDb, etc.)
	/// - A single static cache must store VerifiedTransaction for all type combinations
	///
	/// When retrieving, we downcast to the concrete TxValidationValue type.
	static ref TX_VALIDATION_CACHE: Cache<TxValidationKey, Arc<dyn Any + Send + Sync>> =
		Cache::builder()
			.max_capacity(TX_VALIDATION_CACHE_CAPACITY)
			.time_to_idle(TX_VALIDATION_CACHE_TTI)
			.build();

	/// Proof-verification cache: maps a state-independent tx key to its ZK-proof outcome.
	///
	/// Written exclusively by the batch-verification ingress points (the mempool worker pool and
	/// the block-import wrapper, via `Bridge::batch_verify_transactions`); read by
	/// `verify_transaction` so downstream consumers can skip the (now-deferred) ZK crypto.
	/// A cached `false` lets a downstream consumer reject a known-bad transaction. This cache is
	/// process-global (like the validation cache) and therefore not shared across processes.
	static ref PROOF_VERIFICATION_CACHE: Cache<ProofVerificationKey, bool> =
		Cache::builder()
			.max_capacity(PROOF_VERIFICATION_CACHE_CAPACITY)
			.time_to_idle(TX_VALIDATION_CACHE_TTI)
			.build();
}

/// Records the batch ZK-proof outcome for a transaction, keyed by the state-independent
/// (runtime_version, ledger tx hash) pair. Called only by the batch-verification ingress points.
#[cfg(feature = "std")]
fn insert_proof_result(key: &TxValidationKey, verified: bool) {
	PROOF_VERIFICATION_CACHE.insert(
		ProofVerificationKey { runtime_version: key.runtime_version, tx_hash: key.tx_hash },
		verified,
	);
}

/// Returns the cached ZK-proof outcome for a transaction, if an ingress point has verified it.
/// A `None` result is a performance signal (the caller should verify inline), not a correctness
/// failure.
#[cfg(feature = "std")]
fn get_proof_result(key: &TxValidationKey) -> Option<bool> {
	PROOF_VERIFICATION_CACHE
		.get(&ProofVerificationKey { runtime_version: key.runtime_version, tx_hash: key.tx_hash })
}

/// Current entry count of the proof-verification cache (for metrics/observability).
#[cfg(feature = "std")]
pub fn proof_verification_cache_size() -> u64 {
	PROOF_VERIFICATION_CACHE.entry_count()
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
		let cache_key = TxValidationKey { runtime_version, tx_hash };
		let tblock_ext = externalities.extension::<TBlockCorrectionExt>();
		let tblock_correction = tblock_ext.map(|e| &e.0);
		let (verified_tx, cache_outcome, inline_proof_verify) = Self::get_verified_transaction(
			&ledger,
			&tx,
			&block_context,
			&cache_key,
			tblock_correction,
		)?;
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
			cache_outcome.record_cache_metrics(metrics);
			metrics.set_tx_validation_cache_size("strict", TX_VALIDATION_CACHE.entry_count());
			// Fallback recording of the OFF-path per-tx baseline (`mode="inline"`). For the normal
			// unsigned `send_mn_transaction` flow this is `None`: FRAME runs `pre_dispatch`
			// (`validate_guaranteed_execution`) before dispatching the call, so the inline crypto has
			// already happened and been recorded there, leaving `get_verified_transaction` here a
			// strict-cache hit. This still records `Some` for any path that reaches
			// `apply_transaction` without a preceding `pre_dispatch` (e.g. direct application in tests).
			if let Some(pv) = inline_proof_verify {
				metrics.observe_inline_proof_verify(pv.as_secs_f64());
			}
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

		let cache_key = TxValidationKey { runtime_version, tx_hash: tx.hash() };
		// The pool's `and_provides` tag. Deliberately the state-independent Twox128 key — not the
		// ledger `tx.hash()` used for the validation cache — so it stays byte-for-byte identical
		// to the tag the node's batch-verification mempool path (`batch_chain_api`) builds
		// natively without deserializing the transaction.
		let provides_tag = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// No `tblock` correction on the mempool path: `validate_unsigned` already skews the
		// block context it passes here by `slot_duration * (1 + MaxSkippedSlots)`.
		let cache_outcome =
			Self::do_validate_transaction(&ledger, &tx, &block_context, &cache_key)?;

		let tx_details = if get_tx_details {
			let tx_gas_cost =
				Self::get_transaction_cost(state_key, tx_serialized, &block_context, max_weight)?;

			Some(Self::get_transaction_details(&tx, &ledger, tx_gas_cost)?)
		} else {
			None
		};

		// Write Prometheus metrics
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			let tx_type = Self::get_tx_type(&tx);
			let elapsed_time = start_tx_validation_time.elapsed().as_secs_f64();
			metrics.observe_txs_validating_time(elapsed_time, tx_type, cache_outcome.label());
			cache_outcome.record_cache_metrics(metrics);
			metrics.set_tx_validation_cache_size("strict", TX_VALIDATION_CACHE.entry_count());
		}

		Ok((provides_tag, tx_details))
	}

	/// Validates that applying a transaction will succeed.
	///
	/// Used by `pre_dispatch` to reject transactions whose application
	/// would fail - this keeps the block free of failed transactions.
	///
	/// This function checks the cache for a cached `VerifiedTransaction`
	/// (populated by `validate_unsigned(strict=true)`) to avoid redundant ZK
	/// proof verification via `well_formed()`.
	pub fn validate_guaranteed_execution(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let cache_key = TxValidationKey { runtime_version, tx_hash: tx.hash() };

		let tblock_ext = externalities.extension::<TBlockCorrectionExt>();
		let tblock_correction = tblock_ext.map(|e| &e.0);
		// Perform dry-run validation with caching
		let (cache_outcome, inline_proof_verify) = Self::do_validate_guaranteed_execution(
			&ledger,
			&tx,
			&block_context,
			&cache_key,
			tblock_correction,
		)?;

		// Write Prometheus metrics
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			cache_outcome.record_cache_metrics(metrics);

			// Records the OFF-path per-tx baseline (`mode="inline"`) only when this call actually ran
			// the ZK crypto inline (cold proof cache). `send_mn_transaction` is unsigned, so during
			// `execute_block` FRAME runs this `pre_dispatch` BEFORE the call's `apply_transaction`;
			// the crypto therefore happens here and warms the validation cache, leaving
			// `apply_transaction`'s `get_verified_transaction` a cache hit (`None`). Recording here is
			// what makes the inline baseline observable on the OFF block-import path.
			if let Some(pv) = inline_proof_verify {
				metrics.observe_inline_proof_verify(pv.as_secs_f64());
			}

			metrics.set_tx_validation_cache_size("strict", TX_VALIDATION_CACHE.entry_count());
		}

		Ok(())
	}

	/// Batch-verifies the ZK proofs of many transactions in a single aggregate crypto call and
	/// warms the process-global caches so downstream consumers can skip the (now-deferred) crypto.
	///
	/// This is the batch-verification ingress entry point, called **natively** by the node's
	/// mempool worker pool and block-import wrapper (never through the WASM host-function
	/// boundary). It is the write side of the ingress-vs-downstream rule: it *computes* proof
	/// results and *writes* the caches; it never reads them.
	///
	/// For each transaction it runs `well_formed` with proofs deferred (every stateless-non-proof,
	/// param, op and maintenance check still runs — only the ZK crypto is skipped), then collects
	/// the proof evidence and verifies all of it in one aggregate `batch_proof_verify` call.
	///
	/// On aggregate-verification success, for every transaction whose proofs verified it records
	/// `PROOF_VERIFICATION_CACHE = true`, dry-runs the guaranteed segment, and populates the
	/// validation cache — exactly the state a subsequent `validate_transaction` / `pre_dispatch` /
	/// `apply_transaction` would otherwise have to recompute.
	///
	/// On aggregate-verification failure the behaviour depends on `isolate_on_failure`, which is also
	/// what selects the ledger's `linear_revalidation` mode:
	/// - `true` (mempool): the ledger localizes the offending proofs; each named transaction gets
	///   `PROOF_VERIFICATION_CACHE = false` and an `Invalid` result, while the rest of the batch —
	///   which verified as part of the same aggregate check — is warmed as usual. Nothing is
	///   re-verified.
	/// - `false` (block import): the ledger spends no effort on attribution and this fails fast with
	///   an `Err`, so the whole block is rejected.
	///
	/// A failure the ledger cannot attribute to individual transactions (`Unlocalized`) fails the
	/// whole batch either way.
	///
	/// Returns one `Result<(), LedgerApiError>` per input transaction (in order); the outer `Err`
	/// signals a batch-wide failure (setup error, or an unattributable aggregate failure).
	pub fn batch_verify_transactions(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		txs_serialized: &[Vec<u8>],
		block_context: BlockContext,
		runtime_version: u32,
		isolate_on_failure: bool,
	) -> Result<Vec<Result<(), LedgerApiError>>, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let start_batch_time = Instant::now();

		if txs_serialized.is_empty() {
			return Ok(Vec::new());
		}

		// `isolate_on_failure` doubles as the ingress-point selector (see the doc comment above):
		// only the mempool asks for per-transaction localization, block import fails the batch fast.
		let is_mempool = isolate_on_failure;

		// Ensure the process-global ledger arena storage is initialized from the node's
		// `LedgerStorageExt`. This is idempotent (once-set), so it is a no-op whenever the runtime
		// has already initialized storage in this process.
		Self::set_default_storage(externalities);

		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ctx = ledger.get_transaction_context(block_context.clone())?;

		// Apply the same historical-sync tblock correction the per-transaction path uses, so the
		// non-crypto `well_formed` checks below — and the `VerifiedTransaction`s they produce — match
		// what `get_verified_transaction` would compute for these transactions.
		let tblock = {
			let tblock_correction = externalities.extension::<TBlockCorrectionExt>().map(|e| &e.0);
			if let Some(tc) = tblock_correction
				&& block_context.tblock < tc.disable_after
			{
				ctx.block_context.tblock + DurationLedger::from_secs(tc.offset as i128)
			} else {
				ctx.block_context.tblock
			}
		};

		/// Per-transaction preparation outcome (kept in input order).
		///
		/// `Ready` is deliberately not boxed despite dwarfing `Failed`: this is a short-lived local
		/// buffer holding one entry per transaction in a single batch, so the wasted stack/`Vec` bytes
		/// are bounded by the batch size, whereas boxing would add two heap allocations per
		/// transaction on the batch-verification hot path this whole function exists to speed up.
		#[allow(clippy::large_enum_variant)]
		enum Prep<S: SignatureKind<D>, D: DB> {
			/// Deserialization or the non-crypto `well_formed` checks failed for this tx.
			Failed(LedgerApiError),
			/// Passed the non-crypto checks; carries what the cache-warming step needs.
			Ready { key: TxValidationKey, tx: Transaction<S, D>, verified_tx: VerifiedTransaction<D> },
		}

		// Accumulate the per-tx non-crypto `well_formed` time (proofs deferred). Reported below as
		// `mode="batch_prep"` so the ON path's crypto cost can be compared against the inline path's
		// fused `well_formed`-with-proofs cost on equal terms.
		let mut prep_elapsed = std::time::Duration::ZERO;
		let mut prep_count: u64 = 0;
		let mut preps: Vec<Prep<S, D>> = Vec::with_capacity(txs_serialized.len());
		for tx_serialized in txs_serialized {
			let tx = match api.tagged_deserialize::<Transaction<S, D>>(tx_serialized) {
				Ok(tx) => tx,
				Err(e) => {
					preps.push(Prep::Failed(e));
					continue;
				},
			};
			let key = TxValidationKey { runtime_version, tx_hash: tx.hash() };

			// Defer proofs: run every non-crypto check now, batch the ZK crypto below.
			let mut strictness = mn_ledger_local::verify::WellFormedStrictness::default();
			strictness.verify_contract_proofs = false;
			strictness.verify_native_proofs = false;

			let wf_start = Instant::now();
			let wf_result = tx.0.well_formed(&ctx.ref_state, strictness, tblock);
			prep_elapsed += wf_start.elapsed();
			prep_count += 1;
			match wf_result {
				Ok(verified_tx) => preps.push(Prep::Ready { key, tx, verified_tx }),
				Err(e) => {
					log::warn!(target: LOG_TARGET, "batch: transaction malformed: {e}");
					preps.push(Prep::Failed(LedgerApiError::Transaction(
						types::TransactionError::Malformed(e.into()),
					)));
				},
			}
		}

		// Aggregate crypto step over every transaction that passed the non-crypto checks.
		let ready_txs: Vec<_> = preps
			.iter()
			.filter_map(|p| match p {
				Prep::Ready { tx, .. } => Some(&tx.0),
				Prep::Failed(_) => None,
			})
			.collect();

		// Time only the aggregate crypto (`mode="batch"`); `ready_txs.len()` normalizes it per-tx.
		//
		// `linear_revalidation` mirrors `isolate_on_failure`: isolating the offender(s) *is* asking the
		// ledger to localize the failing proofs, and the fail-fast (block-import) path never needs
		// per-transaction attribution, so it takes the cheaper unlocalized rejection.
		let crypto_start = Instant::now();
		let batch_result = super::batch_verify::batch_verify_proofs(
			&ready_txs,
			&ctx.ref_state,
			/* linear_revalidation */ isolate_on_failure,
		);
		let crypto_elapsed = crypto_start.elapsed();
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			// Skip a batch with no ready txs (all malformed): it does no crypto, so recording a
			// zero sample would only dilute the per-tx average.
			if !ready_txs.is_empty() {
				metrics.observe_batch_proof_verify(
					crypto_elapsed.as_secs_f64(),
					ready_txs.len() as u64,
				);
			}
			if prep_count > 0 {
				metrics.observe_batch_prep_verify(prep_elapsed.as_secs_f64(), prep_count);
			}
		}

		// Transactions the ledger blamed for the aggregate failure, as indices into `ready_txs` (i.e.
		// counting only the transactions that passed the non-crypto checks). Empty when the whole
		// batch verified. Short (usually one) and ascending, so a linear `contains` below is fine.
		let bad_ready: Vec<usize> = match batch_result {
			Ok(()) => Vec::new(),
			// The ledger localized the offender(s): every other ready transaction verified as part of
			// the same aggregate check, so no re-verification is needed to accept them.
			Err(BatchVerifyFailure::Localized(indices)) => indices,
			// Nothing can be concluded per-transaction — reject the whole batch. On the block-import
			// path this is the fail-fast rejection; on the mempool path the caller falls back to
			// per-transaction runtime validation.
			Err(BatchVerifyFailure::Unlocalized) => {
				log::warn!(
					target: LOG_TARGET,
					"batch proof verification failed without localization; rejecting batch"
				);
				return Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				)));
			},
		};

		// Warm the caches for every verified transaction so downstream consumers skip the crypto, and
		// cache `false` for the localized offender(s) so `get_verified_transaction` rejects them.
		let mut results = Vec::with_capacity(preps.len());
		let mut ready_idx = 0usize;
		for prep in preps {
			match prep {
				Prep::Failed(e) => results.push(Err(e)),
				Prep::Ready { key, tx, verified_tx } => {
					let is_bad = bad_ready.contains(&ready_idx);
					ready_idx += 1;
					if is_bad {
						insert_proof_result(&key, false);
						log::warn!(
							target: LOG_TARGET,
							"batch: isolated invalid proof for {}",
							hex::encode(key.tx_hash),
						);
						results.push(Err(LedgerApiError::Transaction(
							types::TransactionError::Invalid(types::InvalidError::UnknownError),
						)));
					} else {
						results.push(Self::warm_verified_tx(
							&ledger,
							&ctx,
							tblock,
							key,
							&tx,
							verified_tx,
							is_mempool,
						));
					}
				},
			}
		}

		log::debug!(
			target: LOG_TARGET,
			"✅ batch-verified {} of {} transaction(s), {} with invalid proofs (elapsed_ms={})",
			ready_idx - bad_ready.len(),
			results.len(),
			bad_ready.len(),
			start_batch_time.elapsed().as_millis(),
		);

		Ok(results)
	}

	/// Warms the process-global caches for a transaction whose proofs the aggregate batch check
	/// verified.
	///
	/// Records `PROOF_VERIFICATION_CACHE = true`, inserts the `VerifiedTransaction` into the
	/// validation cache (against the batch's reference state and effective tblock, so a subsequent
	/// lookup at the same state strict-hits and any later state revalidates the cheap delta), and
	/// dry-runs the guaranteed segment against the batch's reference state. Returns the
	/// per-transaction validation result: `Ok(())` when the guaranteed dry-run passes, otherwise
	/// the `Invalid` error it would fail with.
	///
	/// `is_mempool` selects whether the success arm emits the per-transaction
	/// `📋 Validated transaction … for mempool` line that `do_validate_transaction` emits on the
	/// non-batched path — see the comment at that log site for why block import is excluded.
	#[allow(clippy::too_many_arguments)]
	fn warm_verified_tx(
		ledger: &Sp<Ledger<D>, D>,
		ctx: &TransactionContext<D>,
		tblock: Timestamp,
		key: TxValidationKey,
		tx: &Transaction<S, D>,
		verified_tx: VerifiedTransaction<D>,
		is_mempool: bool,
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		insert_proof_result(&key, true);

		TX_VALIDATION_CACHE.insert(
			key.clone(),
			Arc::new(TxValidationValue {
				verified_tx: verified_tx.clone(),
				state: Sp::new(ledger.state.clone()),
				tblock,
			}),
		);

		// Dry-run the guaranteed segment against the batch's reference state.
		match super::guaranteed_validation::validate_guaranteed_execution(
			&ledger.state,
			verified_tx,
			ctx,
		) {
			Ok(()) => {
				// Mirror the non-batched path's per-tx line. The validation-cache entry inserted just
				// above makes the subsequent `do_validate_transaction` return early from its
				// strict-cache hit *without* logging, so without this a batch-ON node would emit no
				// `📋 Validated transaction` line at all and log-derived tx counts would not be
				// comparable against a batch-OFF node.
				//
				// Block import is excluded: there the non-batched path validates via `pre_dispatch`
				// (`do_validate_guaranteed_execution`), which emits no such line either — so logging
				// per-tx here would *add* lines the OFF side lacks, and put INFO logging in the hot
				// path the A/B is measuring.
				if is_mempool {
					log::info!(
						target: LOG_TARGET,
						"📋 Validated transaction {} for mempool",
						hex::encode(tx.hash())
					);
				}
				Ok(())
			},
			Err(reason) => {
				log::warn!(
					target: LOG_TARGET,
					"batch: guaranteed execution would fail for {}: {reason:?}",
					hex::encode(key.tx_hash),
				);
				Err(LedgerApiError::Transaction(types::TransactionError::Invalid(reason.into())))
			},
		}
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

	fn get_tx_type(tx: &Transaction<S, D>) -> &'static str {
		match tx.0 {
			mn_ledger_local::structure::Transaction::Standard(_) => "standard",
			mn_ledger_local::structure::Transaction::ClaimRewards(_) => "claim_rewards",
		}
	}

	fn get_system_tx_type(tx: &SystemTransaction) -> Result<&'static str, LedgerApiError> {
		get_system_tx_type(tx)
	}

	/// The pool's `and_provides` tag: `Twox128(runtime_version_le ++ tx_bytes)` zero-extended to
	/// 32 bytes. State-independent, and computable without deserializing the transaction. Must
	/// stay byte-for-byte identical to `tx_validation_cache_key` in the node's `batch_chain_api`,
	/// which recomputes it natively for batch-verified transactions.
	fn tx_validation_cache_key(runtime_version: u32, tx_serialized: &[u8]) -> Hash {
		let to_hash = [&runtime_version.to_le_bytes(), tx_serialized].concat();
		let hash16 = Twox128::hash(&to_hash);
		let mut out = [0u8; 32];
		out[..16].copy_from_slice(&hash16);
		out
	}

	/// Gets a VerifiedTransaction, using the cache when possible.
	///
	/// - Checks the cache (keyed by runtime_ver + tx_hash)
	/// - On hit with matching state *and* tblock: returns cached VerifiedTransaction
	/// - On hit with a stale state or tblock: revalidates against the new state, updates cache
	/// - On miss: calls well_formed(), caches result, returns it
	///
	/// Also returns the wall-clock time spent running the ZK crypto **inline**, if any. The
	/// duration is `Some` only on the OFF/cold-cache path — where `well_formed` verified the
	/// proofs itself — and `None` on a cache hit, a revalidation hit, or a proof-cache hit (crypto
	/// deferred). Callers record `Some` as the `mode="inline"` proof-verify metric, the
	/// per-transaction baseline the batched cost is compared against.
	fn get_verified_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		key: &TxValidationKey,
		tblock_correction: Option<&TBlockCorrection>,
	) -> Result<
		(VerifiedTransaction<D>, TxValidationCacheOutcome, Option<std::time::Duration>),
		LedgerApiError,
	>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let tblock = well_formed_tblock(ledger, block_context, tblock_correction);

		if let Some(cached) = TX_VALIDATION_CACHE.get(key) {
			if let Some(cached) = cached.downcast_ref::<TxValidationValue<D>>() {
				let fresh =
					cached.state.hash() == ledger.state.state_hash() && cached.tblock == tblock;
				return if fresh {
					Ok((
						cached.verified_tx.clone(),
						TxValidationCacheOutcome::StrictCacheHit,
						None,
					))
				} else {
					// Revalidation re-runs only the time/state-dependent delta checks — never the
					// ZK crypto — so it contributes no inline proof-verify sample.
					Self::revalidate_transaction(
						ledger,
						tx,
						block_context,
						&cached.state,
						key.clone(),
						tblock,
					)
					.map(|(vt, outcome)| (vt, outcome, None))
				};
			}
			// Downcast failed - fall through to recompute
			log::warn!(target: LOG_TARGET, "VerifiedTransaction cache downcast failed");
		}

		// Cache miss: compute VerifiedTransaction
		Self::verify_transaction(ledger, tx, block_context, key.clone(), tblock)
	}

	fn verify_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_validation_key: TxValidationKey,
		tblock: Timestamp,
	) -> Result<
		(VerifiedTransaction<D>, TxValidationCacheOutcome, Option<std::time::Duration>),
		LedgerApiError,
	> {
		let ctx = ledger.get_transaction_context(block_context.clone())?;

		// Consult the proof-verification cache written by the batch-verification ingress points
		// (mempool worker pool / block-import wrapper) to decide whether the ZK crypto can be
		// deferred:
		// - Some(true):  proofs already batch-verified — defer them (skip the expensive crypto).
		// - Some(false): a known-bad proof — reject.
		// - None:        cache miss — a performance signal, not a correctness failure. Log an
		//                error and fall back to a full inline verification.
		//
		// Deferring proofs still runs every stateless-non-proof, param, op and maintenance check
		// in `well_formed`; only the ZK crypto is skipped.
		let mut strictness = mn_ledger_local::verify::WellFormedStrictness::default();
		// `true` only on the `None` branch below, where `well_formed` runs the ZK crypto itself.
		let mut verifies_proofs_inline = false;
		match get_proof_result(&tx_validation_key) {
			Some(true) => {
				// Equivalent to `WellFormedStrictness::defer_proofs()`, spelled out via the public
				// fields so this shared code compiles against every ledger version (only the
				// ledger-9 branch exposes `defer_proofs()`).
				strictness.verify_contract_proofs = false;
				strictness.verify_native_proofs = false;
			},
			Some(false) => {
				log::warn!(
					target: LOG_TARGET,
					"🚫 proof-verification cache recorded an invalid proof for {}: rejecting",
					hex::encode(tx_validation_key.tx_hash),
				);
				return Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				)));
			},
			None => {
				verifies_proofs_inline = true;
				log::error!(
					target: LOG_TARGET,
					"proof-verification cache miss for {}: verifying inline (slow). Proofs should \
					 have been batch-verified at ingress (mempool/import).",
					hex::encode(tx_validation_key.tx_hash),
				);
			},
		}

		let wf_start = Instant::now();
		let verified_tx = tx.0.well_formed(&ctx.ref_state, strictness, tblock).map_err(|e| {
			log::warn!(
				target: LOG_TARGET,
				"Transaction malformed: {e}",
			);
			LedgerApiError::Transaction(types::TransactionError::Malformed(e.into()))
		})?;
		// Only the `None` branch actually ran the ZK crypto; the deferred-proof branch just did the
		// cheap non-crypto checks, so it is not an inline proof-verification sample.
		let inline_proof_verify = verifies_proofs_inline.then(|| wf_start.elapsed());

		TX_VALIDATION_CACHE.insert(
			tx_validation_key,
			Arc::new(TxValidationValue {
				verified_tx: verified_tx.clone(),
				state: Sp::new(ledger.state.clone()),
				tblock,
			}),
		);
		Ok((verified_tx, TxValidationCacheOutcome::CacheMiss, inline_proof_verify))
	}

	fn revalidate_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		prev_state: &LedgerState<D>,
		tx_validation_key: TxValidationKey,
		tblock: Timestamp,
	) -> Result<(VerifiedTransaction<D>, TxValidationCacheOutcome), LedgerApiError> {
		let ctx = ledger.get_transaction_context(block_context.clone())?;
		let revalidation_ref = mn_ledger_local::verify::RevalidationReference {
			previously_validated_state: prev_state.clone(),
			new_state: ctx.ref_state,
		};
		let verified_tx = Self::is_well_formed(tx, &revalidation_ref, tblock)?;
		TX_VALIDATION_CACHE.insert(
			tx_validation_key,
			Arc::new(TxValidationValue {
				verified_tx: verified_tx.clone(),
				state: Sp::new(ledger.state.clone()),
				tblock,
			}),
		);
		Ok((verified_tx, TxValidationCacheOutcome::RevalidationHit))
	}

	fn is_well_formed(
		tx: &Transaction<S, D>,
		ref_state: &impl StateReference<D>,
		block_timestamp: Timestamp,
	) -> Result<VerifiedTransaction<D>, LedgerApiError> {
		tx.0.well_formed(
			ref_state,
			mn_ledger_local::verify::WellFormedStrictness::default(),
			block_timestamp,
		)
		.map_err(|e| {
			log::warn!(
				target: LOG_TARGET,
				"Transaction malformed: {e}",
			);
			LedgerApiError::Transaction(types::TransactionError::Malformed(e.into()))
		})
	}

	/// Validates a transaction for the mempool.
	///
	/// Uses the cache for revalidation of transactions already in the pool.
	/// Returns the cache outcome indicating how validation was resolved.
	fn do_validate_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		key: &TxValidationKey,
	) -> Result<TxValidationCacheOutcome, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let tx_hash_hex = hex::encode(tx.hash());
		// No `tblock` correction on the mempool path: `validate_unsigned` already skews the
		// block context it passes here by `slot_duration * (1 + MaxSkippedSlots)`.
		// The inline proof-verify duration (`.2`) is recorded on the block-import path
		// (`validate_guaranteed_execution` / `apply_transaction`); the mempool path is not
		// instrumented here.
		let (verified_tx, cache_outcome, _inline_proof_verify) =
			match Self::get_verified_transaction(ledger, tx, block_context, key, None) {
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

		// A strict hit means the previous dry-run ran against this exact state and tblock, so its
		// result still holds. A revalidation hit does not: `well_formed` never checks
		// applicability — no double-spend, balance or dust-fee check, that is
		// `apply_guaranteed_only`'s job — so a transaction whose inputs the last block spent
		// would otherwise survive in the pool until the producing node's `pre_dispatch` rejected
		// it.
		if matches!(cache_outcome, TxValidationCacheOutcome::StrictCacheHit) {
			return Ok(cache_outcome);
		}

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
				Ok(cache_outcome)
			},
			Err(reason) => {
				log::warn!(
					target: LOG_TARGET,
					"🚫 Rejected transaction {} from mempool: guaranteed execution would fail: {reason:?}",
					tx_hash_hex
				);
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
	/// Returns the cache outcome indicating how validation was resolved, together with the
	/// wall-clock time spent running the ZK crypto inline (`Some` only on a cold-cache miss where
	/// `get_verified_transaction` verified proofs itself — see its docs). The caller
	/// (`validate_guaranteed_execution`) records the duration as the `mode="inline"` proof-verify
	/// metric: this pre-dispatch path is where the OFF block-import path actually runs the crypto.
	fn do_validate_guaranteed_execution(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		key: &TxValidationKey,
		tblock_correction: Option<&TBlockCorrection>,
	) -> Result<(TxValidationCacheOutcome, Option<std::time::Duration>), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let (verified_tx, cache_outcome, inline_proof_verify) =
			Self::get_verified_transaction(ledger, tx, block_context, key, tblock_correction)?;

		let ctx = ledger.get_transaction_context(block_context.clone())?;

		match super::guaranteed_validation::validate_guaranteed_execution(
			&ledger.state,
			verified_tx,
			&ctx,
		) {
			Ok(()) => Ok((cache_outcome, inline_proof_verify)),
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

/// The `tblock` to run `well_formed` against.
///
/// Blocks produced before `disable_after` can contain a *first* transaction whose `ctime` runs
/// ahead of the block timestamp: the producing node served that transaction's `well_formed`
/// result from the strict cache, where it had been verified during mempool ingress at
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
/// rule, a total function of `(block_context, is_block_start, config)` evaluated identically on
/// every node, with no try-then-retry branch for consensus to depend on.
///
/// See <https://github.com/midnightntwrk/midnight-node/issues/1924>
#[cfg(feature = "std")]
fn well_formed_tblock<D: DB>(
	ledger: &Ledger<D>,
	block_context: &BlockContext,
	tblock_correction: Option<&TBlockCorrection>,
) -> Timestamp {
	if let Some(tc) = tblock_correction
		&& block_context.tblock < tc.disable_after
		&& ledger.is_block_start()
		&& let Some(parent_block_time) = block_context.parent_block_time()
	{
		Timestamp::from_secs(parent_block_time) + DurationLedger::from_secs(tc.offset as i128)
	} else {
		Timestamp::from_secs(block_context.tblock)
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

	/// Matches `res/cfg/default.toml`: `slot_duration_secs * (1 + MaxSkippedSlots)` = 6 * 2.
	const OFFSET: i64 = 12;
	/// Preview #128537, the block whose first transaction motivated the correction.
	const BLOCK_TBLOCK: u64 = 1784987076;
	const DISABLE_AFTER: u64 = 1785801600;

	fn block_context() -> BlockContext {
		BlockContext { tblock: BLOCK_TBLOCK, ..Default::default() }
	}

	fn correction(disable_after: u64) -> TBlockCorrection {
		TBlockCorrection { offset: OFFSET, disable_after }
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
		let tblock =
			well_formed_tblock(&ledger_at_block_start(), &bc, Some(&correction(DISABLE_AFTER)));

		// The tx is verified at the parent's timestamp plus the mempool skew, reproducing
		// what the producing node's warm strict-cache entry was verified at.
		let parent = bc.parent_block_time().expect("post-ledger-8 contexts always carry one");
		assert_eq!(
			tblock,
			Timestamp::from_secs(parent) + DurationLedger::from_secs(OFFSET as i128)
		);
		assert_ne!(
			tblock,
			Timestamp::from_secs(bc.tblock),
			"the corrected timestamp must not be the block's own tblock"
		);
	}

	#[test]
	fn well_formed_tblock_is_uncorrected_without_a_configured_correction() {
		let bc = block_context();
		assert_eq!(
			well_formed_tblock(&ledger_at_block_start(), &bc, None),
			Timestamp::from_secs(BLOCK_TBLOCK),
		);
	}

	#[test]
	fn well_formed_tblock_is_uncorrected_at_or_after_disable_after() {
		let bc = block_context();
		// `disable_after` is exclusive of the correction: a block at the cutoff is not corrected.
		assert_eq!(
			well_formed_tblock(&ledger_at_block_start(), &bc, Some(&correction(BLOCK_TBLOCK))),
			Timestamp::from_secs(BLOCK_TBLOCK),
		);
		assert_eq!(
			well_formed_tblock(&ledger_at_block_start(), &bc, Some(&correction(BLOCK_TBLOCK - 1))),
			Timestamp::from_secs(BLOCK_TBLOCK),
		);
	}

	#[test]
	fn well_formed_tblock_is_uncorrected_after_the_first_tx_in_a_block() {
		let bc = block_context();
		assert_eq!(
			well_formed_tblock(&ledger_mid_block(), &bc, Some(&correction(DISABLE_AFTER))),
			Timestamp::from_secs(BLOCK_TBLOCK),
		);
	}

	#[test]
	fn proof_verification_cache_roundtrip() {
		// Distinct keys so the shared process-global cache doesn't collide with other tests.
		let present = TxValidationKey { runtime_version: 1, tx_hash: [0xA1u8; 32] };
		let known_bad = TxValidationKey { runtime_version: 1, tx_hash: [0xB2u8; 32] };
		let absent = TxValidationKey { runtime_version: 1, tx_hash: [0xC3u8; 32] };

		assert_eq!(get_proof_result(&absent), None, "missing key must be a cache miss");

		insert_proof_result(&present, true);
		insert_proof_result(&known_bad, false);

		// moka's sync cache guarantees read-your-writes per key.
		assert_eq!(get_proof_result(&present), Some(true), "verified proof must read back true");
		assert_eq!(
			get_proof_result(&known_bad),
			Some(false),
			"known-bad proof must read back false"
		);
		assert_eq!(get_proof_result(&absent), None, "unrelated key stays a miss");
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

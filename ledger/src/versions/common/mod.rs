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
			MaintenanceUpdate, OutputInstructionUnshielded, ProofMarker, SignatureKind,
			SingleUpdate, Transaction as LedgerTransaction, VerifiedTransaction,
		},
	},
	std::{
		any::Any,
		sync::Arc,
		time::{Duration, Instant},
	},
};

#[cfg(feature = "std")]
use crate::common::batch::BatchVerifyFailure;
use crate::common::types::{
	ContractCallsDetails, FallibleCoinsDetails, GasCost, GuaranteedCoinsDetails, Hash, Op,
	SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot, TransactionDetails, Tx,
	WrappedHash,
};

use super::BlockContext;

#[cfg(feature = "std")]
use {lazy_static::lazy_static, moka::sync::Cache};

pub const LOG_TARGET: &str = "midnight::ledger_v2";
pub const MINT_COINS_DOMAIN_SEPARATOR: &[u8; 10] = b"mint_coins";

#[derive(PartialEq, Eq, Hash)]
pub struct StrictTxValidationKey {
	state_hash: Hash,
	tx_hash: Hash,
	block_context_tblock: u64,
}
#[derive(PartialEq, Eq, Hash)]
pub struct SoftTxValidationKey {
	tx_hash: Hash,
}
/// Key for the proof-verification cache.
///
/// Uses only the state-independent `tx_validation_cache_key` (Twox128 of
/// `runtime_version ++ tx_bytes`), so a batch-verified proof result survives the per-extrinsic
/// `state_hash` drift that makes the STRICT cache miss for every tx after the first in a block.
#[derive(PartialEq, Eq, Hash)]
pub struct ProofVerificationKey {
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

/// Capacity of the proof-verification cache.
/// Set at least as high as the soft cache (2000) so batch-verified proof results are never
/// evicted under mempool load before the downstream `get_verified_transaction` reads them.
#[cfg(feature = "std")]
const PROOF_VERIFICATION_CACHE_CAPACITY: u64 = 2000;

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

	/// Proof-verification cache: maps a state-independent tx hash to its ZK-proof outcome.
	///
	/// Written exclusively by the batch-verification ingress points (the mempool worker pool and
	/// the block-import wrapper, via `Bridge::batch_verify_transactions`); read by
	/// `get_verified_transaction` so downstream consumers can skip the (now-deferred) ZK crypto.
	/// A cached `false` lets a downstream consumer reject a known-bad transaction. This cache is
	/// process-global (like the SOFT/STRICT caches) and therefore not shared across processes.
	static ref PROOF_VERIFICATION_CACHE: Cache<ProofVerificationKey, bool> =
		Cache::builder()
			.max_capacity(PROOF_VERIFICATION_CACHE_CAPACITY)
			.time_to_idle(TX_VALIDATION_CACHE_TTI)
			.build();
}

/// Records the batch ZK-proof outcome for a transaction, keyed by its state-independent
/// `tx_validation_cache_key`. Called only by the batch-verification ingress points.
#[cfg(feature = "std")]
pub fn insert_proof_result(tx_hash: &WrappedHash, verified: bool) {
	PROOF_VERIFICATION_CACHE.insert(ProofVerificationKey { tx_hash: tx_hash.0 }, verified);
}

/// Returns the cached ZK-proof outcome for a transaction, if an ingress point has verified it.
/// A `None` result is a performance signal (the caller should verify inline), not a correctness
/// failure.
#[cfg(feature = "std")]
pub fn get_proof_result(tx_hash: &WrappedHash) -> Option<bool> {
	PROOF_VERIFICATION_CACHE.get(&ProofVerificationKey { tx_hash: tx_hash.0 })
}

/// Current entry count of the proof-verification cache (for metrics/observability).
#[cfg(feature = "std")]
pub fn proof_verification_cache_size() -> u64 {
	PROOF_VERIFICATION_CACHE.entry_count()
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

	pub fn pre_fetch_storage(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
	) -> Result<(), LedgerApiError> {
		let api = api::new();
		let typed_key: TypedArenaKey<Ledger<D>, D::Hasher> = api.tagged_deserialize(state_key)?;
		let key: ArenaKey<D::Hasher> = typed_key.into();

		let now = std::time::Instant::now();
		default_storage::<D>().with_backend(|backend| backend.pre_fetch(key.hash(), None, true));
		let elapsed = now.elapsed().as_secs_f64();

		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			metrics.observe_storage_fetch_time(elapsed, "ledger_state");
		}
		Ok(())
	}

	pub fn flush_storage(mut externalities: &mut dyn Externalities) {
		let now = std::time::Instant::now();
		default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
		let elapsed = now.elapsed().as_secs_f64();

		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			metrics.observe_storage_flush_time(elapsed, "ledger_state");
		}
	}

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
		Ok(state_root)
	}

	pub fn get_version() -> Vec<u8> {
		crate::utils::find_crate_version(super::CRATE_NAME).unwrap_or(b"unknown".into())
	}

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
		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);
		let tblock_ext = externalities.extension::<TBlockCorrectionExt>();
		let tblock_correction = tblock_ext.map(|e| &e.0);
		let (verified_tx, inline_proof_verify) = Self::get_verified_transaction(
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
		let (mut new_ledger, applied_stage) =
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
			"⏱️  Persisting ledger (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		new_ledger.persist();
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ledger persisted (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		// Write Prometheus metrics
		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			let tx_type = Self::get_tx_type(&tx);
			let elapsed_time = start_tx_processing_time.elapsed().as_secs_f64();

			metrics.observe_txs_processing_time(elapsed_time, tx_type);
			metrics.observe_txs_size(tx_size as f64, tx_type);
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

		let mut ledger =
			Ledger::apply_system_tx(ledger, &tx, Timestamp::from_secs(block_context.tblock))?;

		let event = SystemTransactionAppliedStateRoot {
			state_root: api.tagged_serialize(&ledger.as_typed_key())?,
			tx_hash,
			tx_type: tx_type.to_string(),
		};

		// Only update state after no errors
		ledger.persist();

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

		let tblock_ext = externalities.extension::<TBlockCorrectionExt>();
		let tblock_correction = tblock_ext.map(|e| &e.0);
		let was_cached = Self::do_validate_transaction(
			&ledger,
			&tx,
			&block_context,
			&wrapped_cache_key,
			tblock_correction,
		)?;

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
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		let tblock_ext = externalities.extension::<TBlockCorrectionExt>();
		let tblock_correction = tblock_ext.map(|e| &e.0);
		// Perform dry-run validation with caching
		let (was_cached, inline_proof_verify) = Self::do_validate_guaranteed_execution(
			&ledger,
			&tx,
			&block_context,
			&cache_key,
			tblock_correction,
		)?;

		// Write Prometheus metrics
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			if was_cached {
				metrics.inc_tx_validation_cache_hit("strict");
			} else {
				metrics.inc_tx_validation_cache_miss();
			}

			// Records the OFF-path per-tx baseline (`mode="inline"`) only when this call actually ran
			// the ZK crypto inline (cold proof cache). `send_mn_transaction` is unsigned, so during
			// `execute_block` FRAME runs this `pre_dispatch` BEFORE the call's `apply_transaction`;
			// the crypto therefore happens here and warms the STRICT cache, leaving
			// `apply_transaction`'s `get_verified_transaction` a cache hit (`None`). Recording here is
			// what makes the inline baseline observable on the OFF block-import path.
			if let Some(pv) = inline_proof_verify {
				metrics.observe_inline_proof_verify(pv.as_secs_f64());
			}

			// Report current cache sizes
			metrics
				.set_tx_validation_cache_size("strict", STRICT_TX_VALIDATION_CACHE.entry_count());
			metrics.set_tx_validation_cache_size("soft", SOFT_TX_VALIDATION_CACHE.entry_count());
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
	/// `PROOF_VERIFICATION_CACHE = true`, dry-runs the guaranteed segment, and populates the STRICT
	/// and SOFT caches — exactly the state a subsequent `validate_transaction` / `pre_dispatch` /
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
		let state_hash: Hash = ledger.state.state_hash().0.into();

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
			Ready { key: WrappedHash, tx: Transaction<S, D>, verified_tx: VerifiedTransaction<D> },
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
			let key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

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
							hex::encode(key.0),
						);
						results.push(Err(LedgerApiError::Transaction(
							types::TransactionError::Invalid(types::InvalidError::UnknownError),
						)));
					} else {
						results.push(Self::warm_verified_tx(
							&ledger,
							&ctx,
							state_hash,
							block_context.tblock,
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
	/// Records `PROOF_VERIFICATION_CACHE = true`, inserts the `VerifiedTransaction` into the STRICT
	/// cache, dry-runs the guaranteed segment against the batch's reference state, and on success
	/// inserts the SOFT-cache entry. Returns the per-transaction validation result: `Ok(())` when
	/// the guaranteed dry-run passes, otherwise the `Invalid` error it would fail with.
	///
	/// `is_mempool` selects whether the success arm emits the per-transaction
	/// `📋 Validated transaction … for mempool` line that `do_validate_transaction` emits on the
	/// non-batched path — see the comment at that log site for why block import is excluded.
	///
	/// The argument list is wide because everything but `key`/`tx`/`verified_tx` is batch-wide state
	/// the caller hoists out of its per-transaction loop (`state_hash` in particular is deliberately
	/// computed once per batch, not once per transaction).
	#[allow(clippy::too_many_arguments)]
	fn warm_verified_tx(
		ledger: &Sp<Ledger<D>, D>,
		ctx: &TransactionContext<D>,
		state_hash: Hash,
		block_context_tblock: u64,
		key: WrappedHash,
		tx: &Transaction<S, D>,
		verified_tx: VerifiedTransaction<D>,
		is_mempool: bool,
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		insert_proof_result(&key, true);

		let strict_key = StrictTxValidationKey { state_hash, tx_hash: key.0, block_context_tblock };
		STRICT_TX_VALIDATION_CACHE.insert(strict_key, Arc::new(verified_tx.clone()));

		// Dry-run the guaranteed segment against the batch's reference state.
		match super::guaranteed_validation::validate_guaranteed_execution(
			&ledger.state,
			verified_tx,
			ctx,
		) {
			Ok(()) => {
				// Mirror the non-batched path's per-tx line. The SOFT-cache entry inserted just below
				// makes the subsequent `do_validate_transaction` return early from its cache hit
				// *without* logging, so without this a batch-ON node would emit no
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
				SOFT_TX_VALIDATION_CACHE.insert(SoftTxValidationKey { tx_hash: key.0 }, Ok(()));
				Ok(())
			},
			Err(reason) => {
				log::warn!(
					target: LOG_TARGET,
					"batch: guaranteed execution would fail for {}: {reason:?}",
					hex::encode(key.0),
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

	fn get_ledger(api: &api::Api, state_key: &[u8]) -> Result<Sp<Ledger<D>, D>, LedgerApiError> {
		let key: TypedArenaKey<Ledger<D>, D::Hasher> = api.tagged_deserialize(state_key)?;
		default_storage().arena.get_lazy(&key).map_err(|e| {
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
										// can't be named here (they don't exist in L7/L8's SingleUpdate);
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
	///
	/// Returns the transaction's `VerifiedTransaction` together with the wall-clock time spent
	/// running the ZK crypto **inline**, if any. The duration is `Some` only on the OFF/cold-cache
	/// path — where `well_formed` verified the proofs itself — and `None` on a strict-cache hit or a
	/// proof-cache hit (crypto deferred). Callers record `Some` as the `mode="inline"` proof-verify
	/// metric, the per-transaction baseline the batched cost is compared against.
	fn get_verified_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		tblock_correction: Option<&TBlockCorrection>,
	) -> Result<(VerifiedTransaction<D>, Option<std::time::Duration>), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let state_hash = ledger.state.state_hash();
		let strict_key = StrictTxValidationKey {
			state_hash: state_hash.0.into(),
			tx_hash: tx_hash.0,
			block_context_tblock: block_context.tblock,
		};

		// Check strict cache
		if let Some(cached) = STRICT_TX_VALIDATION_CACHE.get(&strict_key) {
			if let Some(vt) = cached.downcast_ref::<VerifiedTransaction<D>>() {
				return Ok((vt.clone(), None));
			}
			// Downcast failed - fall through to recompute
			log::warn!(target: LOG_TARGET, "VerifiedTransaction cache downcast failed");
		}

		// Cache miss: compute VerifiedTransaction.
		//
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
		let ctx = ledger.get_transaction_context(block_context.clone())?;

		let mut strictness = mn_ledger_local::verify::WellFormedStrictness::default();
		// `true` only on the `None` branch below, where `well_formed` runs the ZK crypto itself.
		let mut verifies_proofs_inline = false;
		match get_proof_result(tx_hash) {
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
					hex::encode(tx_hash.0),
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
					hex::encode(tx_hash.0),
				);
			},
		}

		let tblock = if let Some(tc) = tblock_correction
			&& block_context.tblock < tc.disable_after
		{
			ctx.block_context.tblock + DurationLedger::from_secs(tc.offset as i128)
		} else {
			ctx.block_context.tblock
		};

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

		// Cache in strict cache (soft cache is managed by do_validate_transaction)
		STRICT_TX_VALIDATION_CACHE.insert(strict_key, Arc::new(verified_tx.clone()));

		Ok((verified_tx, inline_proof_verify))
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
		tblock_correction: Option<&TBlockCorrection>,
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
		// The inline proof-verify duration (`.1`) is recorded on the block-import path
		// (`apply_transaction`); the mempool path is not instrumented here.
		let verified_tx = match Self::get_verified_transaction(
			ledger,
			tx,
			block_context,
			tx_hash,
			tblock_correction,
		) {
			Ok((vt, _)) => vt,
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
	/// Returns whether validation was served from the strict cache, together with the wall-clock time
	/// spent running the ZK crypto inline (`Some` only on a cold-cache miss where
	/// `get_verified_transaction` verified proofs itself — see its docs). The caller
	/// (`validate_guaranteed_execution`) records the duration as the `mode="inline"` proof-verify
	/// metric: this pre-dispatch path is where the OFF block-import path actually runs the crypto.
	fn do_validate_guaranteed_execution(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		tblock_correction: Option<&TBlockCorrection>,
	) -> Result<(bool, Option<std::time::Duration>), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		// Invalidate soft cache — tx must re-validate after a block authoring attempt
		SOFT_TX_VALIDATION_CACHE.invalidate(&SoftTxValidationKey { tx_hash: tx_hash.0 });

		// Check strict cache to determine if this is a cache hit
		let state_hash = ledger.state.state_hash();
		let strict_key = StrictTxValidationKey {
			state_hash: state_hash.0.into(),
			tx_hash: tx_hash.0,
			block_context_tblock: block_context.tblock,
		};
		let was_cached = STRICT_TX_VALIDATION_CACHE.get(&strict_key).is_some();

		let (verified_tx, inline_proof_verify) =
			Self::get_verified_transaction(ledger, tx, block_context, tx_hash, tblock_correction)?;

		let ctx = ledger.get_transaction_context(block_context.clone())?;

		match super::guaranteed_validation::validate_guaranteed_execution(
			&ledger.state,
			verified_tx,
			&ctx,
		) {
			Ok(()) => Ok((was_cached, inline_proof_verify)),
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
	use base_crypto_local::cost_model::FixedPoint;
	use coin_structure_local::coin::{ShieldedTokenType, UnshieldedTokenType};

	#[test]
	fn proof_verification_cache_roundtrip() {
		// Distinct keys so the shared process-global cache doesn't collide with other tests.
		let present = WrappedHash([0xA1u8; 32]);
		let known_bad = WrappedHash([0xB2u8; 32]);
		let absent = WrappedHash([0xC3u8; 32]);

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

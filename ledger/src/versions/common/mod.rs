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
/// Key for the revalidation cache: the transaction's own `transaction_hash` (SHA-256 over its
/// tagged serialization).
///
/// Deliberately **not** the Twox128 `tx_validation_cache_key` the STRICT and SOFT caches use. An
/// entry here says "this transaction's ZK proofs have already been checked", so its key has to be
/// a real cryptographic hash of the transaction: Twox128 collisions are constructible, and one
/// between a valid transaction and an attacker-chosen one would let the latter's proofs go
/// unchecked. Being state-independent, it also survives the per-extrinsic `state_hash` drift that
/// makes the STRICT cache miss for every transaction after the first in a block.
#[derive(PartialEq, Eq, Hash)]
pub struct RevalidationKey {
	tx_hash: Hash,
}

/// What is known about a transaction's ZK proofs from an earlier verification.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ProofOutcome {
	/// The proofs verified against the ledger state with this key. A later validation can reload
	/// that state and re-check the transaction through the ledger's `RevalidationReference`.
	VerifiedAt(Vec<u8>),
	/// The proofs are invalid; reject without spending the crypto again.
	Invalid,
}

/// Which per-transaction `well_formed` a validation actually ran, and how long it took.
///
/// Batch verification does not remove `well_formed` from block execution — it only makes it
/// crypto-free, by turning an [`Inline`](Self::Inline) call into a
/// [`Revalidate`](Self::Revalidate) one. Reporting the two under distinct metric modes is what
/// makes the ON path's true cost visible: the aggregate crypto saving is only a net win if
/// `revalidate` is materially cheaper than `inline`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerifySample {
	/// Served from a cache; no `well_formed` ran, so there is nothing to record.
	#[default]
	Cached,
	/// A full `well_formed` that ran the ZK crypto.
	Inline(core::time::Duration),
	/// A crypto-free `well_formed` against a `RevalidationReference`.
	Revalidate(core::time::Duration),
}

/// Set this high to ensure that even large mempool sizes don't cause performance issues due to
/// unnecessary revalidation.
#[cfg(feature = "std")]
const SOFT_TX_VALIDATION_CACHE_CAPACITY: u64 = 2000;

/// This should be set to no more than the max expected txs per block
/// 600 txs/block allows for 100 TPS (considerable higher than our real max at the time of writing)
#[cfg(feature = "std")]
const STRICT_TX_VALIDATION_CACHE_CAPACITY: u64 = 600;

/// Capacity of the revalidation cache.
/// Set at least as high as the soft cache (2000) so a verified proof result is never evicted under
/// mempool load before the downstream `get_verified_transaction` reads it.
#[cfg(feature = "std")]
const REVALIDATION_CACHE_CAPACITY: u64 = 2000;

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

	/// Revalidation cache: maps a transaction to what is known about its ZK proofs.
	///
	/// Written whenever a transaction's proofs are verified — by `get_verified_transaction` itself
	/// on the inline path, and by the batch-verification ingress points (the mempool worker pool
	/// and the block-import wrapper, via `Bridge::batch_verify_transactions`). Read by
	/// `get_verified_transaction`, which uses the recorded state to re-check the transaction
	/// through the ledger's `RevalidationReference` instead of verifying its proofs again. This
	/// cache is process-global (like the SOFT/STRICT caches) and therefore not shared across
	/// processes.
	static ref REVALIDATION_CACHE: Cache<RevalidationKey, ProofOutcome> =
		Cache::builder()
			.max_capacity(REVALIDATION_CACHE_CAPACITY)
			.time_to_idle(TX_VALIDATION_CACHE_TTI)
			.build();
}

/// Records what is known about a transaction's ZK proofs, keyed by its cryptographic
/// `transaction_hash`.
#[cfg(feature = "std")]
pub fn insert_revalidation_result(tx_hash: &Hash, outcome: ProofOutcome) {
	REVALIDATION_CACHE.insert(RevalidationKey { tx_hash: *tx_hash }, outcome);
}

/// Returns what is known about a transaction's ZK proofs from an earlier verification. A `None`
/// result is a performance signal (the caller verifies in full), not a correctness failure.
#[cfg(feature = "std")]
pub fn get_revalidation_result(tx_hash: &Hash) -> Option<ProofOutcome> {
	REVALIDATION_CACHE.get(&RevalidationKey { tx_hash: *tx_hash })
}

/// Current entry count of the revalidation cache (for metrics/observability).
#[cfg(feature = "std")]
pub fn revalidation_cache_size() -> u64 {
	REVALIDATION_CACHE.entry_count()
}

/// A transaction whose per-batch-independent work is already done: deserialized, non-crypto
/// `well_formed` checks passed, proof evidence collected and prepared.
///
/// Produced by [`Bridge::prepare_transaction`] and decided by [`Bridge::finalize_prepared_batch`].
#[cfg(feature = "std")]
pub struct PreparedTx<S: SignatureKind<D>, D: DB> {
	key: WrappedHash,
	tx: Transaction<S, D>,
	verified_tx: VerifiedTransaction<D>,
	/// Evidence items this transaction contributed, so a fold failure reported in evidence space
	/// can be mapped back to the transaction that owns it.
	evidence_len: usize,
	prepared: super::batch_verify::PreparedProofs,
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
			state_key,
			crate::common::batch::batch_verify_block_enabled(),
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
			match inline_proof_verify {
				VerifySample::Inline(pv) => metrics.observe_inline_proof_verify(pv.as_secs_f64()),
				VerifySample::Revalidate(pv) => {
					metrics.observe_revalidate_proof_verify(pv.as_secs_f64())
				},
				VerifySample::Cached => {},
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

		// No `tblock` correction on the mempool path: `validate_unsigned` already skews the
		// block context it passes here by `slot_duration * (1 + MaxSkippedSlots)`.
		let (was_cached, inline_proof_verify) = Self::do_validate_transaction(
			&ledger,
			&tx,
			&block_context,
			&wrapped_cache_key,
			state_key,
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

			// The mempool half of the per-transaction proof cost (`mode="inline_mempool"`). Kept
			// separate from `mode="inline"` (block execution) so the two can be compared: a
			// transaction that shows up in both has had its proofs verified twice on this node.
			match inline_proof_verify {
				VerifySample::Inline(pv) => {
					metrics.observe_inline_mempool_proof_verify(pv.as_secs_f64())
				},
				VerifySample::Revalidate(pv) => {
					metrics.observe_revalidate_proof_verify(pv.as_secs_f64())
				},
				VerifySample::Cached => {},
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
			state_key,
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
			match inline_proof_verify {
				VerifySample::Inline(pv) => metrics.observe_inline_proof_verify(pv.as_secs_f64()),
				VerifySample::Revalidate(pv) => {
					metrics.observe_revalidate_proof_verify(pv.as_secs_f64())
				},
				VerifySample::Cached => {},
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
	/// On aggregate-verification success, for every transaction whose proofs verified it records the
	/// proof verdict in the revalidation cache, so a subsequent `validate_transaction` /
	/// `pre_dispatch` / `apply_transaction` revalidates instead of re-running the ZK crypto. On the
	/// mempool path it additionally dry-runs the guaranteed segment and populates the STRICT and SOFT
	/// caches; block import skips all three because it consumes none of them — see
	/// [`Self::warm_verified_tx`].
	///
	/// On aggregate-verification failure the behaviour depends on `isolate_on_failure`, which is also
	/// what selects the ledger's `linear_revalidation` mode:
	/// - `true` (mempool): the ledger localizes the offending proofs; each named transaction is
	///   recorded as `ProofOutcome::Invalid` and gets an `Invalid` result, while the rest of the
	///   batch — which verified as part of the same aggregate check — is warmed as usual. Nothing is
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
			// Evidence-space indices only come back from the incremental path, which keeps its own
			// prefix-sum table; this whole-batch entry point never asks for them.
			Err(BatchVerifyFailure::LocalizedEvidence(_)) => {
				log::warn!(
					target: LOG_TARGET,
					"batch proof verification reported evidence-space indices on the whole-batch \
					 path; rejecting batch"
				);
				return Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				)));
			},
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
						insert_revalidation_result(&tx.hash(), ProofOutcome::Invalid);
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
							state_key,
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

	/// Runs everything for one transaction that does not depend on which other transactions share
	/// its batch: deserialization, the non-crypto `well_formed` checks, proof-evidence collection,
	/// and the expensive per-proof preparation.
	///
	/// This is the incremental counterpart of [`Self::batch_verify_transactions`]. A caller that
	/// has idle time before it must decide — a mempool queue filling toward its dispatch window —
	/// can run this as each transaction arrives and then pay only
	/// [`Self::finalize_prepared_batch`], whose cost is essentially independent of batch size.
	pub fn prepare_transaction(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
	) -> Result<PreparedTx<S, D>, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		Self::set_default_storage(externalities);

		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ctx = ledger.get_transaction_context(block_context.clone())?;
		let tblock = Self::batch_tblock(externalities, &ctx, &block_context);

		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// Defer proofs: every non-crypto check now, the proof work immediately after.
		let mut strictness = mn_ledger_local::verify::WellFormedStrictness::default();
		strictness.verify_contract_proofs = false;
		strictness.verify_native_proofs = false;

		let prep_start = Instant::now();
		let verified_tx = tx.0.well_formed(&ctx.ref_state, strictness, tblock).map_err(|e| {
			log::warn!(target: LOG_TARGET, "prepare: transaction malformed: {e}");
			LedgerApiError::Transaction(types::TransactionError::Malformed(e.into()))
		})?;
		let prep_elapsed = prep_start.elapsed();

		let (prepared, evidence_len) =
			super::batch_verify::prepare_tx_proofs(&tx.0, &ctx.ref_state).map_err(|_| {
				LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				))
			})?;

		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			metrics.observe_batch_prep_verify(prep_elapsed.as_secs_f64(), 1);
		}

		Ok(PreparedTx { key, tx, verified_tx, evidence_len, prepared })
	}

	/// Decides a batch of transactions prepared by [`Self::prepare_transaction`]: one fold plus a
	/// single pairing check, then the same cache warming [`Self::batch_verify_transactions`] does.
	///
	/// Returns one result per input transaction, in order.
	pub fn finalize_prepared_batch(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		block_context: BlockContext,
		prepared: Vec<PreparedTx<S, D>>,
		isolate_on_failure: bool,
	) -> Result<Vec<Result<(), LedgerApiError>>, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		if prepared.is_empty() {
			return Ok(Vec::new());
		}
		Self::set_default_storage(externalities);

		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ctx = ledger.get_transaction_context(block_context.clone())?;
		let state_hash: Hash = ledger.state.state_hash().0.into();

		// Fold every transaction's prepared evidence into one batch, recording the per-transaction
		// evidence prefix sum so failures reported in evidence space map back to transactions.
		let mut evidence_ends = Vec::with_capacity(prepared.len());
		let mut total = 0usize;
		let mut acc = super::batch_verify::PreparedProofs::default();
		for p in &prepared {
			total += p.evidence_len;
			evidence_ends.push(total);
		}
		let mut items = prepared;
		for item in items.iter_mut() {
			let taken = core::mem::take(&mut item.prepared);
			super::batch_verify::merge_prepared::<D>(&mut acc, taken);
		}

		let crypto_start = Instant::now();
		let outcome = super::batch_verify::finalize_prepared::<D>(&acc, isolate_on_failure);
		let crypto_elapsed = crypto_start.elapsed();
		if let Some(metrics) = externalities.extension::<LedgerMetricsExt>() {
			metrics.observe_batch_proof_verify(crypto_elapsed.as_secs_f64(), items.len() as u64);
		}

		let bad: Vec<usize> = match outcome {
			Ok(()) => Vec::new(),
			Err(BatchVerifyFailure::LocalizedEvidence(indices)) => {
				let tx_indices =
					super::batch_verify::evidence_to_tx_indices(&evidence_ends, &indices);
				if tx_indices.is_empty() {
					log::warn!(
						target: LOG_TARGET,
						"prepared batch failed for {} transaction(s); could not attribute evidence \
						 index(es) {indices:?}",
						items.len(),
					);
					return Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
						types::InvalidError::UnknownError,
					)));
				}
				tx_indices
			},
			Err(_) => {
				log::warn!(
					target: LOG_TARGET,
					"prepared batch verification failed without localization; rejecting batch"
				);
				return Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				)));
			},
		};

		let mut results = Vec::with_capacity(items.len());
		for (i, item) in items.into_iter().enumerate() {
			if bad.contains(&i) {
				insert_revalidation_result(&item.tx.hash(), ProofOutcome::Invalid);
				log::warn!(
					target: LOG_TARGET,
					"prepared batch: isolated invalid proof for {}",
					hex::encode(item.key.0),
				);
				results.push(Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				))));
			} else {
				results.push(Self::warm_verified_tx(
					&ledger,
					&ctx,
					state_key,
					state_hash,
					block_context.tblock,
					item.key,
					&item.tx,
					item.verified_tx,
					isolate_on_failure,
				));
			}
		}
		Ok(results)
	}

	/// The `tblock` the batch paths verify at: the block context's, with the historical-sync
	/// correction applied exactly as the per-transaction path applies it.
	fn batch_tblock(
		mut externalities: &mut dyn Externalities,
		ctx: &TransactionContext<D>,
		block_context: &BlockContext,
	) -> Timestamp {
		let tblock_correction = externalities.extension::<TBlockCorrectionExt>().map(|e| &e.0);
		if let Some(tc) = tblock_correction
			&& block_context.tblock < tc.disable_after
		{
			ctx.block_context.tblock + DurationLedger::from_secs(tc.offset as i128)
		} else {
			ctx.block_context.tblock
		}
	}

	/// Warms the process-global caches for a transaction whose proofs the aggregate batch check
	/// verified.
	///
	/// Always records the proof verdict in the revalidation cache — that entry is what lets the
	/// downstream `get_verified_transaction` revalidate instead of re-running the ZK crypto, and it
	/// is the *only* product of a batch pass that the block-import path consumes.
	///
	/// `is_mempool` gates everything else, because everything else serves the mempool only:
	/// - the STRICT-cache entry, which costs a `VerifiedTransaction` clone (the proof data, tens to
	///   hundreds of KiB per transaction). Block import cannot use it beyond the first transaction
	///   of a block anyway: the STRICT key pins `state_hash`, and `execute_block` validates each
	///   transaction against the state left by its predecessors, so from the second transaction on
	///   the key can no longer match the batch's parent-state key.
	/// - the guaranteed-segment dry-run and the `Invalid` result it produces, which `maybe_batch_verify`
	///   discards (it acts only on the batch-wide aggregate verdict) and which `execute_block` redoes
	///   for real via `pre_dispatch`.
	/// - the SOFT-cache entry, which only `do_validate_transaction` reads.
	/// - the per-transaction `📋 Validated transaction … for mempool` line that the non-batched path
	///   emits. Block import is excluded because there the non-batched path validates via
	///   `pre_dispatch` (`do_validate_guaranteed_execution`), which emits no such line either — so
	///   logging per-tx here would *add* lines the OFF side lacks, and put INFO logging in the hot
	///   path an A/B measures.
	///
	/// Returns the per-transaction validation result: `Ok(())` when the guaranteed dry-run passes (or
	/// was skipped), otherwise the `Invalid` error it would fail with.
	///
	/// The argument list is wide because everything but `key`/`tx`/`verified_tx` is batch-wide state
	/// the caller hoists out of its per-transaction loop (`state_hash` in particular is deliberately
	/// computed once per batch, not once per transaction).
	#[allow(clippy::too_many_arguments)]
	fn warm_verified_tx(
		ledger: &Sp<Ledger<D>, D>,
		ctx: &TransactionContext<D>,
		state_key: &[u8],
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
		// Record the state these proofs verified against, so the downstream
		// `get_verified_transaction` can revalidate against it instead of re-running the crypto.
		insert_revalidation_result(&tx.hash(), ProofOutcome::VerifiedAt(state_key.to_vec()));

		// Block import needs nothing further from this transaction — see the doc comment. Returning
		// here skips a large clone and a full guaranteed-execution dry-run per transaction.
		if !is_mempool {
			return Ok(());
		}

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
				log::info!(
					target: LOG_TARGET,
					"📋 Validated transaction {} for mempool",
					hex::encode(tx.hash())
				);
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
	///
	/// Returns the transaction's `VerifiedTransaction` together with the wall-clock time spent
	/// running the ZK crypto **inline**, if any. The duration is `Some` only on the OFF/cold-cache
	/// path — where `well_formed` verified the proofs itself — and `None` on a strict-cache hit or a
	/// proof-cache hit (crypto deferred). Callers record `Some` as the `mode="inline"` proof-verify
	/// metric, the per-transaction baseline the batched cost is compared against.
	///
	/// `batching_expected` says whether the ingress point that covers *this* call site is enabled,
	/// so a proof-cache miss can be reported at the right severity. The caller decides, because
	/// the two ingress points cover different call sites: the mempool worker pool warms the cache
	/// for mempool validation, while block execution is covered by either it (on the authoring
	/// node) or the block-import wrapper (on an importing one).
	fn get_verified_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		tblock_correction: Option<&TBlockCorrection>,
		current_state_key: &[u8],
		batching_expected: bool,
	) -> Result<(VerifiedTransaction<D>, VerifySample), LedgerApiError>
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
				return Ok((vt.clone(), VerifySample::Cached));
			}
			// Downcast failed - fall through to recompute
			log::warn!(target: LOG_TARGET, "VerifiedTransaction cache downcast failed");
		}

		// Cache miss: compute the VerifiedTransaction.
		let ctx = ledger.get_transaction_context(block_context.clone())?;
		let tblock = well_formed_tblock(ledger, block_context, tblock_correction);
		let strictness = mn_ledger_local::verify::WellFormedStrictness::default();

		// Has this exact transaction been verified before? The key is the transaction's own
		// SHA-256 hash, so a hit really is the same transaction — see `RevalidationKey`.
		let strong_hash = tx.hash();
		match get_revalidation_result(&strong_hash) {
			Some(ProofOutcome::Invalid) => {
				log::warn!(
					target: LOG_TARGET,
					"🚫 proofs already known invalid for {}: rejecting",
					hex::encode(strong_hash),
				);
				return Err(LedgerApiError::Transaction(types::TransactionError::Invalid(
					types::InvalidError::UnknownError,
				)));
			},
			Some(ProofOutcome::VerifiedAt(previous_state_key)) => {
				// Re-check against the ledger's own revalidation reference. It no-ops
				// `stateless_check` — signature, binding-commitment and zswap structural checks,
				// all functions of the transaction bytes alone and therefore unchanged — and
				// re-runs the state-dependent checks only where the two states actually differ
				// (ledger parameters, the contract's registered operation, its maintenance
				// authority, and the Dust roots at the transaction's ctime).
				//
				// The proof cryptography is skipped, but nothing else is: the reference applies
				// `WellFormedStrictness::assume_proofs_verified` itself, via the ledger's
				// `StateReference::adjust_strictness`. Evidence collection still runs, so
				// `op_check` and `dust_spend_check` still catch a contract operation, verifier
				// key or Dust root that moved since these proofs were verified.
				//
				// Hence the plain `strictness` below — the policy belongs to the reference, not
				// to this call site. Do not "help" by passing `defer_proofs()`: that clears the
				// flags gating evidence collection and would skip those state-dependent checks
				// along with the cryptography.
				//
				// Reloading the previous state can fail if the arena no longer holds it (pruned,
				// or a different process); that is a performance miss, not a correctness problem,
				// so fall through to a full verification.
				match Self::get_ledger(&api::new(), &previous_state_key) {
					Ok(previous) => {
						let reference = mn_ledger_local::verify::RevalidationReference {
							previously_validated_state: previous.state.clone(),
							new_state: ledger.state.clone(),
						};
						let reval_start = Instant::now();
						let verified_tx =
							tx.0.well_formed(&reference, strictness, tblock).map_err(|e| {
								log::warn!(target: LOG_TARGET, "Transaction malformed: {e}");
								LedgerApiError::Transaction(types::TransactionError::Malformed(
									e.into(),
								))
							})?;
						let reval_elapsed = reval_start.elapsed();
						STRICT_TX_VALIDATION_CACHE
							.insert(strict_key, Arc::new(verified_tx.clone()));
						// No ZK crypto ran, but the state-dependent checks did: recorded under its
						// own mode so it is never confused with the inline baseline.
						return Ok((verified_tx, VerifySample::Revalidate(reval_elapsed)));
					},
					Err(e) => {
						log::debug!(
							target: LOG_TARGET,
							"revalidation state for {} no longer loadable ({e:?}); verifying in full",
							hex::encode(strong_hash),
						);
					},
				}
			},
			None => {
				// Only a problem when the ingress point covering this call site was supposed to
				// have batch-verified the transaction already. Otherwise a full verification *is*
				// the expected path, and an error per transaction would be pure noise.
				if batching_expected {
					log::error!(
						target: LOG_TARGET,
						"no verified-proof record for {}: verifying in full (slow). Proofs should \
						 have been batch-verified at ingress (mempool/import).",
						hex::encode(strong_hash),
					);
				} else {
					log::trace!(
						target: LOG_TARGET,
						"verifying proofs in full for {} (first time seen)",
						hex::encode(strong_hash),
					);
				}
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
		// This call ran the ZK crypto itself, so it is the inline proof-verification sample.
		let inline_proof_verify = VerifySample::Inline(wf_start.elapsed());
		// Record the state it verified against, so the next validation can revalidate instead.
		insert_revalidation_result(
			&strong_hash,
			ProofOutcome::VerifiedAt(current_state_key.to_vec()),
		);

		// Cache in strict cache (soft cache is managed by do_validate_transaction)
		STRICT_TX_VALIDATION_CACHE.insert(strict_key, Arc::new(verified_tx.clone()));

		Ok((verified_tx, inline_proof_verify))
	}

	/// Validates a transaction for the mempool using the soft cache.
	///
	/// Uses `tx_hash` only for quick revalidation of transactions already in the pool.
	/// The soft cache prevents redundant ZK proof verification for mempool housekeeping.
	///
	/// Returns whether the validation was served from the soft cache, together with the wall-clock
	/// time spent running the ZK crypto inline (`Some` only when `get_verified_transaction`
	/// verified the proofs itself). The caller records the duration as the
	/// `mode="inline_mempool"` proof-verify metric — the mempool half of a transaction's total
	/// proof-verification cost.
	fn do_validate_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		current_state_key: &[u8],
	) -> Result<(bool, VerifySample), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let soft_key = SoftTxValidationKey { tx_hash: tx_hash.0 };

		// Check soft cache first (quick tx_hash-only lookup for mempool revalidation)
		if let Some(cached) = SOFT_TX_VALIDATION_CACHE.get(&soft_key) {
			return cached.map(|_| (true, VerifySample::Cached));
		}

		// Cache miss: transaction is entering the mempool or being re-validated
		let tx_hash_hex = hex::encode(tx.hash());
		let (verified_tx, inline_proof_verify) = match Self::get_verified_transaction(
			ledger,
			tx,
			block_context,
			tx_hash,
			None,
			current_state_key,
			crate::common::batch::batch_verify_mempool_enabled(),
		) {
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
				Ok((false, inline_proof_verify))
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
		current_state_key: &[u8],
	) -> Result<(bool, VerifySample), LedgerApiError>
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

		let (verified_tx, inline_proof_verify) = Self::get_verified_transaction(
			ledger,
			tx,
			block_context,
			tx_hash,
			tblock_correction,
			current_state_key,
			crate::common::batch::batch_verify_block_enabled(),
		)?;

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
	use super::super::helpers_local::extract_tx_with_context;
	use super::*;
	use base_crypto_local::cost_model::{FixedPoint, SyntheticCost};
	use coin_structure_local::coin::{ShieldedTokenType, UnshieldedTokenType};
	use ledger_storage_local::DefaultDB;
	use midnight_node_res::{
		networks::{MidnightNetwork, UndeployedNetwork},
		undeployed::transactions::{DEPLOY_TX, STORE_TX},
	};
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
	fn revalidation_cache_roundtrip() {
		// Distinct keys so the shared process-global cache doesn't collide with other tests.
		let verified: Hash = [0xA1u8; 32];
		let known_bad: Hash = [0xB2u8; 32];
		let absent: Hash = [0xC3u8; 32];
		let state_key = b"state-key-bytes".to_vec();

		assert_eq!(get_revalidation_result(&absent), None, "missing key must be a cache miss");

		insert_revalidation_result(&verified, ProofOutcome::VerifiedAt(state_key.clone()));
		insert_revalidation_result(&known_bad, ProofOutcome::Invalid);

		// moka's sync cache guarantees read-your-writes per key.
		assert_eq!(
			get_revalidation_result(&verified),
			Some(ProofOutcome::VerifiedAt(state_key)),
			"a verified transaction must read back the state it was verified against",
		);
		assert_eq!(
			get_revalidation_result(&known_bad),
			Some(ProofOutcome::Invalid),
			"a known-bad proof must read back Invalid",
		);
		assert_eq!(get_revalidation_result(&absent), None, "unrelated key stays a miss");
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

	// ------------------------------------------------------------------------------------------
	// Cross-block ZK-proof re-verification
	// ------------------------------------------------------------------------------------------

	/// A `runtime_version` used only by the re-verification test, so its entries in the
	/// process-global validation caches cannot collide with any other test running in the same
	/// process (the cache key is `Twox128(runtime_version ++ tx_bytes)`).
	const REVERIFY_RUNTIME_VERSION: u32 = 0xDEAD_0001;

	/// `pallet_midnight::validate_unsigned` skews the mempool's `tblock` forward by
	/// `slot_duration_secs * (1 + MaxSkippedSlots)` — `6 * (1 + 1)` with the shipped defaults — so a
	/// transaction near the edge of its dust-validity window is not falsely rejected while blocks
	/// are being produced. `pre_dispatch` applies no such skew, so the two paths always disagree on
	/// this component of the strict-cache key.
	const MEMPOOL_TBLOCK_SKEW: u64 = 12;

	type TestBridge = Bridge<TransactionSignature, DefaultDB>;
	type TestTx = Transaction<TransactionSignature, DefaultDB>;

	/// Did `get_verified_transaction` run the ZK crypto *inline* for this call?
	///
	/// The [`VerifySample`] it returns is `Inline` exactly when `well_formed` verified the proofs
	/// itself (strict-cache miss **and** revalidation-cache miss); `Cached` on either cache hit and
	/// `Revalidate` when the revalidation reference let it skip the crypto — so this is a direct,
	/// non-invasive probe for "did we pay for the proofs again?".
	fn reverified(
		label: &str,
		ledger: &Ledger<DefaultDB>,
		tx: &TestTx,
		block_context: &BlockContext,
		key: &WrappedHash,
		state_key: &[u8],
	) -> bool {
		let verified = TestBridge::get_verified_transaction(
			ledger,
			tx,
			block_context,
			key,
			None,
			state_key,
			false,
		)
		.unwrap_or_else(|e| panic!("{label}: fixture transaction must be well-formed: {e:?}"))
		.1;
		let verified = matches!(verified, VerifySample::Inline(_));
		let state_hash: Hash = ledger.state.state_hash().0.into();
		println!(
			"  {label:<46} state_hash={} tblock={:<12} proofs_verified={}",
			&hex::encode(state_hash)[..8],
			block_context.tblock,
			if verified { "YES (crypto ran)" } else { "no  (cache hit)" },
		);
		verified
	}

	/// The `pallet_midnight::StateKey` bytes for a ledger — what the runtime threads into every
	/// host call, and what the revalidation cache records so a prior state can be reloaded.
	fn state_key_of(api: &api::Api, ledger: &Sp<Ledger<DefaultDB>, DefaultDB>) -> Vec<u8> {
		api.tagged_serialize(&ledger.as_typed_key()).expect("state key must serialize")
	}

	/// Applies `tx` to `ledger` and closes the block, as `execute_block` would.
	fn apply_and_close(
		api: &api::Api,
		ledger: &mut Sp<Ledger<DefaultDB>>,
		tx: &TestTx,
		block_context: &BlockContext,
	) {
		let tx_ctx = ledger.get_transaction_context(block_context.clone()).expect("tx context");
		let verified_tx =
			tx.0.well_formed(
				&tx_ctx.ref_state,
				mn_ledger_local::verify::WellFormedStrictness::default(),
				tx_ctx.block_context.tblock,
			)
			.unwrap_or_else(|e| panic!("fixture transaction must be well-formed: {e:?}"));
		let (next, _) = Ledger::<DefaultDB>::apply_verified_transaction(
			ledger.clone(),
			api,
			tx,
			&verified_tx,
			&tx_ctx,
		)
		.unwrap_or_else(|e| panic!("can't apply transaction: {e}"));
		*ledger = Ledger::<DefaultDB>::post_block_update(next, block_context.clone())
			.expect("post block update");
	}

	/// A transaction's ZK proofs are verified **once**, however many states it is validated
	/// against.
	///
	/// `STRICT_TX_VALIDATION_CACHE` is keyed by `{state_hash, tx_hash, block_context_tblock}`, and
	/// both non-`tx_hash` components move between mempool admission and block execution:
	///
	/// - `block_context_tblock` — the mempool skews it forward by [`MEMPOOL_TBLOCK_SKEW`]
	///   (`pallet_midnight::validate_unsigned`); `pre_dispatch` does not.
	/// - `state_hash` — `pallet_midnight` re-puts `StateKey` after every applied extrinsic, so it
	///   moves within a block as well as between blocks.
	///
	/// Either alone misses that cache, and a miss used to re-run the whole of `well_formed`,
	/// proofs included — measured at 2.00x verifications per transaction on a live node. The
	/// revalidation cache closes that: on a strict miss, a transaction already verified once is
	/// re-checked through the ledger's `RevalidationReference`, which skips `stateless_check`
	/// (proofs, signatures, binding commitments — all functions of the transaction bytes, so
	/// unchanged) and re-runs only the state-dependent checks whose inputs actually moved.
	///
	/// Phases 1 and 4 are the one full verification each transaction gets; 3 and 5 are the
	/// crossings that used to pay for it a second time.
	#[test]
	fn proofs_are_verified_once_per_transaction() {
		if super::super::CRATE_NAME != crate::latest::CRATE_NAME {
			println!("fixtures are ledger-9 only; skipping on {}", super::super::CRATE_NAME);
			return;
		}
		sp_tracing::try_init_simple();

		let api = api::new();
		let state: LedgerState<DefaultDB> =
			midnight_serialize_local::tagged_deserialize(UndeployedNetwork.genesis_state())
				.expect("genesis state must deserialize");
		let mut ledger = Sp::new(Ledger::new(state));

		// The fixtures record the block context each transaction was originally applied under, i.e.
		// the *block* context. The mempool would have seen the same context skewed forward.
		let (deploy_bytes, deploy_ctx) = extract_tx_with_context(DEPLOY_TX);
		let deploy_block_ctx: BlockContext = deploy_ctx.into();
		let deploy_mempool_ctx = BlockContext {
			tblock: deploy_block_ctx.tblock + MEMPOOL_TBLOCK_SKEW,
			..deploy_block_ctx.clone()
		};
		let deploy: TestTx = api.tagged_deserialize(&deploy_bytes).expect("deploy tx");
		let deploy_key =
			TestBridge::tx_validation_cache_key(REVERIFY_RUNTIME_VERSION, &deploy_bytes);

		let (store_bytes, store_ctx) = extract_tx_with_context(STORE_TX);
		let store_block_ctx: BlockContext = store_ctx.into();
		let store_mempool_ctx = BlockContext {
			tblock: store_block_ctx.tblock + MEMPOOL_TBLOCK_SKEW,
			..store_block_ctx.clone()
		};
		let store: TestTx = api.tagged_deserialize(&store_bytes).expect("store tx");
		let store_key = TestBridge::tx_validation_cache_key(REVERIFY_RUNTIME_VERSION, &store_bytes);

		let sk = state_key_of(&api, &ledger);
		println!("\n── block N: `deploy` is submitted, then included ──");
		assert!(
			reverified(
				"1. mempool admission",
				&ledger,
				&deploy,
				&deploy_mempool_ctx,
				&deploy_key,
				&sk,
			),
			"a transaction entering the mempool must have its proofs verified",
		);
		assert!(
			!reverified(
				"2. mempool revalidation (same key)",
				&ledger,
				&deploy,
				&deploy_mempool_ctx,
				&deploy_key,
				&sk,
			),
			"repeating the identical call must hit the strict cache — the cache does work when \
			 every key component matches",
		);
		assert!(
			!reverified(
				"3. block N execution (tblock differs)",
				&ledger,
				&deploy,
				&deploy_block_ctx,
				&deploy_key,
				&sk,
			),
			"the mempool's tblock skew still misses the strict cache, but the transaction is \
			 revalidated against the state step 1 verified it at, so the crypto does not re-run",
		);

		apply_and_close(&api, &mut ledger, &deploy, &deploy_block_ctx);

		let sk2 = state_key_of(&api, &ledger);
		println!("── block N+1: `store` is submitted against the new state, then included ──");
		assert!(
			reverified(
				"4. mempool admission",
				&ledger,
				&store,
				&store_mempool_ctx,
				&store_key,
				&sk2,
			),
			"a transaction entering the mempool must have its proofs verified",
		);
		assert!(
			!reverified(
				"5. block N+1 execution",
				&ledger,
				&store,
				&store_block_ctx,
				&store_key,
				&sk2,
			),
			"revalidated here too, across both a tblock and a state-hash change",
		);

		println!("── a transaction whose proofs are already known bad ──");
		insert_revalidation_result(&store.hash(), ProofOutcome::Invalid);
		let bad_ctx =
			BlockContext { tblock: store_block_ctx.tblock + 1, ..store_block_ctx.clone() };
		let rejected = TestBridge::get_verified_transaction(
			&ledger, &store, &bad_ctx, &store_key, None, b"", false,
		);
		assert!(
			rejected.is_err(),
			"a recorded Invalid outcome must reject without re-running the crypto",
		);
		println!("  6. block execution, proofs known bad         rejected without re-verifying");
	}

	/// How the aggregate crypto cost scales with batch size — i.e. what batching N incoming
	/// mempool transactions actually buys over verifying them one at a time.
	///
	/// Run explicitly:
	/// ```text
	/// cargo test -p midnight-node-ledger -p midnight-node-e2e --release --lib \
	///     bench_batch_verify_scaling -- --ignored --nocapture
	/// ```
	///
	/// The batch is built by repeating one fixture transaction. That is sound for a *timing*
	/// measurement — `batch_proof_verify` combines every proof's evidence into one aggregate
	/// check and does not short-circuit duplicates, so N copies cost N proofs' worth of work —
	/// and it sidesteps the workload-generation ceiling (genesis DUST sits in 5 outputs, so a
	/// real burst of N distinct transactions cannot be built on a fresh chain).
	#[test]
	#[ignore = "benchmark; run with --ignored --nocapture"]
	fn bench_batch_verify_scaling() {
		if super::super::CRATE_NAME != crate::latest::CRATE_NAME {
			println!("ledger-9 only; skipping on {}", super::super::CRATE_NAME);
			return;
		}
		sp_tracing::try_init_simple();

		let api = api::new();
		let state: LedgerState<DefaultDB> =
			midnight_serialize_local::tagged_deserialize(UndeployedNetwork.genesis_state())
				.expect("genesis");
		let ledger = Ledger::new(state);
		let (bytes, ctx_raw) = extract_tx_with_context(DEPLOY_TX);
		let block_ctx: BlockContext = ctx_raw.into();
		let tx: TestTx = api.tagged_deserialize(&bytes).expect("tx");
		let ctx = ledger.get_transaction_context(block_ctx.clone()).expect("tctx");
		let tblock = ctx.block_context.tblock;

		// Per-transaction baseline: a full `well_formed` (crypto included) minus the same call
		// with proofs deferred. The difference is the ZK crypto batching is meant to amortize.
		let full = mn_ledger_local::verify::WellFormedStrictness::default();
		let mut deferred = full;
		deferred.verify_contract_proofs = false;
		deferred.verify_native_proofs = false;
		let warm = tx.0.well_formed(&ctx.ref_state, full, tblock);
		assert!(warm.is_ok(), "fixture must verify: {:?}", warm.err());

		let t = Instant::now();
		let _ = tx.0.well_formed(&ctx.ref_state, full, tblock);
		let full_ms = t.elapsed().as_secs_f64() * 1e3;
		let t = Instant::now();
		let _ = tx.0.well_formed(&ctx.ref_state, deferred, tblock);
		let prep_ms = t.elapsed().as_secs_f64() * 1e3;
		let crypto_ms = full_ms - prep_ms;

		println!();
		println!(
			"per-tx inline: full={full_ms:.2}ms  non-crypto={prep_ms:.2}ms  crypto={crypto_ms:.2}ms"
		);
		println!();
		println!("   N   aggregate    per-tx   vs inline crypto");
		println!("  ───  ─────────  ────────  ─────────────────");
		for n in [1usize, 2, 4, 8, 16, 32, 64, 100] {
			let refs: Vec<&_> = (0..n).map(|_| &tx.0).collect();
			let t = Instant::now();
			let r = super::super::batch_verify::batch_verify_proofs(&refs, &ctx.ref_state, false);
			let ms = t.elapsed().as_secs_f64() * 1e3;
			assert!(r.is_ok(), "batch of {n} must verify: {:?}", r.err());
			let per = ms / n as f64;
			println!("  {n:>3}  {ms:>8.1}ms  {per:>6.2}ms  {:>13.2}x", crypto_ms / per);
		}
		println!();
	}
}

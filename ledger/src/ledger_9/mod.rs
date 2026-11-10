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

//! Ledger generation 9: the current ledger.
//!
//! Every module under `ledger_9/` binds the v9 ledger crates through the
//! `*_local` aliases declared below. Its `ledger_8/` counterpart is a separate
//! copy on purpose: `diff -r ledger_8 ledger_9` shows exactly where the two
//! generations diverge, and an edit here cannot leak into v8.

#[cfg(feature = "std")]
pub(crate) use {
	base_crypto as base_crypto_local, coin_structure_ledger_9 as coin_structure_local,
	ledger_storage_ledger_9 as ledger_storage_local,
	midnight_node_ledger_helpers::ledger_9 as helpers_local,
	midnight_serialize as midnight_serialize_local, mn_ledger_9 as mn_ledger_local,
	onchain_runtime_ledger_9 as onchain_runtime_local,
	transient_crypto_ledger_9 as transient_crypto_local, zswap_ledger_9 as zswap_local,
};

pub const CRATE_NAME: &str = "mn-ledger-9";

#[cfg(feature = "std")]
pub(crate) type TransactionSignature = mn_ledger_local::structure::Signature;

mod block_context;
pub use block_context::*;

mod guaranteed_validation;
mod post_block_update;
mod system_tx;

#[cfg(feature = "std")]
use midnight_serialize_local::Tagged;
#[cfg(feature = "std")]
use storage_sha2::digest::{OutputSizeUser, consts::U32};
#[cfg(feature = "std")]
use transient_crypto_local::commitment::PureGeneratorPedersen;

use alloc::vec::Vec;
use frame_support::{Blake2_256, StorageHasher};
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
			LedgerState, MaintenanceUpdate, OutputInstructionUnshielded, ProofMarker,
			SignatureKind, SingleUpdate, Transaction as LedgerTransaction, VerifiedTransaction,
		},
	},
	std::{
		any::Any,
		collections::BTreeMap,
		sync::Arc,
		time::{Duration, Instant},
	},
};

use crate::boundary::types::{
	ContractCallsDetails, FallibleCoinsDetails, GasCost, GuaranteedCoinsDetails, Hash, LedgerStats,
	Op, SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot, TransactionDetails, Tx,
	WrappedHash,
};

#[cfg(feature = "std")]
use {crate::boundary::types::split_tag, lazy_static::lazy_static, moka::sync::Cache};

pub const LOG_TARGET: &str = "midnight::ledger_v2";
pub const MINT_COINS_DOMAIN_SEPARATOR: &[u8; 10] = b"mint_coins";

/// `INITIAL_LIMITS.transaction_byte_limit`, for v1 host functions: old runtimes bind v1
/// forever, and the on-chain limit equals this at every historic block. Pinned by
/// `default_tx_byte_limit_matches_ledger_initial_limits`.
pub const DEFAULT_TX_BYTE_LIMIT: u64 = 1 << 20;

/// Cap on decoded operations/identifiers for v1 `get_decoded_transaction`; the v2 value comes
/// from the runtime (`pallet_midnight::Config::MaxDecodedTxItems`).
pub const DEFAULT_MAX_DECODED_ITEMS: u32 = 4096;

/// Measures the body alone, as `well_formed` does: it limits the untagged `serialized_size`,
/// which equals the body's length because deserialization insists on normal form. Old runtimes
/// reach this on block import, so counting the tag too would refuse transactions consensus
/// accepts and split old and new nodes. An untagged payload is measured whole; it cannot
/// deserialize anyway.
#[cfg(feature = "std")]
fn ensure_within_tx_byte_limit(limit: u64, bytes: &[u8]) -> Result<(), LedgerApiError> {
	let body = split_tag(bytes).map_or(bytes, |(_, body)| body);
	if body.len() as u64 > limit {
		return Err(LedgerApiError::Transaction(types::TransactionError::Malformed(
			types::MalformedError::TransactionTooLarge,
		)));
	}
	Ok(())
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StrictTxValidationKey {
	state_hash: Hash,
	tx_hash: Hash,
	/// The timestamp the cached `VerifiedTransaction` was verified at — the output of
	/// [`well_formed_tblock`], not the block's own `tblock`. Host-function v1 and v2 skew a
	/// block's first transaction differently, and both can run in one process; keying on what
	/// `well_formed` was actually given keeps them from reusing each other's entries.
	well_formed_tblock: u64,
	/// The storage the cached `VerifiedTransaction` lives in. Entries are downcast to the
	/// caller's `VerifiedTransaction<D>`, so storages must not share or replace each other's.
	storage: core::any::TypeId,
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
						let db = ParityDb::<
							storage_sha2::Sha256,
							_,
							{ LedgerStorageExt::COLUMN_OFFSET },
						>::from_existing_db(OwnedDb(db.clone()));
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
						let db = ParityDb::<storage_sha2::Sha256>::open(db_path.as_path());
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
		if !crate::ephemeral::is_onchain(externalities) {
			return;
		}
		let now = std::time::Instant::now();
		default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
		let elapsed = now.elapsed().as_secs_f64();

		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			metrics.observe_storage_flush_time(elapsed, "ledger_state");
		}
	}

	pub fn post_block_update(
		externalities: &mut dyn Externalities,
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
		let ledger = Ledger::post_block_update(ledger, block_context).inspect_err(|e| {
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
		crate::ephemeral::retain(externalities, &state_root, ledger);
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
		externalities: &mut dyn Externalities,
		state_key: &[u8],
		block_context: BlockContext,
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ledger = Ledger::apply_post_block_update(ledger, block_context);
		let state_root = api.tagged_serialize(&ledger.as_typed_key())?;
		crate::ephemeral::retain(externalities, &state_root, ledger);
		Ok(state_root)
	}

	pub fn get_version() -> Vec<u8> {
		crate::utils::find_crate_version(CRATE_NAME).unwrap_or(b"unknown".into())
	}

	/// `net_intra_tx_utxos` nets out UTXOs created and spent within the tx before the
	/// integrity check. It is consensus state, chosen by host-fn version: earlier runtimes
	/// recorded such txs as failed and must replay that way.
	#[allow(clippy::too_many_arguments)]
	pub fn apply_transaction(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		filter_only_failed_segments: bool,
		net_intra_tx_utxos: bool,
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
				Some(segments_to_exclude(&segments, filter_only_failed_segments))
			} else {
				None
			};
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Unshielded UTXOs ready (failed_segments={}, elapsed_ms={})",
			failed_segments.as_ref().map(|segments: &Vec<u16>| segments.len()).unwrap_or(0),
			start_tx_processing_time.elapsed().as_millis()
		);

		let operations = tx.calls_and_deploys(failed_segments);
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ops built (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);

		if net_intra_tx_utxos {
			utxos.net_intra_tx_utxos();
		}
		// Capture segment counts before flattening — the HashMap→BTreeMap fix
		// only changes ordering between segments, not within a single segment.
		let output_segments = utxos.outputs.len();
		let input_segments = utxos.inputs.len();

		let (mut utxo_outputs, mut utxo_inputs) =
			utxos.check_utxos_response_integrity(initial_utxos_size, &new_ledger.state)?;

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
		crate::ephemeral::retain(externalities, &event.state_root, new_ledger);
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
		log::debug!(
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
		crate::ephemeral::retain(externalities, &event.state_root, ledger);

		// Write Prometheus metrics
		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			let elapsed_time = start_system_tx_processing_time.elapsed().as_secs_f64();

			metrics.observe_system_txs_processing_time(elapsed_time, tx_type);
			metrics.observe_txs_size(tx_size as f64, tx_type);
		}

		Ok(event)
	}

	/// Shared body for the caller-restricted `apply_*_system_transaction` wrappers: one
	/// deserialize, one allow-list check, one FFI-crossing apply — as opposed to a
	/// separate classifier call before `apply_system_transaction`, which would deserialize
	/// and cross the WASM host-function boundary twice per call.
	fn apply_system_transaction_checked(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		is_allowed: impl FnOnce(&SystemTransaction) -> bool,
	) -> Result<SystemTransactionAppliedStateRoot, LedgerApiError> {
		// Gather metrics for Prometheus
		let start_system_tx_processing_time = Instant::now();
		let tx_size = tx_serialized.len();

		let api = api::new();
		let tx = api.tagged_deserialize::<SystemTransaction>(tx_serialized)?;
		if !is_allowed(&tx) {
			return Err(LedgerApiError::Transaction(types::TransactionError::SystemTransaction(
				types::SystemTransactionError::NotAllowedForCaller,
			)));
		}
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
		crate::ephemeral::retain(externalities, &event.state_root, ledger);

		// Write Prometheus metrics
		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			let elapsed_time = start_system_tx_processing_time.elapsed().as_secs_f64();

			metrics.observe_system_txs_processing_time(elapsed_time, tx_type);
			metrics.observe_txs_size(tx_size as f64, tx_type);
		}

		Ok(event)
	}

	/// Applies a system transaction submitted through the root-origin governance
	/// extrinsic. Only [`SystemTransaction::OverwriteParameters`] is allowed.
	pub fn apply_governance_system_transaction(
		externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
	) -> Result<SystemTransactionAppliedStateRoot, LedgerApiError> {
		Self::apply_system_transaction_checked(
			externalities,
			state_key,
			tx_serialized,
			block_context,
			|tx| matches!(tx, SystemTransaction::OverwriteParameters(_)),
		)
	}

	/// Applies a system transaction submitted by `pallet-cnight-observation`. Only
	/// [`SystemTransaction::CNightGeneratesDustUpdate`] is allowed.
	pub fn apply_cnight_system_transaction(
		externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
	) -> Result<SystemTransactionAppliedStateRoot, LedgerApiError> {
		Self::apply_system_transaction_checked(
			externalities,
			state_key,
			tx_serialized,
			block_context,
			|tx| matches!(tx, SystemTransaction::CNightGeneratesDustUpdate { .. }),
		)
	}

	/// Applies a system transaction submitted by `pallet-c2m-bridge`. Only
	/// `UnlockToTreasury`, `DistributeReserve`, and
	/// `DistributeNight(ClaimKind::CardanoBridge, _)` are allowed.
	pub fn apply_bridge_system_transaction(
		externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
	) -> Result<SystemTransactionAppliedStateRoot, LedgerApiError> {
		Self::apply_system_transaction_checked(
			externalities,
			state_key,
			tx_serialized,
			block_context,
			|tx| {
				matches!(tx, SystemTransaction::DistributeNight(ClaimKind::CardanoBridge, _))
					|| system_tx::is_distribute_reserve_system_tx(tx)
					|| system_tx::is_unlock_to_treasury_system_tx(tx)
			},
		)
	}

	/// Mempool validation. `check_utxos` adds [`Self::check_guaranteed_utxos`].
	#[allow(clippy::too_many_arguments)]
	pub fn validate_transaction(
		mut externalities: &mut dyn Externalities,
		state_key: &[u8],
		tx_serialized: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
		// The runtime's max weight as of now
		max_weight: u64,
		get_tx_details: bool,
		check_utxos: bool,
	) -> Result<(Hash, Option<TransactionDetails>), LedgerApiError> {
		// Gather metrics for Prometheus
		let start_tx_validation_time = Instant::now();

		let api = api::new();
		// Ledger first, so its `transaction_byte_limit` bounds the payload before deserializing.
		// Semantically inert: `well_formed` enforces the same limit, measured the same way, later.
		let ledger = Self::get_ledger(&api, state_key)?;
		ensure_within_tx_byte_limit(
			ledger.state.parameters.limits.transaction_byte_limit,
			tx_serialized,
		)?;
		let tx = api.tagged_deserialize_input::<Transaction<S, D>>(tx_serialized)?;

		let wrapped_cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// No `tblock` correction on the mempool path: `validate_unsigned` already skews the
		// block context it passes here by `slot_duration * (1 + MaxSkippedSlots)`.
		let was_cached = Self::do_validate_transaction(
			&ledger,
			&tx,
			&block_context,
			&wrapped_cache_key,
			check_utxos,
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
	/// `check_utxos` adds [`Self::check_guaranteed_utxos`]; a `pre_dispatch` rejection
	/// decides block validity, so like netting it is chosen by host-fn version.
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
		check_utxos: bool,
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		ensure_within_tx_byte_limit(
			ledger.state.parameters.limits.transaction_byte_limit,
			tx_serialized,
		)?;
		let tx = api.tagged_deserialize_input::<Transaction<S, D>>(tx_serialized)?;

		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// Perform dry-run validation with caching
		let was_cached = Self::do_validate_guaranteed_execution(
			&ledger,
			&tx,
			&block_context,
			&cache_key,
			skew_tblock,
			check_utxos,
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

	/// For v1 host functions, which old runtimes bind forever.
	pub fn get_decoded_transaction_v1(transaction_bytes: &[u8]) -> Result<Tx, LedgerApiError> {
		ensure_within_tx_byte_limit(DEFAULT_TX_BYTE_LIMIT, transaction_bytes)?;
		Self::decode_transaction_inner(transaction_bytes, DEFAULT_MAX_DECODED_ITEMS)
	}

	pub fn get_decoded_transaction(
		state_key: &[u8],
		transaction_bytes: &[u8],
		max_items: u32,
	) -> Result<Tx, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		ensure_within_tx_byte_limit(
			ledger.state.parameters.limits.transaction_byte_limit,
			transaction_bytes,
		)?;
		Self::decode_transaction_inner(transaction_bytes, max_items)
	}

	/// `max_items` caps operations and identifiers independently — it bounds output
	/// materialization and response size, not the (already byte-limited) parse.
	fn decode_transaction_inner(
		transaction_bytes: &[u8],
		max_items: u32,
	) -> Result<Tx, LedgerApiError> {
		let too_many_items = || {
			LedgerApiError::Transaction(types::TransactionError::Malformed(
				types::MalformedError::TransactionTooLarge,
			))
		};
		let api = api::new();
		let tx = api.tagged_deserialize_input::<Transaction<S, D>>(transaction_bytes)?;
		let hash = tx.hash();
		let operations = tx.calls_and_deploys(None).try_fold(Vec::new(), |mut acc, cd| {
			if acc.len() >= max_items as usize {
				return Err(too_many_items());
			}
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
			if acc.len() >= max_items as usize {
				return Err(too_many_items());
			}
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

	/// Whether the `Ledger` at `state_key` has accrued no `block_fullness`, as it must at the end
	/// of every block.
	#[cfg(feature = "test-utils")]
	pub fn is_block_start(state_key: &[u8]) -> Result<bool, LedgerApiError> {
		let api = api::new();
		Ok(Self::get_ledger(&api, state_key)?.is_block_start())
	}

	/// Recomputes `state_key` the way external consumers such as the indexer do: as the tagged
	/// arena key of `Ledger { state, block_fullness: 0 }` for the ledger state it points to.
	#[cfg(feature = "test-utils")]
	pub fn block_start_state_key(state_key: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let rebuilt = default_storage::<D>().arena.alloc(Ledger::new(ledger.state.clone()));
		api.tagged_serialize(&rebuilt.as_typed_key())
	}

	/// Collection sizes held at the `LedgerState` root (`midnight_ledgerStats`).
	///
	/// Every value is O(1). The counts are read off annotations maintained at each
	/// storage trie root — `NightAnn { size, value }` for the UTXO and contract
	/// maps, `SizeAnn` for the nullifier sets — and the commitment totals are
	/// scalar `first_free` fields. Nothing iterates, so the cost is independent of
	/// how large the state has grown; the only work is forcing the handful of root
	/// nodes that are not already resident.
	pub fn ledger_stats(state_key: &[u8]) -> Result<LedgerStats, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let st = &ledger.state;

		let utxo_ann = st.utxo.utxos.ann();

		Ok(LedgerStats {
			unshielded_utxo_count: utxo_ann.size,
			unshielded_utxo_stars: utxo_ann.value,
			zswap_commitment_count: st.zswap.first_free,
			zswap_nullifier_count: st.zswap.nullifiers.size() as u64,
			dust_commitment_count: st.dust.utxo.commitments_first_free,
			dust_nullifier_count: st.dust.utxo.nullifiers.size() as u64,
			contract_count: st.contract.ann().size,
		})
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
		let tx = api.tagged_deserialize_input::<Transaction<S, D>>(tx)?;
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

	/// For v1 host functions, which old runtimes bind forever.
	pub fn get_transaction_cost_v1(
		state_key: &[u8],
		tx: &[u8],
		block_context: &BlockContext,
		max_weight: u64,
	) -> Result<GasCost, LedgerApiError> {
		ensure_within_tx_byte_limit(DEFAULT_TX_BYTE_LIMIT, tx)?;
		Self::get_transaction_cost(state_key, tx, block_context, max_weight)
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
		// Caller-controlled bytes, via `state_call` and the mempool's filtering pool: bound them
		// before either arm deserializes.
		let ledger = Self::get_ledger(&api::new(), state_key)?;
		ensure_within_tx_byte_limit(ledger.state.parameters.limits.transaction_byte_limit, tx)?;

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

	fn get_ledger(api: &api::Api, state_key: &[u8]) -> Result<Sp<Ledger<D>, D>, LedgerApiError> {
		let key: TypedArenaKey<Ledger<D>, D::Hasher> = api.tagged_deserialize(state_key)?;
		let arena = &default_storage().arena;
		// `get_lazy` reads nothing, so an absent root would only surface as a host panic on
		// first deref. Read the root node here instead: one level, children stay unread, and
		// the backend caches it for that deref. A missing descendant can still panic later.
		if let ArenaKey::Ref(root) = &key.key
			&& arena.with_backend(|backend| backend.get(root).is_none())
		{
			log::error!(target: LOG_TARGET, "Ledger state root {root:?} not in storage");
			return Err(LedgerApiError::NoLedgerState);
		}
		arena.get_lazy(&key).map_err(|_| LedgerApiError::NoLedgerState)
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
		WrappedHash(Blake2_256::hash(&to_hash))
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

	/// The UTXO integrity check `apply_transaction` runs, applied to the guaranteed dry run
	/// so a transaction it would reject after apply never enters a block.
	fn check_guaranteed_utxos(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		after: &LedgerState<D>,
	) -> Result<(), LedgerApiError> {
		let initial_utxos_size = ledger.state.utxo.utxos.size();
		tx.unshielded_utxos().check_guaranteed_integrity(initial_utxos_size, after)
	}

	/// Validates a transaction for the mempool using the soft cache.
	///
	/// Uses `tx_hash` only for quick revalidation of transactions already in the pool.
	/// The soft cache prevents redundant ZK proof verification for mempool housekeeping.
	/// With `check_utxos`, [`Self::check_guaranteed_utxos`] must pass before the success is
	/// cached. The key omits the flag: a runtime binds one host-fn version, and `tx_hash`
	/// hashes in its `spec_version`.
	///
	/// Returns `true` if the validation was served from cache, `false` if validation was performed.
	fn do_validate_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		check_utxos: bool,
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

		match guaranteed_validation::validate_guaranteed_execution(&ledger.state, verified_tx, &ctx)
		{
			Ok(applied) => {
				if check_utxos {
					Self::check_guaranteed_utxos(ledger, tx, &applied)?;
				}
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
					"🚫 Rejected transaction {} from mempool: guaranteed execution would fail: {reason}",
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
	/// version-specific `guaranteed_validation` module), then, with `check_utxos`,
	/// [`Self::check_guaranteed_utxos`] on the resulting state, to validate that the
	/// transaction can enter a block.
	///
	/// Returns `true` if validation was served from the strict cache, `false` otherwise.
	fn do_validate_guaranteed_execution(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
		skew_tblock: bool,
		check_utxos: bool,
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

		match guaranteed_validation::validate_guaranteed_execution(&ledger.state, verified_tx, &ctx)
		{
			Ok(applied) => {
				if check_utxos {
					Self::check_guaranteed_utxos(ledger, tx, &applied)?;
				}
				Ok(was_cached)
			},
			Err(reason) => {
				log::warn!(
					target: LOG_TARGET,
					"🚫 Rejecting transaction {} at pre-dispatch: guaranteed execution would fail: {reason}",
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
			// Not `api.deserialize` (logs at error): invalid keys are expected user input.
			owner: midnight_serialize_local::Deserializable::deserialize(&mut &owner[..], 0)
				.map_err(|e| {
					log::debug!(target: LOG_TARGET, "Invalid DustPublicKey: {e:?}");
					LedgerApiError::Deserialization(api::DeserializationError::DustPublicKey)
				})?,
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
		let system_tx = system_tx::distribute_reserve_system_tx(amount);
		api.tagged_serialize(&system_tx)
	}

	pub fn construct_unlock_to_treasury_system_tx(amount: u128) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let system_tx = system_tx::unlock_to_treasury_system_tx(amount)?;
		api.tagged_serialize(&system_tx)
	}

	pub fn construct_distribute_treasury_system_tx(
		amount: u128,
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let system_tx = system_tx::distribute_treasury_system_tx(amount)?;
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
		tx if system_tx::is_distribute_reserve_system_tx(tx) => Ok("distribute_reserve"),
		tx if system_tx::is_unlock_to_treasury_system_tx(tx) => Ok("unlock_to_treasury"),
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

/// Segments whose contract actions are withheld from the emitted event.
///
/// `only_failed = false` is the v1 `apply_transaction` behaviour, kept for replaying old blocks:
/// every attempted segment is excluded, succeeding ones included.
#[cfg(feature = "std")]
fn segments_to_exclude<E>(segments: &BTreeMap<u16, Result<(), E>>, only_failed: bool) -> Vec<u16> {
	segments
		.iter()
		.filter(|(_, result)| !only_failed || result.is_err())
		.map(|(segment_id, _)| *segment_id)
		.collect()
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
		storage: core::any::TypeId::of::<D>(),
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
	use base_crypto_local::{
		cost_model::{FixedPoint, SyntheticCost},
		rng::SplittableRng,
		schnorr,
	};
	use coin_structure_local::coin::{NIGHT, ShieldedTokenType, UnshieldedTokenType};
	use helpers_local::{Rng, SeedableRng, StdRng};
	use ledger_storage_local::{DefaultDB, storage::HashMap as StorageHashMap};
	use mn_ledger_local::structure::{
		INITIAL_PARAMETERS, Intent, MAX_SUPPLY, SigningKey, StandardTransaction, UnshieldedOffer,
		UtxoOutput, UtxoSpend,
	};
	use transient_crypto_local::commitment::PedersenRandomness;

	/// The undeployed genesis `StateKey` (see `storage.rs`) with its last hash byte changed:
	/// well-formed, but naming a root that was never stored.
	const ABSENT_ROOT_STATE_KEY: &str = "6d69646e696768743a73746f726167652d6b6579286c65646765722d73746174655b7631385d293a0067100cb9fcfc392a863e7b22751c6baf4602e96deef27c4e5339205586ab4400";

	/// A node whose ledger DB lost the root its block state names must get an error back, not
	/// a host panic on first deref that wedges block authoring.
	#[test]
	fn absent_ledger_root_is_no_ledger_state_not_a_panic() {
		let state_key = hex::decode(ABSENT_ROOT_STATE_KEY).expect("valid hex");
		let result = Bridge::<TransactionSignature, DefaultDB>::get_ledger_parameters(&state_key);
		assert!(matches!(result, Err(LedgerApiError::NoLedgerState)), "got {result:?}");
	}

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

	fn transaction_too_large() -> LedgerApiError {
		LedgerApiError::Transaction(types::TransactionError::Malformed(
			types::MalformedError::TransactionTooLarge,
		))
	}

	/// A ledger bump that changes `INITIAL_LIMITS` must fail loudly: the v1 host
	/// functions hardcode this value as the byte limit for historic blocks.
	#[test]
	fn default_tx_byte_limit_matches_ledger_initial_limits() {
		assert_eq!(
			DEFAULT_TX_BYTE_LIMIT,
			mn_ledger_local::structure::INITIAL_LIMITS.transaction_byte_limit
		);
	}

	/// The v1 byte check precedes deserialization and state loading, so junk bytes
	/// and a junk state key are fine — nothing else may run first.
	#[test]
	fn v1_query_paths_reject_oversized_transactions() {
		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let oversized = tagged_zeros(DEFAULT_TX_BYTE_LIMIT as usize + 1);
		assert_eq!(
			TestBridge::get_decoded_transaction_v1(&oversized).unwrap_err(),
			transaction_too_large()
		);
		assert_eq!(
			TestBridge::get_transaction_cost_v1(&[], &oversized, &block_context(), 1_000)
				.unwrap_err(),
			transaction_too_large()
		);
	}

	/// The tag prefix plus `body_len` zero bytes. `well_formed` measures only the body.
	fn tagged_zeros(body_len: usize) -> Vec<u8> {
		let tag = <Transaction<TransactionSignature, DefaultDB> as Tagged>::tag();
		let mut bytes = format!("{}{tag}:", midnight_serialize_local::GLOBAL_TAG).into_bytes();
		bytes.resize(bytes.len() + body_len, 0);
		bytes
	}

	/// The byte check must never be stricter than `well_formed`, which ignores the tag
	/// prefix: refusing a body it accepts splits old and new nodes on block import.
	/// `split_tag` ends the tag at its second colon, so a tag holding one would have the byte
	/// check count part of the tag as body and refuse transactions consensus accepts.
	#[test]
	fn split_tag_finds_the_real_tags() {
		let tags = [
			<Transaction<TransactionSignature, DefaultDB> as Tagged>::tag(),
			<SystemTransaction as Tagged>::tag(),
		];
		for tag in tags {
			assert!(!tag.contains(':'), "tag `{tag}` holds a colon");
		}
		assert_eq!(split_tag(&tagged_zeros(5)).map(|(_, body)| body.len()), Some(5));
	}

	#[test]
	fn v1_query_paths_admit_a_body_at_the_limit() {
		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let at_limit = tagged_zeros(DEFAULT_TX_BYTE_LIMIT as usize);
		assert_ne!(
			TestBridge::get_decoded_transaction_v1(&at_limit).unwrap_err(),
			transaction_too_large()
		);
		assert_ne!(
			TestBridge::get_transaction_cost_v1(&[], &at_limit, &block_context(), 1_000)
				.unwrap_err(),
			transaction_too_large()
		);
	}

	#[test]
	fn decoded_items_are_capped() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			// The fixture transaction is latest-ledger format.
			println!("This test should only be run with ledger latest");
			return;
		}

		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let (tx, _) = midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(
			midnight_node_res::undeployed::transactions::DEPLOY_TX,
		);
		assert_eq!(
			TestBridge::decode_transaction_inner(&tx, 0).unwrap_err(),
			transaction_too_large()
		);
		assert!(TestBridge::decode_transaction_inner(&tx, DEFAULT_MAX_DECODED_ITEMS).is_ok());
	}

	/// `DEPLOY_TX` with its intent repeated under `extra` more segment ids.
	fn deploy_tx_with_repeated_intent(extra: u16) -> Vec<u8> {
		let api = api::new();
		let (base, _) = midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(
			midnight_node_res::undeployed::transactions::DEPLOY_TX,
		);
		let base = api
			.tagged_deserialize::<Transaction<TransactionSignature, DefaultDB>>(&base)
			.expect("DEPLOY_TX deserializes");
		let LedgerTransaction::Standard(standard) = base.0 else {
			panic!("DEPLOY_TX is a standard transaction");
		};
		let intent = standard.intents.clone().into_iter().next().expect("one intent").1;
		let mut tx = standard;
		for segment in 10_000..10_000 + extra {
			tx.intents = tx.intents.clone().insert(segment, intent.clone());
		}
		api.tagged_serialize(&LedgerTransaction::Standard(tx))
			.expect("the transaction serializes")
	}

	/// Assert consequences rather than the encoding, so a tighter encoding (say, varint
	/// `TopoSortedNode::child_indices`) only fails this when it actually matters.
	#[test]
	fn byte_limited_payload_cannot_declare_unbounded_work() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let measure = |extra: u16| {
			let bytes = deploy_tx_with_repeated_intent(extra);
			let ops = TestBridge::decode_transaction_inner(&bytes, u32::MAX)
				.expect("the transaction decodes")
				.operations
				.len();
			assert_eq!(
				ops,
				usize::from(extra) + 1,
				"each extra segment must decode to one operation"
			);
			let nodes = crate::boundary::types::declared_node_count(&bytes).unwrap();
			(bytes.len() as f64, f64::from(nodes))
		};
		let (small_bytes, small_nodes) = measure(64);
		let (large_bytes, large_nodes) = measure(1024);
		let bytes_per_op = (large_bytes - small_bytes) / 960.0;
		let nodes_per_op = (large_nodes - small_nodes) / 960.0;
		// Bytes per segment grow with the segment count, so this overestimates.
		let ops_at_limit = DEFAULT_TX_BYTE_LIMIT as f64 / bytes_per_op;

		assert!(
			ops_at_limit <= 50_000.0,
			"a payload at the byte limit decodes to ~{ops_at_limit:.0} operations",
		);
		// pallet-midnight's `the_node_cap_binds_before_the_item_cap` assumes this ratio.
		assert!(
			nodes_per_op >= 4.0,
			"an operation costs only {nodes_per_op:.2} nodes, so `MaxTransactionNodes` admits \
			 more operations than pallet-midnight's NODES_PER_OPERATION assumes",
		);
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

		if CRATE_NAME != crate::latest::CRATE_NAME {
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
		let tx = system_tx::distribute_reserve_system_tx(0);
		assert_eq!(get_system_tx_type(&tx).unwrap(), "distribute_reserve");
	}

	#[test]
	fn get_system_tx_type_cnight_generates_dust_update() {
		let tx = SystemTransaction::CNightGeneratesDustUpdate { events: vec![] };
		assert_eq!(get_system_tx_type(&tx).unwrap(), "cnight_generates_dust_update");
	}

	#[test]
	fn get_system_tx_type_unlock_to_treasury() {
		if let Ok(tx) = system_tx::unlock_to_treasury_system_tx(0) {
			assert_eq!(get_system_tx_type(&tx).unwrap(), "unlock_to_treasury");
		}
	}

	#[test]
	fn segments_to_exclude_v2_withholds_only_failed_segments() {
		let segments: BTreeMap<u16, Result<(), ()>> =
			[(0, Ok(())), (1, Err(())), (2, Ok(()))].into_iter().collect();

		// v2: the guaranteed segment and the succeeding fallible segment keep their events.
		assert_eq!(segments_to_exclude(&segments, true), vec![1]);

		// v1 (historical replay): everything attempted is withheld.
		assert_eq!(segments_to_exclude(&segments, false), vec![0, 1, 2]);
	}

	/// `validate_unsigned` and `pre_dispatch` reach these with caller bytes ahead of any fee,
	/// and before `CheckWeight` can reject a bare extrinsic on length.
	#[test]
	fn validation_paths_reject_oversized_payloads_before_deserializing() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let api = api::new();
		let genesis = midnight_node_res::networks::MidnightNetwork::genesis_state(
			&midnight_node_res::networks::UndeployedNetwork,
		);
		let state: LedgerState<DefaultDB> =
			midnight_serialize_local::tagged_deserialize(genesis).unwrap();
		let mut ledger = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		ledger.persist();
		let state_key = api.tagged_serialize(&ledger.as_typed_key()).unwrap();

		let limit = ledger.state.parameters.limits.transaction_byte_limit;
		let oversized = tagged_zeros(limit as usize + 1);

		let is_too_large = |err: &LedgerApiError| {
			matches!(
				err,
				LedgerApiError::Transaction(types::TransactionError::Malformed(
					types::MalformedError::TransactionTooLarge
				))
			)
		};

		let mut test_ext = sp_io::TestExternalities::default();
		let mut ext = test_ext.ext();

		let validate = TestBridge::validate_transaction(
			&mut ext,
			&state_key,
			&oversized,
			BlockContext::default(),
			1,
			1,
			false,
			true,
		)
		.unwrap_err();
		assert!(is_too_large(&validate), "validate_transaction returned {validate:?}");

		let guaranteed = TestBridge::validate_guaranteed_execution(
			&mut ext,
			&state_key,
			&oversized,
			BlockContext::default(),
			1,
			false,
			true,
		)
		.unwrap_err();
		assert!(is_too_large(&guaranteed), "validate_guaranteed_execution returned {guaranteed:?}");
	}

	/// As `v1_query_paths_admit_a_body_at_the_limit`, for the stateful paths, block import's
	/// among them.
	#[test]
	fn stateful_paths_admit_a_body_at_the_limit() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		type TestBridge = Bridge<TransactionSignature, DefaultDB>;

		let api = api::new();
		let genesis = midnight_node_res::networks::MidnightNetwork::genesis_state(
			&midnight_node_res::networks::UndeployedNetwork,
		);
		let state: LedgerState<DefaultDB> =
			midnight_serialize_local::tagged_deserialize(genesis).unwrap();
		let mut ledger = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		ledger.persist();
		let state_key = api.tagged_serialize(&ledger.as_typed_key()).unwrap();

		let at_limit = tagged_zeros(ledger.state.parameters.limits.transaction_byte_limit as usize);

		let mut test_ext = sp_io::TestExternalities::default();
		let mut ext = test_ext.ext();

		let validate = TestBridge::validate_transaction(
			&mut ext,
			&state_key,
			&at_limit,
			BlockContext::default(),
			1,
			1,
			false,
			true,
		)
		.unwrap_err();
		assert_ne!(validate, transaction_too_large());

		let guaranteed = TestBridge::validate_guaranteed_execution(
			&mut ext,
			&state_key,
			&at_limit,
			BlockContext::default(),
			1,
			false,
			true,
		)
		.unwrap_err();
		assert_ne!(guaranteed, transaction_too_large());

		let cost =
			TestBridge::get_any_transaction_cost(&state_key, &at_limit, &block_context(), 1_000)
				.unwrap_err();
		assert_ne!(cost, transaction_too_large());

		let decoded =
			TestBridge::get_decoded_transaction(&state_key, &at_limit, DEFAULT_MAX_DECODED_ITEMS)
				.unwrap_err();
		assert_ne!(decoded, transaction_too_large());
	}

	/// End to end: intent 1 creates an unshielded UTXO in its guaranteed section and intent 2
	/// spends it in its own; the ledger applies that in segment order. Through the host-fn
	/// versions that net (`apply_transaction` v3, `validate_transaction` and
	/// `validate_guaranteed_execution` v2), mempool validation, `pre_dispatch` and apply all
	/// accept it, and the UTXO is reported as neither created nor spent. Through the older
	/// versions the old verdict stands: validation admits it and apply rejects it, so the tx
	/// sits in a block without paying.
	#[test]
	fn a_tx_spending_a_utxo_it_created_passes_validation_and_apply() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		type TestBridge = Bridge<TransactionSignature, DefaultDB>;
		type TestIntent = Intent<TransactionSignature, ProofMarker, PedersenRandomness, DefaultDB>;
		const NETWORK: &str = "undeployed";
		const VALUE: u128 = 1_000_000;

		// Zero fee prices: the fee-less transaction then balances under default strictness.
		let mut params = INITIAL_PARAMETERS;
		params.fee_prices.overall_price = FixedPoint::ZERO;
		let state = LedgerState::with_genesis_settings(NETWORK, params, 0, MAX_SUPPLY, 0).unwrap();

		let mut rng = StdRng::seed_from_u64(0x4e35_3200);
		let sk = SigningKey::Schnorr(schnorr::SigningKey::sample(&mut rng));
		let vk = sk.verifying_key();
		let (r1, r2) = (rng.r#gen(), rng.r#gen());
		let offer = |inputs: Vec<UtxoSpend>, outputs: Vec<UtxoOutput>| {
			let (inputs, outputs, signatures) = (inputs.into(), outputs.into(), vec![].into());
			Some(Sp::new(UnshieldedOffer { inputs, outputs, signatures }))
		};
		let intent = |binding_commitment, guaranteed_unshielded_offer| TestIntent {
			guaranteed_unshielded_offer,
			fallible_unshielded_offer: None,
			actions: vec![].into(),
			dust_actions: None,
			ttl: Timestamp::from_secs(0),
			binding_commitment,
		};

		let output = UtxoOutput { owner: vk.clone().into(), type_: NIGHT, value: VALUE };
		let create = intent(r1, offer(vec![], vec![output]));
		let y = UtxoSpend {
			intent_hash: create.erase_proofs().erase_signatures().intent_hash(0),
			output_no: 0,
			owner: vk,
			type_: NIGHT,
			value: VALUE,
		};
		let spend = intent(r2, offer(vec![y], vec![])).sign(&mut rng, 2, &[sk], &[], &[]).unwrap();
		let tx = LedgerTransaction::Standard(StandardTransaction {
			network_id: NETWORK.into(),
			intents: [(1, create), (2, spend)].into_iter().collect(),
			guaranteed_coins: None,
			fallible_coins: StorageHashMap::new(),
			binding_randomness: r1 + r2,
		})
		.seal(rng.split());

		let api = api::new();
		let tx = api.tagged_serialize(&tx).unwrap();
		let mut ledger = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		ledger.persist();
		let key = api.tagged_serialize(&ledger.as_typed_key()).unwrap();
		let mut test_ext = sp_io::TestExternalities::default();
		let mut ext = test_ext.ext();
		let bc = BlockContext::default;

		// Mempool, `pre_dispatch` and apply, through the new host-fn versions or the old.
		// A runtime binds one version and its `spec_version` keys the validation caches,
		// so each arm runs under its own `runtime_version`.
		let mut run = |new: bool| {
			let (ext, rv) = (&mut ext, u32::from(new));
			(
				TestBridge::validate_transaction(ext, &key, &tx, bc(), rv, 1, false, new),
				TestBridge::validate_guaranteed_execution(ext, &key, &tx, bc(), rv, false, new),
				TestBridge::apply_transaction(ext, &key, &tx, bc(), true, new, rv, false),
			)
		};

		let (validated, dispatched, applied) = run(true);
		validated.expect("mempool validation accepts it");
		dispatched.expect("pre_dispatch accepts it");
		let applied = applied.expect("apply accepts it");
		assert!(applied.all_applied);
		assert!(applied.unshielded_utxos_created.is_empty(), "netted: not reported as created");
		assert!(applied.unshielded_utxos_spent.is_empty(), "netted: not reported as spent");

		let (validated, dispatched, applied) = run(false);
		validated.expect("old versions do not check at validation");
		dispatched.expect("old versions do not check at pre_dispatch");
		assert_eq!(applied.err(), Some(LedgerApiError::HostApiError), "old verdict replays");
	}
}

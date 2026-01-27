// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
use {
	api::{
		ContractAddress, ContractState, Ledger, LedgerParameters, SystemTransaction, Transaction,
		TransactionAppliedStage, TransactionOperation,
	},
	base_crypto_local::{
		cost_model::NormalizedCost as LedgerNormalizedCost, hash::HashOutput, time::Timestamp,
	},
	coin_structure_local::coin::Nonce,
	ledger_storage_local::{
		Storage,
		arena::{ArenaKey, Sp, TypedArenaKey},
		db::{DB, ParityDb},
		storage::{default_storage, set_default_storage},
	},
	midnight_primitives_ledger::{LedgerMetricsExt, LedgerStorageExt, SyncStatusExt},
	mn_ledger_local::{
		dust::InitialNonce,
		structure::{
			CNightGeneratesDustActionType, CNightGeneratesDustEvent, ClaimKind, ContractAction,
			MaintenanceUpdate, ProofMarker, SignatureKind, SingleUpdate,
			Transaction as LedgerTransaction, VerifiedTransaction,
		},
	},
	std::{
		any::Any,
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
use {lazy_static::lazy_static, moka::sync::Cache};

pub const LOG_TARGET: &str = "midnight::ledger_v2";
pub const MINT_COINS_DOMAIN_SEPARATOR: &[u8; 10] = b"mint_coins";

#[derive(PartialEq, Eq, Hash)]
pub struct StrictTxValidationKey {
	state_hash: Hash,
	tx_hash: Hash,
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
			let res = set_default_storage(|| {
				let db = ParityDb::<sha2::Sha256>::open(storage.0.db_path.as_path());
				Storage::new(storage.0.cache_size, db)
			});
			if res.is_err() {
				log::warn!("Warning: Failed to set default storage: {res:?}");
			}
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
		default_storage::<D>().with_backend(|backend| {
			backend.flush_all_changes_to_db();
			backend.gc();
		});
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
		let mut ledger = Ledger::post_block_update(ledger, block_context).map_err(|e| {
			log::error!(
				target: LOG_TARGET,
				"Post Block Update error: {e:?}"
			);
			LedgerApiError::NoLedgerState
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
		log::trace!(
			target: LOG_TARGET,
			"⏱️  Ledger loaded (elapsed_ms={})",
			start_tx_processing_time.elapsed().as_millis()
		);
		let initial_utxos_size = ledger.state.utxo.utxos.size();

		// Use cached VerifiedTransaction if available
		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);
		let verified_tx = Self::get_verified_transaction(&ledger, &tx, &block_context, &cache_key)?;
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

		let is_syncing =
			externalities.extension::<SyncStatusExt>().is_some_and(|ext| ext.is_syncing());
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

		let (utxo_outputs, utxo_inputs) =
			utxos.check_utxos_response_integrity(initial_utxos_size, &new_ledger)?;

		// During sync, shuffle segment ordering to probabilistically match historical
		// blocks produced with non-deterministic HashMap iteration order
		let (utxo_outputs, utxo_inputs) = if is_syncing {
			(utxos.outputs_shuffled(), utxos.inputs_shuffled())
		} else {
			(utxo_outputs, utxo_inputs)
		};

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
				TransactionOperation::ClaimRewards { value, .. } => {
					event.claim_rewards.push(value);
					log::trace!(
						target: LOG_TARGET,
						"⏱️  Tx op: ClaimRewards (elapsed_ms={})",
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
		let tx_type = Self::get_system_tx_type(&tx);
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
	) -> Result<(), LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		// Perform dry-run validation with caching
		let was_cached =
			Self::do_validate_guaranteed_execution(&ledger, &tx, &block_context, &cache_key)?;

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

		ledger.get_contract_state(addr).map_or(Ok(Vec::new()), f)
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

		Ok(*ledger.get_unclaimed_amount(night_addr).unwrap_or(&0))
	}

	pub fn get_ledger_parameters(state_key: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;
		let ledger_parameters = Self::get_deserialized_ledger_parameters(&ledger);
		api.tagged_serialize(&ledger_parameters)
	}

	// TODO COST MODEL: Needs to be redone with the new ledger cost model
	#[allow(unused_variables)]
	pub fn get_transaction_cost(
		state_key: &[u8],
		tx: &[u8],
		block_context: &BlockContext,
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

	fn get_system_tx_type(tx: &SystemTransaction) -> &'static str {
		match tx {
			SystemTransaction::OverwriteParameters(_) => "overwrite_parameters",
			SystemTransaction::DistributeNight(claim_kind, _) => match claim_kind {
				ClaimKind::Reward => "distribute_night_reward",
				ClaimKind::CardanoBridge => "distribute_night_cardano_bridge",
			},
			SystemTransaction::PayBlockRewardsToTreasury { .. } => "pay_block_rewards_to_treasury",
			SystemTransaction::PayFromTreasuryShielded { .. } => "pay_from_treasury_shielded",
			SystemTransaction::PayFromTreasuryUnshielded { .. } => "pay_from_treasury_unshielded",
			SystemTransaction::DistributeReserve(_) => "distribute_reserve",
			SystemTransaction::CNightGeneratesDustUpdate { .. } => "cnight_generates_dust_update",
			_ => "unknown",
		}
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
	) -> Result<VerifiedTransaction<D>, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		let state_hash = ledger.state.state_hash();
		let strict_key =
			StrictTxValidationKey { state_hash: state_hash.0.into(), tx_hash: tx_hash.0 };

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
				ctx.block_context.tblock,
			)
			.map_err(|e| {
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
		let verified_tx = match Self::get_verified_transaction(ledger, tx, block_context, tx_hash) {
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

		// Dry-run apply to validate guaranteed execution against current state
		let ctx = ledger.get_transaction_context(block_context.clone())?;
		let (_next_state, result) = ledger.state.apply(&verified_tx, &ctx);

		match result {
			mn_ledger_local::semantics::TransactionResult::Success(_)
			| mn_ledger_local::semantics::TransactionResult::PartialSuccess(_, _) => {
				log::info!(
					target: LOG_TARGET,
					"📋 Validated transaction {} for mempool",
					tx_hash_hex
				);
				// Cache the success (only successes are cached)
				SOFT_TX_VALIDATION_CACHE.insert(soft_key, Ok(()));
				Ok(false)
			},
			mn_ledger_local::semantics::TransactionResult::Failure(reason) => {
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
	/// `VerifiedTransaction`, then performs a dry-run `apply()` to validate
	/// the guaranteed part will succeed.
	///
	/// Returns `true` if validation was served from the strict cache, `false` otherwise.
	fn do_validate_guaranteed_execution(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
	) -> Result<bool, LedgerApiError>
	where
		VerifiedTransaction<D>: Send + Sync + 'static,
	{
		// Invalidate soft cache — tx must re-validate after a block authoring attempt
		SOFT_TX_VALIDATION_CACHE.invalidate(&SoftTxValidationKey { tx_hash: tx_hash.0 });

		// Check strict cache to determine if this is a cache hit
		let state_hash = ledger.state.state_hash();
		let strict_key =
			StrictTxValidationKey { state_hash: state_hash.0.into(), tx_hash: tx_hash.0 };
		let was_cached = STRICT_TX_VALIDATION_CACHE.get(&strict_key).is_some();

		let verified_tx = Self::get_verified_transaction(ledger, tx, block_context, tx_hash)?;

		let ctx = ledger.get_transaction_context(block_context.clone())?;
		let (_next_state, result) = ledger.state.apply(&verified_tx, &ctx);

		match result {
			mn_ledger_local::semantics::TransactionResult::Success(_)
			| mn_ledger_local::semantics::TransactionResult::PartialSuccess(_, _) => Ok(was_cached),
			mn_ledger_local::semantics::TransactionResult::Failure(reason) => {
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
}

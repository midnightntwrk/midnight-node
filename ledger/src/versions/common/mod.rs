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
use transient_crypto_local::commitment::PureGeneratorPedersen;

use frame_support::{StorageHasher, Twox128};
use sp_externalities::{Externalities, ExternalitiesExt};
use sp_std::vec::Vec;

pub mod types;
use types::LedgerApiError;

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
	base_crypto_local::{hash::HashOutput, time::Timestamp},
	coin_structure_local::coin::Commitment,
	coin_structure_local::coin::Nonce,
	coin_structure_local::coin::UnshieldedTokenType,
	ledger_storage_local::{
		Storage,
		arena::{ArenaKey, Sp, TypedArenaKey},
		db::{DB, ParityDb},
		storage::{Map, default_storage, set_default_storage},
	},
	midnight_primitives_ledger::{LedgerMetricsExt, LedgerStorageExt},
	mn_ledger_local::{
		dust::{DustPublicKey, InitialNonce},
		semantics::TransactionContext,
		structure::{
			CNightGeneratesDustActionType, CNightGeneratesDustEvent, ClaimKind, ContractAction,
			ContractCall, MaintenanceUpdate, ProofMarker, SignatureKind, SingleUpdate,
			Transaction as LedgerTransaction,
		},
	},
	onchain_runtime_local::cost_model::CostModel,
	std::time::Instant,
	transient_crypto_local::{curve::Fr, proofs::Proof as BaseProof},
	zswap_local::Offer,
};

use crate::common::types::{
	BlockContext, ContractCallsDetails, FallibleCoinsDetails, GasCost, GuaranteedCoinsDetails,
	Hash, Op, StorageCost, SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot,
	TransactionDetails, TransactionValidationWasCached, Tx, WrappedHash,
};

#[cfg(feature = "std")]
use {lazy_static::lazy_static, moka::sync::Cache};

pub const LOG_TARGET: &str = "midnight::ledger_v2";
pub const MINT_COINS_DOMAIN_SEPARATOR: &[u8; 10] = b"mint_coins";

#[cfg(feature = "std")]
lazy_static! {
	static ref TX_VALIDATION_CACHE: Cache<Hash, Result<(), LedgerApiError>> = Cache::new(1000);
}

#[cfg(feature = "std")]
pub struct Bridge<S: SignatureKind<D>, D: DB> {
	_phantom: core::marker::PhantomData<(S, D)>,
}

#[cfg(feature = "std")]
impl<S: SignatureKind<D> + std::fmt::Debug, D: DB> Bridge<S, D>
where
	mn_ledger_local::structure::Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Tagged,
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
		default_storage::<D>().with_backend(|backend| backend.pre_fetch(&key, None, true));
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
		let api = api::new();
		let ledger = Self::get_ledger(&api, state_key)?;

		let ledger = Ledger::post_block_update(ledger, block_context).map_err(|e| {
			log::error!(
				target: LOG_TARGET,
				"Post Block Update error: {e:?}"
			);
			LedgerApiError::NoLedgerState
		})?;

		let state_root = api.tagged_serialize(&ledger.hash())?;

		// Only update state after no errors
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
	) -> Result<TransactionAppliedStateRoot, LedgerApiError> {
		// Gather metrics for Prometheus
		let start_tx_processing_time = Instant::now();
		let tx_size = tx_serialized.len();

		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		log::info!(
			target: LOG_TARGET,
			"⚙️  Processing Tx {tx:?}"
		);
		let tx_hash = tx.hash();
		let ledger = Self::get_ledger(&api, state_key)?;
		let initial_utxos_size = ledger.state.utxo.utxos.size();

		let tx_ctx = ledger.get_transaction_context(block_context.clone());
		let (ledger, applied_stage) = Ledger::apply_transaction(ledger, &api, &tx, &tx_ctx)?;

		let all_applied = matches!(applied_stage, TransactionAppliedStage::AllApplied);

		let mut utxos = tx.unshielded_utxos();

		if let TransactionAppliedStage::PartialSuccess(segments) = applied_stage {
			// Remove from `utxos` the `segments` that failed
			utxos.remove_failed_segments(&segments);
		}

		let (utxo_outputs, utxo_inputs) =
			utxos.check_utxos_response_integrity(initial_utxos_size, &ledger)?;

		let mut event = TransactionAppliedStateRoot {
			state_root: api.tagged_serialize(&ledger.hash())?,
			tx_hash,
			all_applied,
			call_addresses: vec![],
			deploy_addresses: vec![],
			maintain_addresses: vec![],
			claim_rewards: vec![],
			unshielded_utxos_created: utxo_outputs,
			unshielded_utxos_spent: utxo_inputs,
		};

		for op in tx.calls_and_deploys() {
			match op {
				TransactionOperation::Call { address, .. } => {
					event.call_addresses.push(api.tagged_serialize(&address)?);
				},
				TransactionOperation::Deploy { address } => {
					event.deploy_addresses.push(api.tagged_serialize(&address)?);
				},
				TransactionOperation::Maintain { address } => {
					event.maintain_addresses.push(api.tagged_serialize(&address)?);
				},
				TransactionOperation::ClaimRewards { value, .. } => {
					event.claim_rewards.push(value);
				},
			}
		}

		// Only update state after no errors
		ledger.persist();

		// Write Prometheus metrics
		let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
		if let Some(metrics) = maybe_metrics {
			let tx_type = Self::get_tx_type(&tx);
			let elapsed_time = start_tx_processing_time.elapsed().as_secs_f64();

			metrics.observe_txs_processing_time(elapsed_time, tx_type);
			metrics.observe_txs_size(tx_size as f64, tx_type);
		}

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

		let ledger =
			Ledger::apply_system_tx(ledger, &tx, Timestamp::from_secs(block_context.tblock))?;

		let event = SystemTransactionAppliedStateRoot {
			state_root: api.tagged_serialize(&ledger.hash())?,
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
	) -> Result<(Hash, TransactionDetails), LedgerApiError> {
		// Gather metrics for Prometheus
		let start_tx_validation_time = Instant::now();

		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(tx_serialized)?;
		let ledger = Self::get_ledger(&api, state_key)?;

		let wrapped_cache_key = Self::tx_validation_cache_key(runtime_version, tx_serialized);

		let was_cached =
			Self::do_validate_transaction(&ledger, &tx, &block_context, &wrapped_cache_key)?;

		let tx_details = Self::get_transaction_details(&tx, &ledger)?;

		// We only want to record the metric once
		if let TransactionValidationWasCached::No = was_cached {
			// Write Prometheus metrics
			let maybe_metrics = externalities.extension::<LedgerMetricsExt>();
			if let Some(metrics) = maybe_metrics {
				let tx_type = Self::get_tx_type(&tx);
				let elapsed_time = start_tx_validation_time.elapsed().as_secs_f64();

				metrics.observe_txs_validating_time(elapsed_time, tx_type);
			}
		}

		Ok((wrapped_cache_key.0, tx_details))
	}

	pub fn get_decoded_transaction(transaction_bytes: &[u8]) -> Result<Tx, LedgerApiError> {
		let api = api::new();
		let tx = api.tagged_deserialize::<Transaction<S, D>>(transaction_bytes)?;
		let hash = tx.hash();
		let operations = tx.calls_and_deploys().try_fold(Vec::new(), |mut acc, cd| {
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

	pub fn mint_coins(
		state_key: &[u8],
		amount: u128,
		receiver: &[u8],
		block_context: BlockContext,
	) -> Result<Vec<u8>, LedgerApiError> {
		let api = api::new();
		let target_address = api.night_address(receiver)?;

		let nonce = create_nonce(MINT_COINS_DOMAIN_SEPARATOR, &block_context.parent_block_hash, 0);

		let sys_tx = api::SystemTransaction::PayFromTreasuryUnshielded {
			outputs: vec![api::OutputInstructionUnshielded { amount, target_address, nonce }],
			token_type: UnshieldedTokenType(HashOutput([0u8; 32])), // TODO: UnshieldedTokenType::Reward,
		};
		let ledger = Self::get_ledger(&api, state_key)?;
		let ledger =
			Ledger::apply_system_tx(ledger, &sys_tx, Timestamp::from_secs(block_context.tblock))?;

		// Only update state after no errors
		ledger.persist();
		api.tagged_serialize(&ledger.hash())
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
	) -> Result<(StorageCost, GasCost), LedgerApiError> {
		Ok((0, 0))
	}

	// TODO COST MODEL: Needs to be redone with the new ledger cost model
	#[allow(unused_variables)]
	fn get_contract_call_gas_cost(
		ledger: &Ledger<D>,
		indicies: &Map<Commitment, u64>,
		tx_ctx: &TransactionContext<D>,
		guaranteed: Option<Option<&Offer<BaseProof, D>>>,
		cost_model: &CostModel,
		total_gas: u64,
		call: &ContractCall<ProofMarker, D>,
	) -> Result<GasCost, LedgerApiError> {
		Ok(0)
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
		ledger: &Ledger<D>,
	) -> Result<TransactionDetails, LedgerApiError> {
		let ledger_tx = &tx.0;
		// Indicies do not affect to cost calculation
		let indicies = Map::new();
		// `BlockContext` does not affect to cost calculation
		let block_context = BlockContext::default();
		let tx_ctx = ledger.get_transaction_context(block_context.clone());
		let ledger_parameters = Self::get_deserialized_ledger_parameters(ledger);
		let cost_model = ledger_parameters.cost_model.runtime_cost_model;

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

				let guaranteed = None;

				let mut total_gas = 0;

				let contract_calls = tx.actions().try_fold(
					ContractCallsDetails::default(),
					|mut cd, (_segment, action)| {
						match action {
							ContractAction::Call(call) => {
								cd.inc_calls();

								total_gas = Self::get_contract_call_gas_cost(
									ledger,
									&indicies,
									&tx_ctx,
									guaranteed,
									&cost_model,
									total_gas,
									&call,
								)
								.unwrap_or(0); // For now we set `gas_cost` to `0` in case of failure

								cd.set_gas_cost(total_gas);
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

	fn do_validate_transaction(
		ledger: &Ledger<D>,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
		tx_hash: &WrappedHash,
	) -> Result<TransactionValidationWasCached, LedgerApiError> {
		// We always revalidate the transaction, whether it's in the cache or not.
		let validation = ledger.validate_transaction(tx, block_context);

		// Caching remains helpful as it prevent us from recording validation metrics multiple times
		// Tx is cached: map `Ok` to `TransactionValidationWasCached::Yes`
		if TX_VALIDATION_CACHE.get(&tx_hash.0).is_some() {
			validation.map(|_| TransactionValidationWasCached::Yes)
		// Tx is not cached: insert the validation and map `Ok` to `TransactionValidationWasCached::No` afterwards
		} else {
			TX_VALIDATION_CACHE.insert(tx_hash.0, validation.clone());
			validation.map(|_| TransactionValidationWasCached::No)
		}
	}

	pub fn construct_cnight_generates_dust_event(
		value: u128,
		owner: &[u8],
		time: u64,
		action: u8,
		nonce: [u8; 32],
	) -> Result<Vec<u8>, LedgerApiError> {
		let event = CNightGeneratesDustEvent {
			value,
			owner: DustPublicKey(Fr::from_le_bytes(owner).ok_or(
				LedgerApiError::Deserialization(api::DeserializationError::DustPublicKey),
			)?),
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
		let api = api::new();
		api.tagged_serialize(&event)
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
fn create_nonce(separator: &[u8], block_hash: &[u8], output_number: u8) -> Nonce {
	use sp_runtime::traits::{BlakeTwo256, Hash};

	let concatenated = [block_hash, separator, &[output_number]].concat();

	let h256 = BlakeTwo256::hash(&concatenated);

	Nonce(HashOutput(h256.0))
}

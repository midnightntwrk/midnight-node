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
	base_crypto_local, coin_structure_local, ledger_storage_local, midnight_serialize_local,
	mn_ledger_local, onchain_runtime_local, transient_crypto_local, zswap_local,
};

#[cfg(feature = "std")]
use midnight_serialize::Tagged;
#[cfg(feature = "std")]
use rand::{Rng, SeedableRng, rngs::StdRng};
#[cfg(feature = "std")]
use transient_crypto::commitment::PureGeneratorPedersen;

use frame_support::{StorageHasher, Twox128};
use sp_externalities::{Externalities, ExternalitiesExt};
use sp_std::vec::Vec;

pub mod types;
use types::{LedgerApiError, TransactionError};

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

#[cfg(feature = "std")]
lazy_static! {
	static ref TX_VALIDATION_CACHE: Cache<Hash, Result<(), LedgerApiError>> = Cache::new(1000);
}

// Benchmarks imports
#[cfg(feature = "runtime-benchmarks")]
use crate::common::types::{
	BenchmarkClaimMintTxBuilder, BenchmarkComponents, BenchmarkStandardTxBuilder,
};

#[cfg(all(feature = "std", feature = "runtime-benchmarks"))]
use {
	midnight_node_ledger_helpers::{ContractKind, StdRng, TokenType, WalletState},
	mn_ledger_local::structure::{ClaimRewardsTransaction, LedgerState},
};

#[cfg(feature = "runtime-benchmarks")]
const LOG_TARGET_BENCHMARKS: &str = "midnight::benchmarks";

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
		let valid_tx =
			tx.0.well_formed(
				&tx_ctx.ref_state,
				mn_ledger::verify::WellFormedStrictness::default(),
				tx_ctx.block_context.tblock,
			)
			.map_err(|e| LedgerApiError::Transaction(TransactionError::Malformed(e.into())))?;
		let (ledger, applied_stage) = Ledger::apply_transaction(ledger, &api, &valid_tx, &tx_ctx)?;

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
		let addr = api.tagged_deserialize::<ContractAddress>(contract_address)?;
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
		let addr = api.tagged_deserialize::<ContractAddress>(contract_address)?;
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
		let mut rng = StdRng::seed_from_u64(0x42);
		let sys_tx = api::SystemTransaction::PayFromTreasuryUnshielded {
			outputs: vec![api::OutputInstructionUnshielded {
				amount,
				target_address,
				nonce: rng.r#gen(),
			}],
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

	#[cfg(feature = "runtime-benchmarks")]
	fn do_build_transaction(
		api: &api::Api,
		runtime: &tokio::runtime::Runtime,
		args: &BenchmarkStandardTxBuilder,
		ledger_state: &LedgerState<D>,
		rng: &mut StdRng,
		funding_wallet_state: &mut WalletState<D>,
		token: &TokenType,
		alt_token: &TokenType,
		prev_contract_addrs: Option<Vec<ContractAddress>>,
		contract_type: &ContractType,
		setup: bool,
	) -> Result<
		(Vec<u8>, WalletState<D>, LedgerState<D>, Option<Vec<ContractAddress>>),
		LedgerApiError,
	> {
		use midnight_node_ledger_helpers::*;

		let prev_contract_addrs = prev_contract_addrs.unwrap_or_default();

		let BenchmarkComponents {
			num_guaranteed_inputs,
			num_guaranteed_outputs,
			num_guaranteed_transients,
			num_fallible_inputs,
			num_fallible_outputs,
			num_fallible_transients,
			num_contracts_deploy,
			num_contract_replace_auth,
			num_contract_key_remove,
			num_contract_key_insert,
			num_contract_operations,
		} = args.benchmark_components;

		log::info!(
			target: LOG_TARGET_BENCHMARKS,
			"\n
            Setup: {:?}\n
            num_guaranteed_inputs {:?}
            num_guaranteed_outputs {:?}
            num_guaranteed_transients {:?}
            num_fallible_inputs {:?}
            num_fallible_outputs {:?}
            num_fallible_transients {:?}
            num_contracts_deploy {:?}
            num_contract_replace_auth {:?}
            num_contract_key_remove {:?}
            num_contract_key_insert {:?}
            num_contract_operations {:?}
            \n",
			setup,
			num_guaranteed_inputs,
			num_guaranteed_outputs,
			num_guaranteed_transients,
			num_fallible_inputs,
			num_fallible_outputs,
			num_fallible_transients,
			num_contracts_deploy,
			num_contract_replace_auth,
			num_contract_key_remove,
			num_contract_key_insert,
			num_contract_operations,
		);

		log::info!("\n\nGUARANTEED  starts\n\n");

		let zswap_contract_addrs = ZswapContractAddresses { outputs: None, transients: None };

		// Guaranteed Offer creation
		let guaranteed_offer = build_offer(
			num_guaranteed_inputs,
			num_guaranteed_outputs,
			num_guaranteed_transients,
			// 0,
			funding_wallet_state,
			args.mint_amount,
			args.fee_per_tx,
			token,
			alt_token,
			zswap_contract_addrs.clone(),
			rng,
			setup,
		);

		*funding_wallet_state = funding_wallet_state.apply(&guaranteed_offer);

		log::info!("\n\nGuaranteed Offer: {:?}\n\n", guaranteed_offer);
		log::info!("\n\nZSWAP STATE AFTER GUARANTEED {:?}\n\n", funding_wallet_state);

		log::info!("\n\nFALLIBLE  starts\n\n");

		// Fallible Offer creation
		let fallible_offer = build_offer(
			num_fallible_inputs,
			num_fallible_outputs,
			num_fallible_transients,
			funding_wallet_state,
			args.mint_amount,
			args.fee_per_tx,
			token,
			alt_token,
			zswap_contract_addrs,
			rng,
			setup,
		);

		*funding_wallet_state = funding_wallet_state.apply(&fallible_offer);

		log::info!("\n\nFalllible Offer: {:?}\n\n", fallible_offer);
		log::info!("\n\nZSWAP STATE AFTER FALLIBLE {:?}\n\n", funding_wallet_state);

		// Contract deployment actions creation
		let maybe_contracts_deploy = {
			// Deploy a contract during setup if there are maintain updates, or contract calls
			let potential_contract_deploys = [
				num_contract_replace_auth,
				num_contract_key_remove,
				num_contract_key_insert,
				num_contract_operations,
			];
			let max_deploys =
				*potential_contract_deploys.iter().max().expect("The array is not empty; qed");

			if setup && max_deploys > 0 {
				Some(build_contracts(max_deploys, rng, contract_type))
			} else {
				// If it is not setup, deploy as many as requested
				(!setup).then(|| build_contracts(num_contracts_deploy, rng, contract_type))
			}
		};

		// Contract maintenance actions creation
		let maybe_maintenance_updates = (!setup).then(|| {
			let mut builder = MaintenanceUpdateBuilder::new(
				num_contract_replace_auth,
				num_contract_key_remove,
				num_contract_key_insert,
			);
			builder.add_addresses(&prev_contract_addrs, vec![0; prev_contract_addrs.len()]);
			build_maintenance_updates(&mut builder, contract_type)
		});

		// Contract calls creation
		let maybe_contract_operations = (!setup).then(|| {
			build_contract_call_operations(
				num_contract_operations,
				rng,
				&ledger_state,
				&prev_contract_addrs,
				&contract_type,
			)
		});

		// Gather all contract call actions and apply to `ContractCalls`
		let maybe_contract_calls = {
			let mut contract_actions = Vec::new();

			if let Some(contracts_deploy) = maybe_contracts_deploy {
				contract_actions.extend(contracts_deploy);
			}

			if let Some(maintenance_updates) = maybe_maintenance_updates {
				contract_actions.extend(maintenance_updates);
			}

			if contract_actions.is_empty() && maybe_contract_operations.is_none() {
				None
			} else {
				let mut contract_calls = ContractCalls::new(rng);
				contract_calls.calls = contract_actions;

				// Add the contract call operations if any
				if let Some(contract_operations) = maybe_contract_operations {
					contract_calls = contract_operations
						.into_iter()
						.fold(contract_calls.clone(), |contract_calls, call| {
							contract_calls.add_call::<ProofPreimage>(call)
						});
				}

				Some(contract_calls)
			}
		};

		// Transaction creation
		let unproven_tx = Transaction::new(
			guaranteed_offer.clone(),
			Some(fallible_offer.clone()),
			maybe_contract_calls.clone(),
		);

		// Deserialized proven tx
		let proven_tx =
			runtime.block_on(prove_tx(unproven_tx, rng.clone(), Some(contract_type.clone())));

		// Serialize proven tx
		let tx = api.serialize::<Transaction<ProofMarker, D>>(&proven_tx)?;

		// Update state and collect contract deployed addresses to pass over only during the setup phase
		// (we do not want to waste time for nothing)
		let new_ledger_state: LedgerState<D>;
		let mut contract_addrs: Option<Vec<ContractAddress>> = None;
		if setup {
			new_ledger_state = ledger_state.assert_apply(&proven_tx);

			let contract_calls =
				maybe_contract_calls.clone().map(|cc| cc.calls).unwrap_or_default();

			let ca = get_contract_addresses_from_actions(&contract_calls);

			if !ca.is_empty() {
				contract_addrs = Some(ca);
			}
		} else {
			new_ledger_state = ledger_state.clone();
		}

		Ok((tx, funding_wallet_state.clone(), new_ledger_state, contract_addrs))
	}

	/// Method to build custom Standard transactions to be benchmarked
	#[cfg(feature = "runtime-benchmarks")]
	pub fn build_standard_transactions(
		ledger_state_key: &[u8],
		args: BenchmarkStandardTxBuilder,
	) -> Result<(Vec<u8>, Vec<u8>), LedgerApiError> {
		use midnight_node_ledger_helpers::*;

		log::info!(target: LOG_TARGET_BENCHMARKS, "\n\nSTART BUILDING TXS\n");

		let contract_kind = ContractKind::MerkleTree;

		let api = api::new();
		let args_clone = args.clone();

		let ledger_state = Self::get_ledger(&api, ledger_state_key)?;

		// An async runtime is needed to block on the async method of proving a tx
		let runtime = tokio::runtime::Runtime::new().expect("Async runtime should be initialized");

		// Source of randomness
		let genesis_seed: [u8; 32] =
			args.genesis_seed.try_into().expect("Genesis seed should have 32 bytes");
		let mut rng = StdRng::from_seed(WalletSeed::from(genesis_seed).into());

		// The wallet to hold all the funding to be potentially spent in the benchmarks
		let wallet_seed: [u8; 32] =
			args.wallet_seed.try_into().expect("Wallet seed should have 32 bytes");
		let mut funding_wallet_state =
			create_wallet(WalletSeed::from(wallet_seed), 0, None, WalletKind::NoLegacy);

		// Create as many `Outputs` as `Inputs` are expected in the tx
		// A coin can not be spent unless it has been previously minted
		// We make sure the wallet will have enough credit to spend adding an extra amount to pay for fees
		let token = token_type_decode(&hex::encode(args.token));
		let alt_token = token_type_decode(&hex::encode(args.alt_token));

		// No contracts deployed prior setup tx
		let prev_contract_addrs = None;

		let (setup_tx, mut new_funding_wallet_state, new_ledger_state, new_contract_addresses) =
			Self::do_build_transaction(
				&api,
				&runtime,
				&args_clone,
				&ledger_state.0,
				&mut rng,
				&mut funding_wallet_state,
				&token,
				&alt_token,
				prev_contract_addrs,
				&contract_type,
				true,
			)?;

		let (tx, _, _, _) = Self::do_build_transaction(
			&api,
			&runtime,
			&args_clone,
			&new_ledger_state,
			&mut rng,
			&mut new_funding_wallet_state,
			&token,
			&alt_token,
			new_contract_addresses,
			&contract_type,
			false,
		)?;

		log::info!(target: LOG_TARGET_BENCHMARKS, "\n\nFINISH BUILDING TXS\n");

		Ok((setup_tx, tx))
	}

	/// Method to build custom Standard transactions to be benchmarked
	#[cfg(feature = "runtime-benchmarks")]
	pub fn build_claim_mint_transactions(
		ledger_state_key: &[u8],
		args: BenchmarkClaimMintTxBuilder,
	) -> Result<Vec<u8>, LedgerApiError> {
		use midnight_node_ledger_helpers::*;

		let api = api::new();

		let ledger_state = Self::get_ledger(&api, ledger_state_key)?;

		let wallet_seed: [u8; 32] =
			args.wallet_seed.try_into().expect("Genesis seed should have 32 bytes");

		let wallet_state = create_wallet::<S, D>(WalletSeed::from(wallet_seed));

		let mint_cost = ledger_state.0.parameters.cost_model.mint_cost as u128;
		let amount = args.claim_amount - mint_cost;

		let token = token_type_decode(&hex::encode(args.token));
		let mut rng = StdRng::from_seed(WalletSeed::from(wallet_seed).into());

		let coin_info = CoinInfo::new(&mut rng, amount, token);
		let unproven_tx = Transaction::ClaimRewards(ClaimRewardsTransaction::from(
			wallet_state.authorize_mint(&mut rng, coin_info).unwrap(),
		));

		// An async runtime is needed to block on the async method of proving a tx
		let runtime = tokio::runtime::Runtime::new().expect("Async runtime should be initialized");

		// Deserialized proven tx
		let proven_tx = runtime.block_on(prove_tx(unproven_tx, rng.clone(), None));

		// Serialize proven tx
		let tx = api.serialize::<Transaction<ProofMarker, D>>(&proven_tx)?;

		Ok(tx)
	}

	/// Method to execute a contract call for the purpose of benchmarking its execution time in relation to its `gas_cost`
	/// Only works for infallible contract calls
	#[cfg(feature = "runtime-benchmarks")]
	pub fn execute_contract_call(ledger_state_key: &[u8], tx: &[u8]) -> Result<(), LedgerApiError> {
		let api = api::new();
		let state = Self::get_ledger(&api, &ledger_state_key)?;
		let tx = api.deserialize::<Transaction<S, D>>(tx)?.0;
		let context = TransactionContext::<S, D>::default();

		let panic_msg = "Tx does not include a valid Contract Call";

		match tx {
			LedgerTransaction::Standard(StandardTransaction {
				guaranteed_coins,
				contract_calls,
				..
			}) => {
				let (zswap2, indicies) =
					state.0.zswap.try_apply(&guaranteed_coins, context.whitelist.clone()).unwrap();
				let mut new_state = state.0.clone();
				new_state.zswap = zswap2;

				match contract_calls {
					Some(calls) => {
						// Only one call is expected and executed
						let call_action = calls.calls.first().expect(panic_msg);

						match call_action {
							ContractAction::Call(call) => {
								if let Some(cstate) = new_state.index(call.address) {
									let mut qcontext = QueryContext::new(cstate.data, call.address);
									qcontext.com_indicies = indicies.clone();
									qcontext.block = context.block_context.clone();
									let transcript = call.guaranteed_transcript.as_ref();

									if let Some(transcript) = transcript {
										let parameters = state.0.parameters;
										let qcontext2 = qcontext
											.run_transcript(
												transcript,
												&parameters.cost_model.runtime_cost_model,
											)
											.unwrap();
										if qcontext2.effects != transcript.effects {
											panic!("{}", panic_msg);
										}
										Ok(())
									} else {
										panic!("{}", panic_msg);
									}
								} else {
									panic!("{}", panic_msg);
								}
							},
							_ => {
								panic!("{}", panic_msg);
							},
						}
					},
					_ => {
						panic!("{}", panic_msg);
					},
				}
			},
			_ => {
				panic!("{}", panic_msg);
			},
		}
	}

	/// Method to calculate deserialization times
	#[cfg(feature = "runtime-benchmarks")]
	pub fn deserialize_transaction(transaction_bytes: &[u8]) -> Result<(), LedgerApiError> {
		let api = api::new();
		let _tx = api.deserialize::<Transaction<S, D>>(transaction_bytes)?;
		Ok(())
	}
}

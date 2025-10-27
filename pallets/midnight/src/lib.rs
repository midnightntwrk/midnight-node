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

#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/reference/frame-pallets/>
// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

pub mod weights;

mod runtime_api;
pub use runtime_api::*;

pub use midnight_primitives::{
	LedgerMutFn, LedgerStateProviderMut, TransactionType, TransactionTypeV2,
};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod migrations;

#[frame_support::pallet]
pub mod pallet {
	use crate::weights::WeightInfo;
	use frame_support::{
		pallet_prelude::*, sp_runtime::traits::UniqueSaturatedInto,
		weights::constants::WEIGHT_REF_TIME_PER_SECOND,
	};
	use frame_system::pallet_prelude::*;
	use midnight_primitives::LedgerBlockContextProvider;
	use scale_info::prelude::{string::String, vec::Vec};

	use midnight_node_ledger::types::{
		self as LedgerTypes, GasCost, StorageCost, Tx as LedgerTx, UtxoInfo,
		active_ledger_bridge as LedgerApi,
		active_version::{
			DeserializationError, LedgerApiError, SerializationError, TransactionError,
		},
	};

	impl<T: Config> super::LedgerStateProviderMut for Pallet<T> {
		fn get_ledger_state_key() -> Vec<u8> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			state_key.into()
		}

		fn mut_ledger_state<F, E, R>(f: F) -> Result<R, E>
		where
			F: FnOnce(Vec<u8>) -> Result<(Vec<u8>, R), E>,
		{
			let state_key = StateKey::<T>::get().expect("Failed to get state key");

			let (new_state_key, custom_result) = f(state_key.into())?;

			let new_state_key: BoundedVec<_, _> =
				new_state_key.to_vec().try_into().expect("State key size out of boundaries");
			StateKey::<T>::put(new_state_key.clone());

			Ok(custom_result)
		}
	}

	impl<T: Config> LedgerBlockContextProvider for Pallet<T> {
		fn get_block_context() -> LedgerTypes::BlockContext {
			let parent_hash = <frame_system::Pallet<T>>::parent_hash();
			let now_ms = <pallet_timestamp::Pallet<T>>::get();
			let now_s = now_ms / <T as pallet_timestamp::Config>::Moment::from(1_000u32);
			let drift_s = 30; // (from private const MAX_TIMESTAMP_DRIFT_MILLIS in substrate/frame/timestamp/src/lib.rs)

			LedgerTypes::BlockContext {
				tblock: now_s.unique_saturated_into(),
				tblock_err: drift_s as u32,
				parent_block_hash: parent_hash.as_ref().to_vec(),
			}
		}
	}

	#[cfg(not(hardfork_test))]
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[cfg(hardfork_test)]
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(100);

	pub const FIXED_MN_TRANSACTION_WEIGHT: Weight =
		Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND / 1000, 0);
	pub const EXTRA_WEIGHT_PER_CONTRACT_CALL: Weight = Weight::from_parts(0, 0);
	pub const EXTRA_WEIGHT_TX_SIZE: Weight = Weight::from_parts(0, 0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub network_id: String,
		pub genesis_state_key: Vec<u8>,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Pallet::<T>::initialize_state(&self.network_id, &self.genesis_state_key);
		}
	}

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config {
		/// Information on runtime weights.
		type WeightInfo: WeightInfo;

		/// Block reward getter.
		type BlockReward: Get<(u128, Option<LedgerTypes::Hash>)>;

		#[pallet::constant]
		type SlotDuration: Get<<Self as pallet_timestamp::Config>::Moment>;
	}

	// The pallet's runtime storage items.
	// https://docs.substrate.io/main-docs/build/runtime-storage/
	pub type StateKeyLength = ConstU32<128>;
	type MaxNetworkIdLength = ConstU32<64>;
	#[pallet::storage]
	#[pallet::getter(fn state_key)]
	// Learn more about declaring storage items:
	// https://docs.substrate.io/main-docs/build/runtime-storage/#declaring-storage-items
	pub type StateKey<T> = StorageValue<_, BoundedVec<u8, StateKeyLength>>;

	#[pallet::storage]
	pub type NetworkId<T> = StorageValue<_, BoundedVec<u8, MaxNetworkIdLength>>;

	#[pallet::storage]
	pub type DParameterOverride<T: Config> = StorageValue<_, (u16, u16), OptionQuery>;

	#[pallet::type_value]
	pub fn DefaultWeight() -> Weight {
		FIXED_MN_TRANSACTION_WEIGHT
	}

	#[pallet::type_value]
	pub fn DefaultContractCallWeight() -> Weight {
		EXTRA_WEIGHT_PER_CONTRACT_CALL
	}

	#[pallet::type_value]
	pub fn DefaultTransactionSizeWeight() -> Weight {
		EXTRA_WEIGHT_TX_SIZE
	}

	#[pallet::type_value]
	pub fn DefaultSafeMode() -> bool {
		true
	}

	#[pallet::type_value]
	pub fn DefaultMaxSkippedSlots() -> u8 {
		1
	}

	#[pallet::storage]
	#[pallet::getter(fn configurable_weight)]
	pub type ConfigurableWeight<T> = StorageValue<_, Weight, ValueQuery, DefaultWeight>;

	#[pallet::storage]
	#[pallet::getter(fn configurable_contract_call_weight)]
	pub type ConfigurableContractCallWeight<T> =
		StorageValue<_, Weight, ValueQuery, DefaultContractCallWeight>;

	#[pallet::storage]
	#[pallet::getter(fn configurable_transaction_size_weight)]
	pub type ConfigurableTransactionSizeWeight<T> =
		StorageValue<_, Weight, ValueQuery, DefaultTransactionSizeWeight>;

	#[pallet::storage]
	pub type SafeMode<T> = StorageValue<_, bool, ValueQuery, DefaultSafeMode>;

	#[pallet::storage]
	pub type MaxSkippedSlots<T> = StorageValue<_, u8, ValueQuery, DefaultMaxSkippedSlots>;

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct TxAppliedDetails {
		pub tx_hash: LedgerTypes::Hash,
	}

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct MaintainDetails {
		pub tx_hash: LedgerTypes::Hash,
		pub contract_address: Vec<u8>,
	}

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct DeploymentDetails {
		pub tx_hash: LedgerTypes::Hash,
		pub contract_address: Vec<u8>,
	}

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct CallDetails {
		pub tx_hash: LedgerTypes::Hash,
		pub contract_address: Vec<u8>,
	}

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct ClaimRewardsDetails {
		pub tx_hash: LedgerTypes::Hash,
		pub value: u128,
	}

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct PayoutDetails {
		pub amount: u128,
		pub receiver: Vec<u8>,
	}

	#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct UnshieldedTokensDetails {
		pub spent: Vec<UtxoInfo>,
		pub created: Vec<UtxoInfo>,
	}

	// grcov-excl-start
	// Pallets use events to inform users when important changes are made.
	// https://docs.substrate.io/main-docs/build/events-errors/
	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event {
		/// A contract was called.
		ContractCall(CallDetails),
		/// A contract has been deployed.
		ContractDeploy(DeploymentDetails),
		/// A transaction has been applied (both the guaranteed and conditional part).
		TxApplied(TxAppliedDetails),
		/// Contract ownership changes to enable snark upgrades
		ContractMaintain(MaintainDetails),
		/// New payout minted.
		PayoutMinted(PayoutDetails),
		/// Payout was claimed.
		ClaimRewards(ClaimRewardsDetails),
		/// Unshielded Tokens Trasfers
		UnshieldedTokens(UnshieldedTokensDetails),
		/// Partial Success.
		TxPartialSuccess(TxAppliedDetails),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		#[codec(index = 0)]
		NewStateOutOfBounds,
		#[codec(index = 1)]
		Deserialization(DeserializationError),
		#[codec(index = 2)]
		Serialization(SerializationError),
		#[codec(index = 3)]
		Transaction(TransactionError),
		#[codec(index = 4)]
		LedgerCacheError,
		#[codec(index = 5)]
		NoLedgerState,
		#[codec(index = 6)]
		LedgerStateScaleDecodingError,
		#[codec(index = 7)]
		ContractCallCostError,
		#[codec(index = 8)]
		BlockLimitExceededError,
		#[codec(index = 9)]
		FeeCalculationError,
		#[codec(index = 10)]
		HostApiError,
		#[codec(index = 11)]
		NetworkIdNotString,
	}
	// grcov-excl-stop

	impl<T: Config> From<LedgerApiError> for Error<T> {
		fn from(value: LedgerApiError) -> Self {
			match value {
				LedgerApiError::Deserialization(error) => Error::<T>::Deserialization(error),
				LedgerApiError::Serialization(error) => Error::<T>::Serialization(error),
				LedgerApiError::Transaction(error) => Error::<T>::Transaction(error),
				LedgerApiError::LedgerCacheError => Error::<T>::LedgerCacheError,
				LedgerApiError::NoLedgerState => Error::<T>::NoLedgerState,
				LedgerApiError::LedgerStateScaleDecodingError => {
					Error::<T>::LedgerStateScaleDecodingError
				},
				LedgerApiError::ContractCallCostError => Error::<T>::ContractCallCostError,
				LedgerApiError::BlockLimitExceededError => Error::<T>::BlockLimitExceededError,
				LedgerApiError::FeeCalculationError => Error::<T>::FeeCalculationError,
				LedgerApiError::HostApiError => Error::<T>::HostApiError,
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_block: BlockNumberFor<T>) -> Weight {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");

			LedgerApi::pre_fetch_storage(&state_key).expect("Failed to pre-fetch storage");

			<T as Config>::WeightInfo::on_finalize()
		}

		fn on_finalize(_block: BlockNumberFor<T>) {
			// Post Block Ledger Update
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			let block_context = Self::get_block_context();

			let state_root = LedgerApi::post_block_update(&state_key, block_context.clone())
				.expect("Post block update failed");

			let new_state_key: BoundedVec<_, _> =
				state_root.to_vec().try_into().expect("State key size out of boundaries");
			StateKey::<T>::put(new_state_key);

			// Flush ledger storage changes to disk
			LedgerApi::flush_storage();

			let (reward, beneficiary) = T::BlockReward::get();
			if reward == 0 {
				return;
			}
			if let Some(beneficiary) = beneficiary {
				let state_key = StateKey::<T>::get().expect("Failed to get state key");

				match LedgerApi::mint_coins(&state_key, reward, &beneficiary[..], block_context) {
					Ok(new_state_key) => {
						log::info!("Minting {reward:?} coins for {beneficiary:?}");
						Self::deposit_event(Event::PayoutMinted(PayoutDetails {
							amount: reward,
							receiver: beneficiary.to_vec(),
						}));
						let state_key: BoundedVec<_, _> =
							new_state_key.try_into().expect("New state key size out of boundaries");
						StateKey::<T>::put(state_key);

						LedgerApi::flush_storage();
					},
					Err(e) => log::error!("Unable to mint coins: {e:#?}"),
				};
			}
		}

		#[cfg(hardfork_test)]
		fn on_runtime_upgrade() -> Weight {
			if Self::in_code_storage_version() != Self::on_chain_storage_version() {
				LedgerApi::drop_default_storage();
				LedgerApi::set_default_storage();
			}
			// TODO: Benchmark Weight in case of a real hard-fork
			Weight::zero()
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	//todo example of custom transaction type (extrinsic) transaction has to be signed to call it
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight({
                if SafeMode::<T>::get() {
                    ConfigurableWeight::<T>::get()
                } else {
                    // TODO: Now that we always revalidate txs, we don't want to validate the tx again to calculate the Weight
                    //       Weight calculation and benchmarks should be revisited anyway once new Ledger's Cost Model is finished.
                    //       Deleted code can be checked in: https://github.com/midnightntwrk/midnight-node/pull/1054
                    ConfigurableWeight::<T>::get()
                }
            })]
		pub fn send_mn_transaction(_origin: OriginFor<T>, midnight_tx: Vec<u8>) -> DispatchResult {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			let block_context = Self::get_block_context();
			let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;

			let result = LedgerApi::apply_transaction(
				&state_key,
				&midnight_tx,
				block_context,
				runtime_version,
			)
			.map_err(Error::<T>::from)?;

			let state_key: BoundedVec<_, _> =
				result.state_root.to_vec().try_into().expect("State key size out of boundaries");
			StateKey::<T>::put(state_key);

			let tx_hash = result.tx_hash;
			for address in result.call_addresses {
				let call_event =
					Event::ContractCall(CallDetails { tx_hash, contract_address: address });
				Self::deposit_event(call_event);
			}

			for address in result.deploy_addresses {
				let deploy_event =
					Event::ContractDeploy(DeploymentDetails { tx_hash, contract_address: address });
				Self::deposit_event(deploy_event);
			}

			for address in result.maintain_addresses {
				let maintain_event =
					Event::ContractMaintain(MaintainDetails { tx_hash, contract_address: address });
				Self::deposit_event(maintain_event);
			}

			for value in result.claim_rewards {
				let claim_event = Event::ClaimRewards(ClaimRewardsDetails { tx_hash, value });
				Self::deposit_event(claim_event);
			}

			if !result.unshielded_utxos_created.is_empty()
				|| !result.unshielded_utxos_spent.is_empty()
			{
				Self::deposit_event(Event::UnshieldedTokens(UnshieldedTokensDetails {
					spent: result.unshielded_utxos_spent,
					created: result.unshielded_utxos_created,
				}));
			}

			if result.all_applied {
				Self::deposit_event(Event::TxApplied(TxAppliedDetails { tx_hash }));
			} else {
				Self::deposit_event(Event::TxPartialSuccess(TxAppliedDetails { tx_hash }));
			}

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		// A system transaction for configuring weights - for testing transaction throughput on devnets only.
		pub fn set_mn_tx_weight(origin: OriginFor<T>, new_weight: Weight) -> DispatchResult {
			ensure_root(origin)?;
			ConfigurableWeight::<T>::set(new_weight);
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		pub fn override_d_parameter(
			origin: OriginFor<T>,
			d_parameter_override: Option<(u16, u16)>,
		) -> DispatchResult {
			ensure_root(origin)?;
			DParameterOverride::<T>::set(d_parameter_override);
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		// A system transaction for configuring contract call weights
		pub fn set_contract_call_weight(
			origin: OriginFor<T>,
			new_weight: Weight,
		) -> DispatchResult {
			ensure_root(origin)?;
			ConfigurableContractCallWeight::<T>::set(new_weight);
			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		// A system transaction for configuring contract call weights
		pub fn set_tx_size_weight(origin: OriginFor<T>, new_weight: Weight) -> DispatchResult {
			ensure_root(origin)?;
			ConfigurableTransactionSizeWeight::<T>::set(new_weight);
			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		// A system transaction for configuring safe mode
		pub fn set_safe_mode(origin: OriginFor<T>, mode: bool) -> DispatchResult {
			ensure_root(origin)?;
			SafeMode::<T>::set(mode);
			Ok(())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			let mut block_context = Self::get_block_context();
			let slot_duration: u64 = T::SlotDuration::get().unique_saturated_into();
			let slot_duration_secs = slot_duration.saturating_div(1000);

			// Simulate the expected next block time during validation.
			// This is needed to avoid potential `OutOfDustValidityWindow` tx validation errors where `ctime > tblock`.
			// During transaction pool validation, the stored Timestamp still corresponds to the last produced block.
			// Validity is increased by `slot_duration_secs * MaxSkippedSlots` to prevent the node
			// from rejecting potentially valid transactions if an AURA block production slots are skipped.
			let skipped_slots_margin =
				slot_duration_secs.saturating_mul(MaxSkippedSlots::<T>::get() as u64);
			block_context.tblock = block_context
				.tblock
				.saturating_add(slot_duration_secs)
				.saturating_add(skipped_slots_margin);

			Self::validate_unsigned(call, block_context)
		}

		fn pre_dispatch(call: &Self::Call) -> Result<(), TransactionValidityError> {
			let block_context = Self::get_block_context();

			Self::validate_unsigned(call, block_context).map(|_| ())
		}
	}

	// grcov-excl-start
	impl<T: Config> Pallet<T> {
		pub fn initialize_state(network_id: &str, state_key: &[u8]) {
			//todo add checks
			let genesis_state_key: BoundedVec<_, _> =
				state_key.to_vec().try_into().expect("Genesis state key size out of boundaries");
			StateKey::<T>::put(genesis_state_key);

			let network_id: BoundedVec<_, _> = network_id
				.as_bytes()
				.to_vec()
				.try_into()
				.expect("Network Id size out of boundaries");
			NetworkId::<T>::put(network_id);
		}

		pub fn get_contract_state(contract_address: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			LedgerApi::get_contract_state(&state_key, contract_address)
		}

		pub fn get_decoded_transaction(
			midnight_transaction: &[u8],
		) -> Result<LedgerTx, LedgerApiError> {
			LedgerApi::get_decoded_transaction(midnight_transaction)
		}

		pub fn get_ledger_version() -> Vec<u8> {
			LedgerApi::get_version()
		}

		// grcov-excl-start
		pub fn get_network_id() -> String {
			match <NetworkId<T>>::get() {
				None => String::new(),
				Some(name) => String::from_utf8(name.to_vec()).expect("NetworkId is not a String"),
			}
		}

		pub fn get_zswap_chain_state(contract_address: &[u8]) -> Result<Vec<u8>, LedgerApiError> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			LedgerApi::get_zswap_chain_state(&state_key, contract_address)
		}
		// grcov-excl-stop

		//todo annotate with exclude for non test runs
		fn invalid_transaction(error_code: u8) -> TransactionValidityError {
			TransactionValidityError::Invalid(InvalidTransaction::Custom(error_code))
		}

		fn validate_unsigned(
			call: &Call<T>,
			block_context: LedgerTypes::BlockContext,
		) -> TransactionValidity {
			if let Call::send_mn_transaction { midnight_tx } = call {
				let state_key = StateKey::<T>::get().expect("Failed to get state key");
				let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;

				let (tx_hash, _) = LedgerApi::validate_transaction(
					&state_key,
					midnight_tx,
					block_context,
					runtime_version,
				)
				.map_err(|e| Self::invalid_transaction(e.into()))?;
				ValidTransaction::with_tag_prefix("Midnight")
					// Transactions can live in the pool for max 600 blocks before they must be revalidated
					.longevity(600)
					.and_provides(tx_hash)
					.build()
			} else {
				// grcov-excl-start
				Err(Self::invalid_transaction(Default::default()))
				// grcov-excl-stop
			}
		}

		pub fn get_unclaimed_amount(beneficiary: &[u8]) -> Result<u128, LedgerApiError> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			LedgerApi::get_unclaimed_amount(&state_key, beneficiary)
		}

		pub fn get_ledger_parameters() -> Result<Vec<u8>, LedgerApiError> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			LedgerApi::get_ledger_parameters(&state_key)
		}

		pub fn get_transaction_cost(tx: &[u8]) -> Result<(StorageCost, GasCost), LedgerApiError> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			let block_context = Self::get_block_context();
			LedgerApi::get_transaction_cost(&state_key, tx, block_context)
		}

		pub fn get_zswap_state_root() -> Result<Vec<u8>, LedgerApiError> {
			let state_key = StateKey::<T>::get().expect("Failed to get state key");
			LedgerApi::get_zswap_state_root(&state_key)
		}
	}
}

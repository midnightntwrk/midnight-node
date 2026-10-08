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

//! # Native Token Observation Pallet
//!
//! This pallet provides mechanisms for tracking all registrations for cNIGHT generates DUST from Cardano,
//! as well as observation of all cNIGHT utxos of valid registrants of cNIGHT generates DUST.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use derive_new::new;
use frame_support::{
	dispatch::{Pays, PostDispatchInfo},
	pallet_prelude::*,
};
use frame_system::pallet_prelude::*;
use midnight_primitives_cnight_observation::{
	CardanoPosition, INHERENT_IDENTIFIER, InherentError, ObservationCostModel,
};
use midnight_primitives_mainchain_follower::MidnightObservationTokenMovement;
pub use pallet::*;
use serde::{Deserialize, Serialize};
use sidechain_domain::McBlockHash;

pub mod config;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

/// Cardano-based Midnight System Transaction (CMST)  Header
///
///  * `block`: hash of the last processed Cardano block
///  * `index`: index (zero based) of the next transaction to process in the
///    `block`.  If `index` equals the size of the block, it means that a block has
///    been processed in full.
///
/// See spec for more details:
/// https://github.com/midnightntwrk/midnight-ledger/blob/main/spec/cardano-system-transactions.md#cmst-header
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct CmstHeader {
	/// Hash of the last processed block
	pub block_hash: McBlockHash,
	/// The index of the next transaction to process in the block
	pub tx_index_in_block: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[repr(u8)]
pub enum UtxoActionType {
	Create,
	Destroy,
}

/// Default share of the block's ledger cost limits one observation batch may use.
pub const DEFAULT_OBSERVATION_BUDGET_PERCENT: u32 = 50;

/// Serialized zero field element: a valid dust public key for pricing dummy events.
const ZERO_DUST_PUBLIC_KEY: [u8; 32] = [0; 32];

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{sp_runtime::traits::Hash, storage::with_storage_layer};
	use midnight_primitives::{
		LedgerBlockContextProvider, LedgerStateProvider, MidnightSystemTransactionCNightExecutor,
	};
	use midnight_primitives_cnight_observation::{
		CARDANO_ASSET_NAME_MAX_LENGTH, CARDANO_BECH32_ADDRESS_MAX_LENGTH, CNIGHT_POLICY_ID_LENGTH,
		CardanoRewardAddressBytes, DustPublicKeyBytes,
	};
	use midnight_primitives_mainchain_follower::{
		CreateData, DeregistrationData, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
		RegistrationData, SpendData,
	};
	use scale_info::prelude::vec::Vec;
	use sidechain_domain::UtxoId;
	use sp_core::H256;

	use midnight_node_ledger::types::{
		Hash as LedgerHash, active_ledger_bridge as LedgerApi,
		active_version::{
			DeserializationError, LedgerApiError, SerializationError, TransactionError,
		},
	};

	use crate::config::CNightGenesis;
	use crate::weights::WeightInfo;

	use super::*;

	struct CNightGeneratesDustEventSerialized(Vec<u8>);

	pub type BoundedCardanoAddress = BoundedVec<u8, ConstU32<CARDANO_BECH32_ADDRESS_MAX_LENGTH>>;

	#[derive(
		Debug,
		Clone,
		PartialEq,
		Eq,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		Serialize,
		Deserialize,
	)]
	pub struct MappingEntry {
		pub cardano_reward_address: CardanoRewardAddressBytes,
		pub dust_public_key: DustPublicKeyBytes,
		pub utxo_id: UtxoId,
	}

	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug, PartialEq, new)]
	pub struct Registration {
		pub cardano_reward_address: CardanoRewardAddressBytes,
		pub dust_public_key: DustPublicKeyBytes,
	}

	#[derive(Clone, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, PartialEq, new)]
	pub struct Deregistration {
		pub cardano_reward_address: CardanoRewardAddressBytes,
		pub dust_public_key: DustPublicKeyBytes,
	}

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct SystemTransactionApplied {
		pub header: CmstHeader,
		pub system_transaction_hash: LedgerHash,
	}

	// v2: re-apply the cNIGHT dust generation entries the ledger 8 -> 9 hardfork
	// wipes (see `migrations::v2`).
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<Hash = H256> {
		type MidnightSystemTransactionExecutor: MidnightSystemTransactionCNightExecutor;
		/// Reads the ledger state key, to capture the pre-hardfork (ledger-8)
		/// one before the pallet-midnight translation replaces it.
		type LedgerStateProvider: LedgerStateProvider;
		/// Supplies the ledger time stamped on the replayed dust events.
		type LedgerBlockContextProvider: LedgerBlockContextProvider;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: crate::weights::WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		Registration(Registration),
		Deregistration(Deregistration),
		MappingAdded(MappingEntry),
		MappingRemoved(MappingEntry),
		SystemTransactionApplied(SystemTransactionApplied),
		/// The hardfork upgrade block armed the dust generation replay
		/// by saving the pre-fork ledger state key.
		/// It was emitted by the past migration.
		DustReapplyStarted,
		/// One replay batch failed to apply; its nonces were not restored. The
		/// replay continues with the next batch.
		/// It was emitted by the past migration.
		DustReapplyBatchFailed {
			nonces: Vec<T::Hash>,
		},
		/// The replay finished. `applied` entries were restored; `skipped` were
		/// not (untracked, already destroyed, or in a failed batch).
		/// It was emitted by the past migration.
		DustReapplyCompleted {
			applied: u32,
			skipped: u32,
		},
		/// The replay was abandoned before the last page: the hardfork did not
		/// wipe dust state, no pre-fork state key was recorded, that key became
		/// unreadable, or a batch priced above what a whole block affords. The
		/// reason is logged.
		/// It was emitted by the past migration.
		DustReapplySkipped {
			applied: u32,
			skipped: u32,
		},
		/// `process_tokens` ignored this block's Cardano observations because a
		/// multi-block migration of this pallet's storage is still running.
		/// `NextCardanoPosition` is left unchanged, so nothing is lost — the
		/// observer re-delivers these UTXOs once the migration winds up. Emitted
		/// once per block for as long as the gate holds; the exact versions are
		/// in the node log.
		ObservationsSkippedForMigration,
		/// The ledger refused this block's cNIGHT-generates-DUST update as over the
		/// block limit, so the native cost estimate disagrees with the ledger. The batch
		/// was rolled back and `NextCardanoPosition` left unchanged: observation stalls
		/// until a runtime upgrade fixes the estimate.
		ObservationsDeferred,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A Cardano Wallet address was sent, but was longer than expected
		MaxCardanoAddrLengthExceeded,
		/// A Cardano asset name contained non-ASCII bytes
		NonAsciiAssetName,
		/// Only one inherent is allowed per block
		InherentAlreadyExecuted,
		/// Next Cardano position does not advance beyond current position
		CardanoPositionRegression,
		/// `ObservationBudgetPercent` must be at most 100
		BudgetPercentOutOfRange,
		// Ledger errors mirrored from `LedgerApiError`. Flattened (rather than wrapped)
		// so the encoding fits within `MAX_MODULE_ERROR_ENCODED_SIZE`.
		Deserialization(DeserializationError),
		Serialization(SerializationError),
		Transaction(TransactionError),
		LedgerCacheError,
		NoLedgerState,
		LedgerStateScaleDecodingError,
		ContractCallCostError,
		BlockLimitExceededError,
		FeeCalculationError,
		HostApiError,
		GetTransactionContextError,
		ContractNotPresent,
		BeneficiaryNotFound,
	}

	impl<T: Config> From<LedgerApiError> for Error<T> {
		fn from(value: LedgerApiError) -> Self {
			match value {
				LedgerApiError::Deserialization(e) => Error::<T>::Deserialization(e),
				LedgerApiError::Serialization(e) => Error::<T>::Serialization(e),
				LedgerApiError::Transaction(e) => Error::<T>::Transaction(e),
				LedgerApiError::LedgerCacheError => Error::<T>::LedgerCacheError,
				LedgerApiError::NoLedgerState => Error::<T>::NoLedgerState,
				LedgerApiError::LedgerStateScaleDecodingError => {
					Error::<T>::LedgerStateScaleDecodingError
				},
				LedgerApiError::ContractCallCostError => Error::<T>::ContractCallCostError,
				LedgerApiError::BlockLimitExceededError => Error::<T>::BlockLimitExceededError,
				LedgerApiError::FeeCalculationError => Error::<T>::FeeCalculationError,
				LedgerApiError::HostApiError => Error::<T>::HostApiError,
				LedgerApiError::GetTransactionContextError => {
					Error::<T>::GetTransactionContextError
				},
				LedgerApiError::ContractNotPresent => Error::<T>::ContractNotPresent,
				LedgerApiError::BeneficiaryNotFound => Error::<T>::BeneficiaryNotFound,
			}
		}
	}

	#[pallet::storage]
	// Script address for managing registrations on Cardano
	pub type MainChainMappingValidatorAddress<T: Config> =
		StorageValue<_, BoundedCardanoAddress, ValueQuery>;

	#[pallet::storage]
	// Asset name for auth token used in MappingValidator
	pub type MainChainAuthTokenAssetName<T: Config> =
		StorageValue<_, BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>, ValueQuery>;

	/// Individual Cardano -> DUST mappings, keyed by the reward address and the
	/// source UTXO reference. Each UTXO produces exactly one mapping, so
	/// the `UtxoId` is globally unique.
	#[pallet::storage]
	pub type Mapping<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		CardanoRewardAddressBytes,
		Blake2_128Concat,
		UtxoId,
		DustPublicKeyBytes,
		OptionQuery,
	>;

	// TODO: Read from ledger state directly ?
	#[pallet::storage]
	pub type UtxoOwners<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, DustPublicKeyBytes, OptionQuery>;

	#[pallet::storage]
	// The next Cardano position to look for new transactions
	pub type NextCardanoPosition<T: Config> = StorageValue<_, CardanoPosition, ValueQuery>;

	#[pallet::storage]
	// A full identifier for a native asset on Cardano: (policy id, asset name)
	pub type CNightIdentifier<T: Config> = StorageValue<
		_,
		(
			// Policy ID
			BoundedVec<u8, ConstU32<CNIGHT_POLICY_ID_LENGTH>>,
			// Asset Name
			BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>,
		),
		ValueQuery,
	>;

	#[pallet::type_value]
	pub fn DefaultObservationBudgetPercent() -> u32 {
		DEFAULT_OBSERVATION_BUDGET_PERCENT
	}

	/// Share of the block's ledger cost limits one observation batch may consume, in
	/// percent. `CNightObservation` runs before `C2MBridge`, whose system transactions
	/// are silently dropped when the block is already full, so this must leave headroom.
	#[pallet::storage]
	pub type ObservationBudgetPercent<T: Config> =
		StorageValue<_, u32, ValueQuery, DefaultObservationBudgetPercent>;

	#[pallet::storage]
	pub type InherentExecutedThisBlock<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The ledger-8 arena root as of the hardfork upgrade block, retained so the
	/// dust replay (V1 to V2 migration) can read pre-wipe night values and owners
	/// after `pallet_midnight::StateKey` has moved on to the v9 root. Mirrors
	/// that item's shape. Killed when the replay finishes.
	/// It was was by the past migration.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type PreForkStateKey<T: Config> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Ledger time stamped on every replayed dust event: the fork block's own
	/// time, backdated by the dust `time_to_cap` so every restored entry lands
	/// at its DUST cap rather than at zero. Written by the first replay step.
	/// It was emitted by the past migration.
	#[pallet::storage]
	pub type DustReapplyCtime<T: Config> = StorageValue<_, u64, OptionQuery>;

	/// Running (applied, skipped) tallies of the dust replay — the only on-chain
	/// evidence it ran to completion. Killed when the replay finishes.
	/// It was emitted by the past migration.
	#[pallet::storage]
	pub type DustReapplyProgress<T: Config> = StorageValue<_, (u32, u32), ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub config: CNightGenesis,
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Substrate genesis fail-fast convention: build() returns (), so we panic on
			// invalid chain-spec input rather than silently producing incorrect state. Each
			// panic names the chain-spec field path (matching the camelCase JSON keys the
			// operator edits) and reads the cap from the destination BoundedVec type, so a
			// startup-failure log points directly at the offending field.
			MainChainMappingValidatorAddress::<T>::set(
				self.config
					.addresses
					.mapping_validator_address
					.as_bytes()
					.to_vec()
					.try_into()
					.unwrap_or_else(|v: Vec<u8>| {
						panic!(
							"genesis: cNightObservation.config.addresses.mapping_validator_address \
							 length {} bytes exceeds maximum {}",
							v.len(),
							BoundedCardanoAddress::bound(),
						)
					}),
			);

			CNightIdentifier::<T>::set((
				self.config.addresses.cnight_policy_id.to_vec().try_into().unwrap_or_else(
					|v: Vec<u8>| {
						panic!(
							"genesis: cNightObservation.config.addresses.cnight_policy_id \
							 length {} bytes exceeds maximum {}",
							v.len(),
							BoundedVec::<u8, ConstU32<CNIGHT_POLICY_ID_LENGTH>>::bound(),
						)
					},
				),
				self.config
					.addresses
					.cnight_asset_name
					.as_bytes()
					.to_vec()
					.try_into()
					.unwrap_or_else(|v: Vec<u8>| {
						panic!(
							"genesis: cNightObservation.config.addresses.cnight_asset_name \
							 length {} bytes exceeds maximum {}",
							v.len(),
							BoundedVec::<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>::bound(),
						)
					}),
			));

			MainChainAuthTokenAssetName::<T>::set(
				self.config
					.addresses
					.auth_token_asset_name
					.as_bytes()
					.to_vec()
					.try_into()
					.unwrap_or_else(|v: Vec<u8>| {
						panic!(
							"genesis: cNightObservation.config.addresses.auth_token_asset_name \
							 length {} bytes exceeds maximum {}",
							v.len(),
							BoundedVec::<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>::bound(),
						)
					}),
			);

			for (addr, entries) in &self.config.mappings {
				for entry in entries {
					Mapping::<T>::insert(
						addr,
						UtxoId::new(entry.utxo_tx_hash.0, entry.utxo_index),
						entry.dust_public_key.clone(),
					);
				}
			}

			for (k, v) in &self.config.utxo_owners {
				UtxoOwners::<T>::insert(H256(*k), v);
			}

			NextCardanoPosition::<T>::set(self.config.next_cardano_position.clone());
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			match Self::get_data_from_inherent_data(data) {
				Ok(Some(data)) => Some(Call::process_tokens {
					utxos: data.utxos,
					next_cardano_position: data.next_cardano_position,
				}),
				Ok(None) => None,
				Err(e) => {
					log::error!(target: "cnight-observation", "Failed to decode inherent data: {e:?}");
					None
				},
			}
		}

		fn check_inherent(call: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			let Call::process_tokens { utxos, next_cardano_position } = call else {
				return Ok(());
			};

			let parsed = Self::get_data_from_inherent_data(data)?.ok_or(InherentError::Other)?;
			if parsed.utxos != *utxos || parsed.next_cardano_position != *next_cardano_position {
				return Err(InherentError::Other);
			}
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::process_tokens { .. })
		}

		fn is_inherent_required(data: &InherentData) -> Result<Option<Self::Error>, Self::Error> {
			let data = Self::get_data_from_inherent_data(data)?;
			Ok(if data.is_some() { Some(InherentError::Missing) } else { None })
		}
	}

	impl<T: Config> Pallet<T> {
		fn get_data_from_inherent_data(
			data: &InherentData,
		) -> Result<Option<MidnightObservationTokenMovement>, InherentError> {
			data.get_data::<MidnightObservationTokenMovement>(&INHERENT_IDENTIFIER)
				.map_err(|_| InherentError::DecodeFailed)
		}

		pub fn get_registration(wallet: &CardanoRewardAddressBytes) -> Option<DustPublicKeyBytes> {
			Self::unique_dust_key(wallet)
		}

		// Check if any form of a registration could be considered valid as of now
		pub fn is_registered(utxo_holder: &CardanoRewardAddressBytes) -> bool {
			Self::unique_dust_key(utxo_holder).is_some()
		}

		/// Prices one observed UTXO by the pallet's own weight, and a one-`Create` and
		/// a one-`Destroy` cNIGHT-generates-DUST update against the current ledger
		/// parameters. `None` if any price is unavailable.
		pub fn observation_cost_model() -> Option<ObservationCostModel> {
			let full_gas = T::BlockWeights::get().max_block.ref_time();
			let weight = |n: u32| T::WeightInfo::process_tokens(n).ref_time();
			let utxo_gas = weight(1).saturating_sub(weight(0));
			if utxo_gas == 0 {
				log::error!(
					"WeightInfo::process_tokens has no per-UTXO cost; cannot bound the cNIGHT observation batch"
				);
				return None;
			}
			match (
				Self::cnight_generates_dust_update_gas(UtxoActionType::Create),
				Self::cnight_generates_dust_update_gas(UtxoActionType::Destroy),
			) {
				(Ok(create_gas), Ok(destroy_gas)) => Some(ObservationCostModel {
					utxo_gas,
					create_gas,
					destroy_gas,
					budget_gas: full_gas
						.saturating_mul(ObservationBudgetPercent::<T>::get().into())
						/ 100,
					full_gas,
				}),
				(create, destroy) => {
					log::error!(
						"failed to price the cNIGHT-generates-DUST update: create {create:?}, destroy {destroy:?}"
					);
					None
				},
			}
		}

		/// Ledger gas of a cNIGHT-generates-DUST update carrying one `action` event.
		/// The ledger price ignores event content, so a zero-valued dummy event prices
		/// exactly what a real one will.
		fn cnight_generates_dust_update_gas(action: UtxoActionType) -> Result<u64, LedgerApiError> {
			let event = LedgerApi::construct_cnight_generates_dust_event(
				0,
				&ZERO_DUST_PUBLIC_KEY,
				0,
				action as u8,
				[0; 32],
			)?;
			let tx = LedgerApi::construct_cnight_generates_dust_system_tx(alloc::vec![event])?;
			LedgerApi::get_transaction_cost(
				&T::LedgerStateProvider::get_ledger_state_key(),
				&tx,
				T::LedgerBlockContextProvider::get_block_context(),
				T::BlockWeights::get().max_block.ref_time(),
			)
		}

		/// Weight of `process_tokens` over `utxos`: the pallet's own weight plus the
		/// cNIGHT-generates-DUST update's ledger gas, priced by kind as the inherent
		/// data provider admits them. An unpriceable update charges a whole block per
		/// event.
		pub fn process_tokens_weight(utxos: &[ObservedUtxo]) -> Weight {
			let full_gas = T::BlockWeights::get().max_block.ref_time();
			let price = |action| Self::cnight_generates_dust_update_gas(action).unwrap_or(full_gas);
			let count = |f: fn(&ObservedUtxoData) -> bool| {
				utxos.iter().filter(|u| f(&u.data)).count() as u64
			};
			let creates = count(|d| matches!(d, ObservedUtxoData::AssetCreate(_)));
			let spends = count(|d| matches!(d, ObservedUtxoData::AssetSpend(_)));
			// Price lazily: a batch without creates or spends never queries the ledger.
			let mut ledger_gas = 0u64;
			if creates > 0 {
				ledger_gas = creates.saturating_mul(price(UtxoActionType::Create));
			}
			if spends > 0 {
				ledger_gas = ledger_gas
					.saturating_add(spends.saturating_mul(price(UtxoActionType::Destroy)));
			}
			T::WeightInfo::process_tokens(utxos.len() as u32)
				.saturating_add(Weight::from_parts(ledger_gas, 0))
		}

		/// Everything `process_tokens` does once the per-block guards have passed.
		/// Runs inside a storage layer so the caller can roll it back as a whole.
		fn apply_observations(
			utxos: Vec<ObservedUtxo>,
			next_cardano_position: CardanoPosition,
		) -> DispatchResult {
			let prev = NextCardanoPosition::<T>::get();
			ensure!(next_cardano_position >= prev, Error::<T>::CardanoPositionRegression);

			let mut events: Vec<CNightGeneratesDustEventSerialized> = Vec::new();

			for utxo in utxos {
				// Truncate the block timestamp from milliseconds to seconds
				// Timestamp on Cardano is calculated using (slotLength * slotNumber) + systemStart
				// which can be a fractional value - but in practice, it's an int for
				// preview, pre-prod, and mainnet
				//
				// Check the Shelley genesis files for the networks here:
				// https://book.world.dev.cardano.org/environments.html
				let now = utxo.header.tx_position.block_timestamp.0 as u64 / 1000;

				match utxo.data {
					ObservedUtxoData::Registration(data) => {
						log::debug!("Processing Registration: {data:?}");
						Self::handle_registration(&utxo.header, data);
					},
					ObservedUtxoData::Deregistration(data) => {
						log::debug!("Processing Deregistration: {data:?}");
						Self::handle_registration_removal(&utxo.header, data)
					},
					ObservedUtxoData::AssetCreate(data) => {
						log::debug!("Processing CNight Create: {data:?}");
						if let Some(event) = Self::handle_create(now, data) {
							events.push(event);
						}
					},
					ObservedUtxoData::AssetSpend(data) => {
						log::debug!("Processing CNight Spend: {data:?}");
						if let Some(event) = Self::handle_spend(now, data) {
							events.push(event);
						}
					},
				}
			}

			NextCardanoPosition::<T>::set(next_cardano_position.clone());

			if !events.is_empty() {
				// Construct the Ledger system transaction
				// Note: this into-map should compile into a no-op
				let system_tx_result = LedgerApi::construct_cnight_generates_dust_system_tx(
					events.into_iter().map(|e| e.0).collect(),
				);
				if let Ok(midnight_system_tx) = system_tx_result {
					let system_transaction_hash =
						<T as Config>::MidnightSystemTransactionExecutor::execute_system_transaction(midnight_system_tx)?;

					// Emit System Transaction for the indexer
					let system_tx = SystemTransactionApplied {
						header: CmstHeader {
							block_hash: next_cardano_position.block_hash,
							tx_index_in_block: next_cardano_position.tx_index_in_block,
						},
						system_transaction_hash,
					};
					Self::deposit_event(Event::<T>::SystemTransactionApplied(system_tx));
				} else {
					log::error!("Fatal: failed to construct ledger system transaction");
				}
			}
			Ok(())
		}

		/// Returns the unique dust key for `addr` if and only if exactly one
		/// mapping is registered. Bounded at two storage reads regardless of
		/// how many entries exist for the prefix.
		fn unique_dust_key(addr: &CardanoRewardAddressBytes) -> Option<DustPublicKeyBytes> {
			let mut iter = Mapping::<T>::iter_prefix_values(addr);
			match (iter.next(), iter.next()) {
				(Some(only), None) => Some(only),
				_ => None,
			}
		}

		fn handle_registration(header: &ObservedUtxoHeader, data: RegistrationData) {
			let RegistrationData { cardano_reward_address, dust_public_key } = data;
			let utxo_id = UtxoId::new(header.utxo_tx_hash.0, header.utxo_index.0);

			// Capture the unique-key state before and after the insert; the
			// 0 -> 1 and 1 -> 2+ transitions are exactly the diff between the two.
			let previous_dust_key = Self::unique_dust_key(&cardano_reward_address);
			Mapping::<T>::insert(cardano_reward_address, utxo_id, dust_public_key.clone());
			let new_dust_key = Self::unique_dust_key(&cardano_reward_address);

			match (previous_dust_key, new_dust_key) {
				// 0 -> 1: a new valid registration.
				(None, Some(sole_dust_key)) => {
					Self::deposit_event(Event::<T>::Registration(Registration {
						cardano_reward_address,
						dust_public_key: sole_dust_key,
					}))
				},
				// 1 -> 2+: the previously-valid registration is now ambiguous.
				(Some(prev_dust_key), None) => {
					Self::deposit_event(Event::<T>::Deregistration(Deregistration {
						cardano_reward_address,
						dust_public_key: prev_dust_key,
					}))
				},
				// 2+ -> 3+: still ambiguous, nothing to announce.
				(None, None) => {
					log::debug!(
						"Additional registration for {cardano_reward_address:?}, which already has 2+ mappings"
					);
				},
				// 1 -> 1: only possible if this exact UTXO was already stored.
				(Some(_), Some(_)) => {
					log::error!(
						"error: registration UTXO {utxo_id:?} for {cardano_reward_address:?} observed twice; cursor or data source bug"
					);
				},
			}

			Self::deposit_event(Event::<T>::MappingAdded(MappingEntry {
				cardano_reward_address,
				dust_public_key,
				utxo_id,
			}));
		}

		fn handle_registration_removal(header: &ObservedUtxoHeader, data: DeregistrationData) {
			let DeregistrationData { cardano_reward_address, dust_public_key } = data;
			let utxo_id = UtxoId::new(header.utxo_tx_hash.0, header.utxo_index.0);

			let reg_entry = MappingEntry {
				cardano_reward_address,
				dust_public_key: dust_public_key.clone(),
				utxo_id,
			};

			// Same diff-the-unique-key pattern as handle_registration: the
			// 1 -> 0 and 2+ -> 1 transitions fall out of comparing before vs.
			// after the take.
			let previous_dust_key = Self::unique_dust_key(&cardano_reward_address);

			match Mapping::<T>::take(cardano_reward_address, utxo_id) {
				Some(stored_dust_key) if stored_dust_key != dust_public_key => {
					log::error!(
						"dust key mismatch on deregistration for {cardano_reward_address:?}; removing by utxo ref anyway",
					);
				},
				Some(_) => {},
				None => {
					log::error!(
						"A registration was requested for removal, but does not exist: {reg_entry:?}",
					);
				},
			}

			let new_dust_key = Self::unique_dust_key(&cardano_reward_address);

			match (previous_dust_key, new_dust_key) {
				// 2+ -> 1: the single remaining mapping is now a valid registration.
				(None, Some(sole_dust_key)) => {
					Self::deposit_event(Event::<T>::Registration(Registration {
						cardano_reward_address,
						dust_public_key: sole_dust_key,
					}))
				},
				// 1 -> 0: the valid registration is gone.
				(Some(_), None) => {
					Self::deposit_event(Event::<T>::Deregistration(Deregistration {
						cardano_reward_address,
						dust_public_key,
					}))
				},
				_ => {},
			}

			Self::deposit_event(Event::<T>::MappingRemoved(reg_entry));
		}

		fn handle_create(
			cur_time: u64,
			data: CreateData,
		) -> Option<CNightGeneratesDustEventSerialized> {
			// Unregistered owners are expected (most Cardano reward addresses never
			// post a DUST registration) so this is traced, not warned. Enable trace
			// level on this target to debug "I registered but no DUST appeared".
			let Some(ref dust_public_key) = Self::get_registration(&data.owner) else {
				log::trace!("No valid dust registration for {:?}", data.owner);
				return None;
			};

			let nonce = T::Hashing::hash(
				&[b"asset_create", &data.utxo_tx_hash.0[..], &data.utxo_tx_index.to_be_bytes()[..]]
					.concat(),
			);

			let event = LedgerApi::construct_cnight_generates_dust_event(
				data.value,
				&dust_public_key.0,
				cur_time,
				UtxoActionType::Create as u8,
				nonce.0,
			);

			match event {
				Ok(event_bytes) => {
					UtxoOwners::<T>::insert(nonce, dust_public_key.clone());
					Some(CNightGeneratesDustEventSerialized(event_bytes))
				},
				Err(e) => {
					Self::log_dust_event_failure(&data.utxo_tx_hash.0, data.utxo_tx_index, e);
					None
				},
			}
		}

		fn handle_spend(
			cur_time: u64,
			data: SpendData,
		) -> Option<CNightGeneratesDustEventSerialized> {
			let nonce = T::Hashing::hash(
				&[b"asset_create", &data.utxo_tx_hash.0[..], &data.utxo_tx_index.to_be_bytes()[..]]
					.concat(),
			);

			// No create event means the UTXO was created under an unregistered owner
			// (filtered in handle_create) — the matching spend is a no-op. Traced
			// rather than warned because this is expected for any unregistered holder.
			let Some(dust_public_key) = UtxoOwners::<T>::take(nonce) else {
				log::trace!(
					"No create event for UTXO: {}#{}",
					hex::encode(data.utxo_tx_hash.0),
					data.utxo_tx_index
				);
				return None;
			};

			let event = LedgerApi::construct_cnight_generates_dust_event(
				data.value,
				&dust_public_key.0,
				cur_time,
				UtxoActionType::Destroy as u8,
				nonce.0,
			);

			match event {
				Ok(event_bytes) => Some(CNightGeneratesDustEventSerialized(event_bytes)),
				Err(e) => {
					Self::log_dust_event_failure(&data.utxo_tx_hash.0, data.utxo_tx_index, e);
					None
				},
			}
		}

		fn log_dust_event_failure(tx_hash: &[u8], index: u16, e: LedgerApiError) {
			let utxo = hex::encode(tx_hash);
			match e {
				// Registration datums are user-controlled; out-of-range keys are common.
				LedgerApiError::Deserialization(DeserializationError::DustPublicKey) => {
					log::debug!(
						"Skipping cNIGHT observation {utxo}#{index}: invalid DustPublicKey"
					);
				},
				e => log::warn!(
					"cNIGHT observation {utxo}#{index} dropped: unable to construct CNightGeneratesDustEvent: {e:?}"
				),
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Pre-account for on_finalize weight (storage write to reset inherent flag)
			T::DbWeight::get().writes(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			InherentExecutedThisBlock::<T>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((Pallet::<T>::process_tokens_weight(utxos), DispatchClass::Mandatory))]
		pub fn process_tokens(
			origin: OriginFor<T>,
			utxos: Vec<ObservedUtxo>,
			next_cardano_position: CardanoPosition,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			ensure!(!InherentExecutedThisBlock::<T>::get(), Error::<T>::InherentAlreadyExecuted);
			InherentExecutedThisBlock::<T>::put(true);

			// Skip observation processing entirely while any multi-block migration of
			// this pallet's storage is in flight; `NextCardanoPosition` stays
			// unchanged so the next block's inherent re-presents the same UTXOs (plus
			// any new ones) and we resume once the migration finishes.
			if Pallet::<T>::on_chain_storage_version() < STORAGE_VERSION {
				log::warn!(
					"ObservationsSkippedForMigration: skipping process_tokens (on-chain storage version {:?} < {:?}); MBM in progress",
					Pallet::<T>::on_chain_storage_version(),
					STORAGE_VERSION,
				);
				Self::deposit_event(Event::<T>::ObservationsSkippedForMigration);
				return Ok(PostDispatchInfo {
					actual_weight: Some(T::DbWeight::get().reads_writes(2, 1)),
					pays_fee: Pays::No,
				});
			}

			let weight = Self::process_tokens_weight(&utxos);
			// A failing Mandatory inherent halts the chain, so a block-limit refusal
			// (the native estimate disagreeing with the ledger) rolls back instead.
			let outcome =
				with_storage_layer(|| Self::apply_observations(utxos, next_cardano_position));
			match outcome {
				Ok(()) => {},
				Err(e) if T::MidnightSystemTransactionExecutor::is_block_limit_exceeded(&e) => {
					log::error!(
						"ObservationsDeferred: the ledger refused the cNIGHT-generates-DUST update as over the block limit although the batch was sized to fit; the native cost estimate disagrees with the ledger (bug). Batch rolled back, NextCardanoPosition unchanged."
					);
					Self::deposit_event(Event::<T>::ObservationsDeferred);
				},
				Err(e) => return Err(e.into()),
			}
			Ok(PostDispatchInfo { actual_weight: Some(weight), pays_fee: Pays::No })
		}

		/// Changes the mainchain address for the mapping validator contract
		///
		/// This extrinsic needs Root origin
		#[pallet::call_index(2)]
		#[pallet::weight((
			T::WeightInfo::set_mapping_validator_contract_address(),
			DispatchClass::Normal
		))]
		pub fn set_mapping_validator_contract_address(
			origin: OriginFor<T>,
			address: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;
			MainChainMappingValidatorAddress::<T>::set(
				address
					.clone()
					.try_into()
					.map_err(|_| Error::<T>::MaxCardanoAddrLengthExceeded)?,
			);

			Ok(())
		}

		/// Replaces the (policy id, asset name) pair identifying the cNIGHT native asset
		/// on Cardano. Intended for ephemeral forks redirecting to STAGING contracts.
		///
		/// This extrinsic needs Root origin.
		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::set_cnight_identifier(), DispatchClass::Normal))]
		pub fn set_cnight_identifier(
			origin: OriginFor<T>,
			policy_id: [u8; CNIGHT_POLICY_ID_LENGTH as usize],
			asset_name: BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>,
		) -> DispatchResult {
			ensure_root(origin)?;
			// Genesis validates asset names as ASCII-only strings, and block authors
			// convert this value to a `String` when building the cNIGHT observation
			// inherent. Enforce the same constraint here so a root call cannot store
			// bytes that would make inherent-data creation fail.
			ensure!(asset_name.is_ascii(), Error::<T>::NonAsciiAssetName);
			// Infallible: the array length equals the BoundedVec bound.
			let bounded_policy_id: BoundedVec<u8, ConstU32<CNIGHT_POLICY_ID_LENGTH>> =
				BoundedVec::truncate_from(policy_id.to_vec());
			CNightIdentifier::<T>::set((bounded_policy_id, asset_name));

			Ok(())
		}

		/// Replaces the asset name of the auth token used by the mapping validator on Cardano.
		/// Intended for ephemeral forks redirecting to STAGING contracts.
		///
		/// This extrinsic needs Root origin.
		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::set_auth_token_asset_name(), DispatchClass::Normal))]
		pub fn set_auth_token_asset_name(
			origin: OriginFor<T>,
			asset_name: BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>,
		) -> DispatchResult {
			ensure_root(origin)?;
			// Genesis validates this field as an ASCII-only string, and block authors
			// convert it to a `String` when building the cNIGHT observation inherent.
			// Enforce the same constraint here so a root call cannot store bytes that
			// would make inherent-data creation fail.
			ensure!(asset_name.is_ascii(), Error::<T>::NonAsciiAssetName);
			MainChainAuthTokenAssetName::<T>::set(asset_name);

			Ok(())
		}

		/// Sets the share of the block's ledger cost limits one cNIGHT observation
		/// batch may consume, in percent. The remainder stays available to the
		/// bridge and to user transactions.
		///
		/// This extrinsic needs Root origin.
		#[pallet::call_index(5)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Normal))]
		pub fn set_observation_budget_percent(
			origin: OriginFor<T>,
			percent: u32,
		) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(percent <= 100, Error::<T>::BudgetPercentOutOfRange);
			ObservationBudgetPercent::<T>::set(percent);

			Ok(())
		}
	}
}

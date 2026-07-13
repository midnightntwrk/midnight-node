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
use midnight_primitives_cnight_observation::{CardanoPosition, INHERENT_IDENTIFIER, InherentError};
use midnight_primitives_mainchain_follower::MidnightObservationTokenMovement;
pub use pallet::*;
use serde::{Deserialize, Serialize};
use sidechain_domain::McBlockHash;

pub mod config;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod migrations;

pub mod weights;

/// Cardano-based Midnight System Transaction (CMST)  Header
///
///  * `block`: hash of the last processed Cardano block
///  * `index`: index (zero based) of the next transaction to process in the
///    `block`.  If `index` equals the size of the block, it means that a block has
///    been processed in full.
///
/// See spec for more details:
/// https://github.com/midnightntwrk/midnight-architecture/blob/main/specification/cardano-system-transactions.md#cmst-header
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

pub const INITIAL_CARDANO_BLOCK_WINDOW_SIZE: u32 = 1000;
pub const DEFAULT_CARDANO_TX_CAPACITY_PER_BLOCK: u32 = 200;

/// Runtime acceptance envelope: upper bound on the UTXO-to-TX ratio that
/// `process_tokens` and the worst-case weight will accept per inherent.
///
/// This is intentionally *wider* than the IDP's actual fetch factor (which the
/// node binary picks per `CNightObservationApi` version — 4x at v2+, 64x at v1).
/// The runtime must keep accepting the legacy 64x envelope so that v1 binaries
/// pairing with a v2 runtime during the upgrade window can still have their
/// inherents verified. Do not lower this to match the IDP fetch factor.
pub const UTXO_PER_TX_OVERESTIMATE: u32 = 64;

/// Upper bound on UTXO count per block, used for worst-case weight declaration.
pub const MAX_UTXO_COUNT: u32 = DEFAULT_CARDANO_TX_CAPACITY_PER_BLOCK * UTXO_PER_TX_OVERESTIMATE;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::sp_runtime::traits::Hash;
	use midnight_primitives::MidnightSystemTransactionExecutor;
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

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<Hash = H256> {
		type MidnightSystemTransactionExecutor: MidnightSystemTransactionExecutor;
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
		/// UTXO count exceeds `CardanoTxCapacityPerBlock * UTXO_PER_TX_OVERESTIMATE`
		TooManyUtxos,
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
	pub fn DefaultCardanoBlockWindowSize() -> u32 {
		INITIAL_CARDANO_BLOCK_WINDOW_SIZE
	}

	#[pallet::storage]
	pub type CardanoBlockWindowSize<T: Config> =
		StorageValue<_, u32, ValueQuery, DefaultCardanoBlockWindowSize>;

	#[pallet::type_value]
	pub fn DefaultCardanoTxCapacityPerBlock() -> u32 {
		DEFAULT_CARDANO_TX_CAPACITY_PER_BLOCK
	}

	#[pallet::storage]
	/// Max amount of Cardano transactions that can be processed per block
	pub type CardanoTxCapacityPerBlock<T: Config> =
		StorageValue<_, u32, ValueQuery, DefaultCardanoTxCapacityPerBlock>;

	#[pallet::storage]
	pub type InherentExecutedThisBlock<T: Config> = StorageValue<_, bool, ValueQuery>;

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
				_ => {
					log::error!(
						"fatal integrity error: mapping added, previous and post mapping count == 1"
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
				log::trace!("No valid dust registration for {:?}", &data.owner);
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
					log::error!("Fatal: Unable to construct CNightGeneratesDustEvent: {e:?}");
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
					log::error!("Fatal: Unable to construct CNightGeneratesDustEvent: {e:?}");
					None
				},
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
		#[pallet::weight((T::WeightInfo::process_tokens(CardanoTxCapacityPerBlock::<T>::get().saturating_mul(UTXO_PER_TX_OVERESTIMATE)), DispatchClass::Mandatory))]
		pub fn process_tokens(
			origin: OriginFor<T>,
			utxos: Vec<ObservedUtxo>,
			next_cardano_position: CardanoPosition,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			let utxo_count = utxos.len() as u32;
			ensure!(
				utxo_count
					<= CardanoTxCapacityPerBlock::<T>::get()
						.saturating_mul(UTXO_PER_TX_OVERESTIMATE),
				Error::<T>::TooManyUtxos
			);
			ensure!(!InherentExecutedThisBlock::<T>::get(), Error::<T>::InherentAlreadyExecuted);
			InherentExecutedThisBlock::<T>::put(true);

			// While a multi-block migration of `Mapping` is still draining v0 storage,
			// `unique_dust_key` (and therefore `handle_registration`,
			// `handle_registration_removal`, `handle_create`) reads only v1, missing
			// any v0 row that hasn't been moved yet. Acting on that partial view would
			// silently corrupt registration state — e.g. a deregistration whose v0
			// row is still pending would no-op here and then re-appear as live once
			// the migration drains it. Skip processing entirely; `NextCardanoPosition`
			// stays unchanged so the next block's inherent re-presents the same UTXOs
			// (plus any new ones) and we resume once the migration finishes.
			if Pallet::<T>::on_chain_storage_version() < STORAGE_VERSION {
				log::warn!(
					"cnight-observation: skipping process_tokens (on-chain storage version {:?} < {:?}); MBM in progress",
					Pallet::<T>::on_chain_storage_version(),
					STORAGE_VERSION,
				);
				return Ok(PostDispatchInfo {
					actual_weight: Some(T::DbWeight::get().reads_writes(2, 1)),
					pays_fee: Pays::No,
				});
			}

			let prev = NextCardanoPosition::<T>::get();
			ensure!(next_cardano_position >= prev, Error::<T>::CardanoPositionRegression);
			let jump = next_cardano_position.block_number.saturating_sub(prev.block_number);
			let window = CardanoBlockWindowSize::<T>::get();
			if jump > window {
				log::warn!(
					"CardanoPosition jump ({jump}) exceeds CardanoBlockWindowSize ({window}); allowing but flagging"
				);
			}

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
			Ok(PostDispatchInfo {
				actual_weight: Some(T::WeightInfo::process_tokens(utxo_count)),
				pays_fee: Pays::No,
			})
		}

		/// Changes the mainchain address for the mapping validator contract
		///
		/// This extrinsic needs Root origin
		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Normal))]
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
	}
}

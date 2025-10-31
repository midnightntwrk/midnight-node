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

//! # Native Token Observation Pallet
//!
//! This pallet provides mechanisms for tracking all registrations for cNIGHT generates DUST from Cardano,
//! as well as observation of all cNIGHT utxos of valid registrants of cNIGHT generates DUST.

#![cfg_attr(not(feature = "std"), no_std)]

use derive_new::new;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use midnight_primitives_cnight_observation::{CardanoPosition, INHERENT_IDENTIFIER, InherentError};
use midnight_primitives_mainchain_follower::MidnightObservationTokenMovement;
pub use pallet::*;
use serde::{Deserialize, Serialize};

pub mod config;

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
	pub block_hash: [u8; 32],
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

#[frame_support::pallet]
pub mod pallet {
	use frame_support::sp_runtime::traits::Hash;
	use midnight_primitives::MidnightSystemTransactionExecutor;
	use midnight_primitives_cnight_observation::{
		CARDANO_BECH32_ADDRESS_MAX_LENGTH, CardanoRewardAddressBytes, DustPublicKeyBytes,
	};
	use midnight_primitives_mainchain_follower::{
		CreateData, DeregistrationData, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
		RedemptionCreateData, RedemptionSpendData, RegistrationData, SpendData,
	};
	use scale_info::prelude::vec::Vec;
	use sp_core::H256;

	use midnight_node_ledger::types::{
		Hash as LedgerHash, active_ledger_bridge as LedgerApi, active_version::LedgerApiError,
	};

	use crate::config::CNightGenesis;

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
		pub utxo_id: [u8; 32],
		pub utxo_index: u16,
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

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<Hash = H256> {
		type MidnightSystemTransactionExecutor: MidnightSystemTransactionExecutor;
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
		MaxRegistrationsExceeded,
		LedgerApiError(LedgerApiError),
	}

	impl<T: Config> From<LedgerApiError> for Error<T> {
		fn from(value: LedgerApiError) -> Self {
			Error::<T>::LedgerApiError(value)
		}
	}

	#[pallet::storage]
	// Script address for managing registrations on Cardano
	pub type MainChainMappingValidatorAddress<T: Config> =
		StorageValue<_, BoundedCardanoAddress, ValueQuery>;

	#[pallet::storage]
	// Script address for executing Glacier Drop redemptions on Cardano
	pub type MainChainRedemptionValidatorAddress<T: Config> =
		StorageValue<_, BoundedCardanoAddress, ValueQuery>;

	#[pallet::storage]
	pub type Mappings<T: Config> =
		StorageMap<_, Blake2_128Concat, CardanoRewardAddressBytes, Vec<MappingEntry>, ValueQuery>;

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
			BoundedVec<u8, ConstU32<28>>,
			// Asset Name
			BoundedVec<u8, ConstU32<32>>,
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

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub config: CNightGenesis,
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MainChainMappingValidatorAddress::<T>::set(
				self.config
					.addresses
					.mapping_validator_address
					.as_bytes()
					.to_vec()
					.try_into()
					.expect("Mapping Validator address longer than expected"),
			);

			MainChainRedemptionValidatorAddress::<T>::set(
				self.config
					.addresses
					.redemption_validator_address
					.as_bytes()
					.to_vec()
					.try_into()
					.expect("Redemption Validator address longer than expected"),
			);

			CNightIdentifier::<T>::set((
				self.config
					.addresses
					.cnight_policy_id
					.to_vec()
					.try_into()
					.expect("Policy ID too long"),
				self.config
					.addresses
					.cnight_asset_name
					.as_bytes()
					.to_vec()
					.try_into()
					.expect("Asset name too long"),
			));

			for (k, v) in &self.config.mappings {
				Mappings::<T>::insert(k, v.clone());
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
			Self::get_data_from_inherent_data(data).map(|data| Call::process_tokens {
				utxos: data.utxos,
				next_cardano_position: data.next_cardano_position,
			})
		}

		fn check_inherent(call: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			let Call::process_tokens { utxos, next_cardano_position } = call else {
				return Ok(());
			};

			let parsed = Self::get_data_from_inherent_data(data).ok_or(InherentError::Other)?;
			if parsed.utxos != *utxos || parsed.next_cardano_position != *next_cardano_position {
				return Err(InherentError::Other);
			}
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::process_tokens { .. })
		}

		fn is_inherent_required(data: &InherentData) -> Result<Option<Self::Error>, Self::Error> {
			Ok(if Self::get_data_from_inherent_data(data).is_some() {
				Some(InherentError::Missing)
			} else {
				None
			})
		}
	}

	impl<T: Config> Pallet<T> {
		fn get_data_from_inherent_data(
			data: &InherentData,
		) -> Option<MidnightObservationTokenMovement> {
			data.get_data::<MidnightObservationTokenMovement>(&INHERENT_IDENTIFIER)
				.expect("Token transfer data not encoded correctly")
		}

		pub fn get_registration(wallet: &CardanoRewardAddressBytes) -> Option<DustPublicKeyBytes> {
			let mappings = Mappings::<T>::get(wallet);
			if mappings.len() == 1 { Some(mappings[0].dust_public_key) } else { None }
		}

		// Check if any form of a registration could be considered valid as of now
		pub fn is_registered(utxo_holder: &CardanoRewardAddressBytes) -> bool {
			let mappings = Mappings::<T>::get(utxo_holder);
			// For a registration to be valid, there can only be one stored
			mappings.len() == 1
		}

		#[allow(clippy::type_complexity)]
		fn handle_registration(
			header: &ObservedUtxoHeader,
			data: RegistrationData,
		) -> Option<(CardanoRewardAddressBytes, Vec<MappingEntry>)> {
			let RegistrationData { cardano_reward_address, dust_public_key } = data;

			let new_reg = MappingEntry {
				cardano_reward_address,
				dust_public_key,
				utxo_id: header.utxo_tx_hash.0,
				utxo_index: header.utxo_index.0,
			};

			let mut mappings = Mappings::<T>::get(cardano_reward_address);
			mappings.push(new_reg.clone());
			Mappings::<T>::insert(cardano_reward_address, mappings.clone());

			if mappings.len() == 1 {
				Self::deposit_event(Event::<T>::Registration(Registration {
					cardano_reward_address,
					dust_public_key,
				}));
			}

			Self::deposit_event(Event::<T>::MappingAdded(new_reg));
			Some((cardano_reward_address, mappings))
		}

		fn handle_registration_removal(header: &ObservedUtxoHeader, data: DeregistrationData) {
			let DeregistrationData { cardano_reward_address, dust_public_key } = data;

			let reg_entry = MappingEntry {
				cardano_reward_address,
				dust_public_key,
				utxo_id: header.utxo_tx_hash.0,
				utxo_index: header.utxo_index.0,
			};

			let was_valid = Self::is_registered(&cardano_reward_address);
			let mut mappings = Mappings::<T>::get(cardano_reward_address);

			if let Some(index) = mappings.iter().position(|x| x == &reg_entry) {
				mappings.remove(index);
			} else {
				log::error!(
					"A registration was requested for removal, but does not exist: {:?} ",
					reg_entry.clone()
				);
			}

			if mappings.is_empty() {
				Mappings::<T>::remove(cardano_reward_address);
			} else {
				Mappings::<T>::insert(cardano_reward_address, mappings.clone());
			}

			let is_valid = Self::is_registered(&cardano_reward_address);
			// A removal of a mapping can be done in the case of an invalid registration, making the mapping a valid registration.
			if !was_valid && is_valid {
				Self::deposit_event(Event::<T>::Registration(Registration {
					cardano_reward_address,
					dust_public_key,
				}))
			}

			// If we previously had a valid registration, then had the amount of mappings brought to 0, we've had a Deregistration
			if was_valid && !is_valid {
				Self::deposit_event(Event::<T>::Deregistration(Deregistration {
					cardano_reward_address,
					dust_public_key,
				}))
			}

			Self::deposit_event(Event::<T>::MappingRemoved(reg_entry));
		}

		fn handle_create(
			cur_time: u64,
			data: CreateData,
		) -> Option<CNightGeneratesDustEventSerialized> {
			let Some(dust_public_key) = Self::get_registration(&data.owner) else {
				log::warn!("No valid dust registration for {:?}", &data.owner);
				return None;
			};

			let nonce = T::Hashing::hash(
				&[b"asset_create", &data.utxo_tx_hash.0[..], &data.utxo_tx_index.to_be_bytes()[..]]
					.concat(),
			);

			UtxoOwners::<T>::insert(nonce, dust_public_key);

			let event = LedgerApi::construct_cnight_generates_dust_event(
				data.value,
				&dust_public_key.0,
				cur_time,
				UtxoActionType::Create as u8,
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

		fn handle_spend(
			cur_time: u64,
			data: SpendData,
		) -> Option<CNightGeneratesDustEventSerialized> {
			let nonce = T::Hashing::hash(
				&[b"asset_create", &data.utxo_tx_hash.0[..], &data.utxo_tx_index.to_be_bytes()[..]]
					.concat(),
			);

			let Some(dust_public_key) = UtxoOwners::<T>::get(nonce) else {
				log::warn!(
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

		fn handle_redemption_create(
			cur_time: u64,
			data: RedemptionCreateData,
		) -> Option<CNightGeneratesDustEventSerialized> {
			let Some(dust_public_key) = Self::get_registration(&data.owner) else {
				log::warn!("No valid dust registration for {:?}", &data.owner);
				return None;
			};

			let nonce = T::Hashing::hash(
				&[
					b"redemption_create",
					&data.utxo_tx_hash.0[..],
					&data.utxo_tx_index.to_be_bytes()[..],
				]
				.concat(),
			);

			UtxoOwners::<T>::insert(nonce, dust_public_key);

			let event = LedgerApi::construct_cnight_generates_dust_event(
				data.value,
				&dust_public_key.0,
				cur_time,
				UtxoActionType::Create as u8,
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

		fn handle_redemption_spend(
			cur_time: u64,
			data: RedemptionSpendData,
		) -> Option<CNightGeneratesDustEventSerialized> {
			let nonce = T::Hashing::hash(
				&[
					b"redemption_create",
					&data.utxo_tx_hash.0[..],
					&data.utxo_tx_index.to_be_bytes()[..],
				]
				.concat(),
			);

			let Some(dust_public_key) = UtxoOwners::<T>::get(nonce) else {
				log::warn!(
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

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn process_tokens(
			origin: OriginFor<T>,
			utxos: Vec<ObservedUtxo>,
			next_cardano_position: CardanoPosition,
		) -> DispatchResult {
			ensure_none(origin)?;

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
					ObservedUtxoData::RedemptionCreate(data) => {
						log::debug!("Processing Redemption Create: {data:?}");
						if let Some(event) = Self::handle_redemption_create(now, data) {
							events.push(event);
						}
					},
					ObservedUtxoData::RedemptionSpend(data) => {
						log::debug!("Processing Redemption Spend: {data:?}");
						if let Some(event) = Self::handle_redemption_spend(now, data) {
							events.push(event);
						}
					},
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
							block_hash: next_cardano_position.block_hash.0,
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

		/// Changes the mainchain address for the mapping validator contract
		///
		/// This extrinsic must be run either using `sudo` or some other chain governance mechanism.
		#[pallet::call_index(2)]
		#[pallet::weight((1, DispatchClass::Normal))]
		pub fn set_mapping_validator_contract_address(
			origin: OriginFor<T>,
			address: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;
			MainChainMappingValidatorAddress::<T>::set(
				address
					.clone()
					.try_into()
					.expect("Mainchain contract address longer than expected"),
			);

			Ok(())
		}
	}
}

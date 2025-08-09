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

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use midnight_primitives_mainchain_follower::idp::MidnightObservationTokenMovement;
use midnight_primitives_mainchain_follower::idp::{INHERENT_IDENTIFIER, InherentError};
use midnight_primitives_native_token_observation::CardanoPosition;
pub use pallet::*;
use pallet_timestamp::{self as timestamp};
use scale_info::prelude::vec::Vec;

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
pub enum UtxoActionType {
	Create,
	Destroy,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct NgDPayloadEntry {
	value: u128,
	owner: Vec<u8>,
	time: u64,
	action: UtxoActionType,
	nonce: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct NgDPayload {
	events: Vec<NgDPayloadEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct SystemTx {
	header: CmstHeader,
	body: NgDPayload,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mock;

pub const INITIAL_CARDANO_BLOCK_WINDOW_SIZE: u32 = 1000;
pub const DEFAULT_CARDANO_TX_CAPACITY_PER_BLOCK: u32 = 30;
pub const MAX_CARDANO_ADDR_LEN: u32 = 150;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::sp_runtime::traits::Hash;
	use midnight_primitives_mainchain_follower::{
		CreateData, DeregistrationData, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
		RegistrationData, SpendData,
	};
	use scale_info::prelude::vec::Vec;
	use sp_core::H256;

	use super::*;

	pub type BoundedCardanoAddress = BoundedVec<u8, ConstU32<MAX_CARDANO_ADDR_LEN>>;
	pub type DustAddress = BoundedVec<u8, ConstU32<32>>;
	pub type BoundedUtxoHash = BoundedVec<u8, ConstU32<32>>;

	#[derive(
		Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct MappingEntry {
		cardano_address: BoundedCardanoAddress,
		dust_address: DustAddress,
		utxo_id: [u8; 32],
		utxo_index: u16,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<Hash = H256> + timestamp::Config<Moment = u64> {
		#[pallet::constant]
		type MaxRegistrationsPerCardanoAddress: Get<u32>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		Added(
			(BoundedCardanoAddress, BoundedVec<MappingEntry, T::MaxRegistrationsPerCardanoAddress>),
		),
		/// Tried to remove an element, but it was not found in the list of registrations
		AttemptedRemoveNonexistantElement(MappingEntry),
		/// Could not add registration
		CouldNotAddRegistration,
		DuplicatedRegistration(
			(BoundedCardanoAddress, BoundedVec<MappingEntry, T::MaxRegistrationsPerCardanoAddress>),
		),
		InvalidCardanoAddress,
		InvalidDustAddress,
		Registered(
			(BoundedCardanoAddress, BoundedVec<MappingEntry, T::MaxRegistrationsPerCardanoAddress>),
		),
		/// Removed registrations
		Removed((BoundedCardanoAddress, MappingEntry)),
		/// Removed single registration in order to add a new registration in order to respect length bound of registration list
		RemovedOld((BoundedCardanoAddress, MappingEntry)),
		/// System transaction - the `SystemTx` struct is defined in the Node for now, but this event will contain a Ledger System Transaction
		SystemTx(SystemTx),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A Cardano Wallet address was sent, but was longer than expected
		MaxCardanoAddrLengthExceeded,
		MaxRegistrationsExceeded,
	}

	#[pallet::storage]
	// Script address for managing registrations on Cardano
	pub type MainChainGenerationRegistrantsAddress<T: Config> =
		StorageValue<_, BoundedCardanoAddress, ValueQuery>;

	// TODO: Move registrations offchain
	#[pallet::storage]
	pub type Registrations<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		BoundedCardanoAddress,
		BoundedVec<MappingEntry, T::MaxRegistrationsPerCardanoAddress>,
		ValueQuery,
	>;

	// TODO: Read from ledger state directly ?
	#[pallet::storage]
	pub type UtxoOwners<T: Config> =
		StorageMap<_, Blake2_128Concat, BoundedUtxoHash, DustAddress, OptionQuery>;

	#[pallet::storage]
	// The next Cardano position to look for new transactions
	pub type NextCardanoPosition<T: Config> = StorageValue<_, CardanoPosition, ValueQuery>;

	#[pallet::storage]
	// A full identifier for a native asset on Cardano: (policy id, asset name)
	pub type NativeAssetIdentifier<T: Config> = StorageValue<
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
		pub registration_address: Vec<u8>,
		pub token_policy_id: Vec<u8>,
		pub token_asset_name: Vec<u8>,
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MainChainGenerationRegistrantsAddress::<T>::set(
				self.registration_address
					.clone()
					.try_into()
					.expect("Mainchain contract address longer than expected"),
			);

			NativeAssetIdentifier::<T>::set((
				self.token_policy_id.clone().try_into().expect("Policy ID too long"),
				self.token_asset_name.clone().try_into().expect("Asset name too long"),
			));
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

		pub fn get_registrations_for(
			wallet: BoundedCardanoAddress,
		) -> BoundedVec<DustAddress, T::MaxRegistrationsPerCardanoAddress> {
			let regs: Vec<_> =
				Registrations::<T>::get(wallet).into_iter().map(|r| r.dust_address).collect();
			regs.try_into().unwrap()
		}

		pub fn get_registration(wallet: &BoundedCardanoAddress) -> Option<DustAddress> {
			let registrations = Registrations::<T>::get(wallet);
			if registrations.len() == 1 {
				Some(registrations[0].dust_address.clone())
			} else {
				None
			}
		}

		pub fn is_registered(utxo_holder: &BoundedCardanoAddress) -> bool {
			let registrations = Registrations::<T>::get(utxo_holder);
			registrations.len() == 1
		}

		#[allow(clippy::type_complexity)]
		fn handle_registration(
			header: &ObservedUtxoHeader,
			data: RegistrationData,
		) -> Option<(
			BoundedCardanoAddress,
			BoundedVec<MappingEntry, T::MaxRegistrationsPerCardanoAddress>,
		)> {
			// TODO: Invalid addresses should be skipped - they may be for an incorrect network, or
			// something else
			// TODO: Add UTXO info for invalid addresses

			let cardano_address: BoundedCardanoAddress = match data.cardano_address.try_into() {
				Ok(addr) => addr,
				Err(_) => {
					Self::deposit_event(Event::<T>::InvalidCardanoAddress);
					return None;
				},
			};

			let dust_address: DustAddress = match data.dust_address.try_into() {
				Ok(addr) => addr,
				Err(_) => {
					Self::deposit_event(Event::<T>::InvalidDustAddress);
					return None;
				},
			};

			let new_reg = MappingEntry {
				cardano_address: cardano_address.clone(),
				dust_address,
				utxo_id: header.utxo_tx_hash.0,
				utxo_index: header.utxo_index.0,
			};

			let was_valid = Self::is_registered(&cardano_address);
			let mut registrations = Registrations::<T>::get(&cardano_address);
			let removed_old =
				if registrations.is_full() { Some(registrations.remove(0)) } else { None };

			if let Some(removed_old) = removed_old {
				Self::deposit_event(Event::<T>::RemovedOld((cardano_address.clone(), removed_old)));
			};

			if registrations.try_push(new_reg).is_err() {
				Self::deposit_event(Event::<T>::CouldNotAddRegistration);
			}

			if registrations.len() == 2 {
				Self::deposit_event(Event::<T>::DuplicatedRegistration((
					cardano_address.clone(),
					registrations.clone(),
				)));
			}

			Registrations::<T>::insert(cardano_address.clone(), registrations.clone());

			let is_valid_registration = Self::is_registered(&cardano_address);

			if was_valid != is_valid_registration && is_valid_registration {
				Self::deposit_event(Event::<T>::Registered((
					cardano_address.clone(),
					registrations.clone(),
				)));
			}
			Self::deposit_event(Event::<T>::Added((
				cardano_address.clone(),
				registrations.clone(),
			)));
			Some((cardano_address, registrations))
		}

		fn handle_registration_removal(header: &ObservedUtxoHeader, data: DeregistrationData) {
			let Ok(cardano_address) = BoundedVec::try_from(data.cardano_address) else {
				Self::deposit_event(Event::<T>::InvalidCardanoAddress);
				return;
			};

			let Ok(dust_address): Result<DustAddress, _> = data.dust_address.try_into() else {
				Self::deposit_event(Event::<T>::InvalidDustAddress);
				return;
			};

			let reg_entry = MappingEntry {
				cardano_address: cardano_address.clone(),
				dust_address,
				utxo_id: header.utxo_tx_hash.0,
				utxo_index: header.utxo_index.0,
			};

			let was_valid = Self::is_registered(&cardano_address);
			let mut registrations = Registrations::<T>::get(&cardano_address);

			if let Some(index) = registrations.iter().position(|x| x == &reg_entry) {
				registrations.remove(index);
			} else {
				Self::deposit_event(Event::<T>::AttemptedRemoveNonexistantElement(
					reg_entry.clone(),
				));
			}

			if registrations.is_empty() {
				Registrations::<T>::remove(&cardano_address);
			} else {
				Registrations::<T>::insert(&cardano_address, registrations.clone());
			}

			let is_valid = Self::is_registered(&cardano_address);
			if was_valid != is_valid && is_valid {
				Self::deposit_event(Event::<T>::Registered((
					cardano_address.clone(),
					registrations.clone(),
				)))
			}
			Self::deposit_event(Event::<T>::Removed((cardano_address, reg_entry)));
		}

		fn handle_create(cur_time: u64, data: CreateData) -> Option<NgDPayloadEntry> {
			let Ok(cardano_address) = BoundedVec::try_from(data.owner) else {
				Self::deposit_event(Event::<T>::InvalidCardanoAddress);
				return None;
			};

			let Some(dust_address) = Self::get_registration(&cardano_address) else {
				log::warn!("No valid dust registration for {cardano_address:?}");
				return None;
			};

			let nonce = T::Hashing::hash(
				&[&data.utxo_tx_hash[..], &data.utxo_tx_index.to_be_bytes()[..]].concat(),
			);

			let Ok(utxo_id) = BoundedVec::try_from(nonce.0.to_vec()) else {
				log::error!(
					"cannot create bounded vec from utxo: {}#{}",
					hex::encode(data.utxo_tx_hash),
					data.utxo_tx_index
				);
				return None;
			};

			UtxoOwners::<T>::insert(&utxo_id, dust_address.clone());

			Some(NgDPayloadEntry {
				value: data.value,
				owner: dust_address.into(),
				time: cur_time,
				action: UtxoActionType::Create,
				nonce: nonce.0,
			})
		}

		fn handle_spend(cur_time: u64, data: SpendData) -> Option<NgDPayloadEntry> {
			let nonce = T::Hashing::hash(
				&[&data.utxo_tx_hash[..], &data.utxo_tx_index.to_be_bytes()[..]].concat(),
			);

			let Ok(utxo_hash) = BoundedVec::try_from(nonce.0.to_vec()) else {
				Self::deposit_event(Event::<T>::InvalidCardanoAddress);
				return None;
			};

			let Some(dust_address) = UtxoOwners::<T>::get(&utxo_hash) else {
				log::warn!(
					"No create event for UTXO: {}#{}",
					hex::encode(data.utxo_tx_hash),
					data.utxo_tx_index
				);
				return None;
			};

			Some(NgDPayloadEntry {
				value: data.value,
				owner: dust_address.into(),
				time: cur_time,
				action: UtxoActionType::Destroy,
				nonce: data.spending_tx_hash,
			})
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

			let now = timestamp::Pallet::<T>::get();

			let mut events = Vec::new();

			for utxo in utxos {
				match utxo.data {
					ObservedUtxoData::Registration(data) => {
						log::info!("Processing Registration: {data:?}");
						Self::handle_registration(&utxo.header, data);
					},
					ObservedUtxoData::Deregistration(data) => {
						log::info!("Processing Deregistration: {data:?}");
						Self::handle_registration_removal(&utxo.header, data)
					},
					ObservedUtxoData::AssetCreate(data) => {
						log::info!("Processing CNight Create: {data:?}");
						if let Some(event) = Self::handle_create(now, data) {
							events.push(event);
						}
					},
					ObservedUtxoData::AssetSpend(data) => {
						log::info!("Processing CNight Spend: {data:?}");
						if let Some(event) = Self::handle_spend(now, data) {
							events.push(event);
						}
					},
				}
			}

			NextCardanoPosition::<T>::set(next_cardano_position);

			if !events.is_empty() {
				// Emit System Transaction for the indexer
				let system_tx = SystemTx {
					header: CmstHeader {
						block_hash: next_cardano_position.block_hash,
						tx_index_in_block: next_cardano_position.tx_index_in_block,
					},
					body: NgDPayload { events },
				};
				Self::deposit_event(Event::<T>::SystemTx(system_tx));
				// TODO: Call Ledger API
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
			MainChainGenerationRegistrantsAddress::<T>::set(
				address
					.clone()
					.try_into()
					.expect("Mainchain contract address longer than expected"),
			);

			Ok(())
		}
	}
}

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
use midnight_primitives_mainchain_follower::idp::MidnightRuntimeUtxoRepresentation;
use midnight_primitives_mainchain_follower::idp::{INHERENT_IDENTIFIER, InherentError};
pub use pallet::*;
use sidechain_domain::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mock;

pub const INITIAL_CARDANO_BLOCK_WINDOW_SIZE: i64 = 1000;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	pub type BoundedCardanoAddress<T> = BoundedVec<u8, <T as Config>::MaxCardanoAddrLen>;
	pub type BoundedDustAddress = BoundedVec<u8, ConstU32<32>>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		#[pallet::constant]
		type MaxCardanoAddrLen: Get<u32>;
		#[pallet::constant]
		type MaxRegistrationsPerCardanoAddress: Get<u32>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Added new DUST address mapping to Cardano wallet
		Added(
			(
				BoundedCardanoAddress<T>,
				BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress>,
			),
		),
		/// Tried to remove an element, but it was not found in the list of registrations
		AttemptedRemoveNonexistantElement,
		/// Could not add registration
		CouldNotAddRegistration,
		/// Someone submitted a registration that took them from 1 -> 2 registrations
		DuplicatedRegistration(
			(
				BoundedCardanoAddress<T>,
				BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress>,
			),
		),
		InvalidCardanoAddress,
		InvalidDustAddress,
		/// A mapping has become valid
		Registered(
			(
				BoundedCardanoAddress<T>,
				BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress>,
			),
		),
		/// Removed registrations
		Removed(
			(
				BoundedCardanoAddress<T>,
				BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress>,
			),
		),
		/// Removed single registration in order to add a new registration in order to respect length bound of registration list
		RemovedOld((BoundedCardanoAddress<T>, BoundedDustAddress)),
		/// A placeholder for the new Ledger ValidUtxo variant
		ValidUtxo(MidnightRuntimeUtxoRepresentation),
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
		StorageValue<_, BoundedCardanoAddress<T>, ValueQuery>;

	#[pallet::storage]
	pub type Registrations<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		BoundedCardanoAddress<T>,
		BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type LastCardanoBlock<T: Config> = StorageValue<_, i64, ValueQuery>;

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
	pub fn DefaultCardanoBlockWindowSize() -> i64 {
		INITIAL_CARDANO_BLOCK_WINDOW_SIZE
	}

	#[pallet::storage]
	pub type CardanoBlockWindowSize<T: Config> =
		StorageValue<_, i64, ValueQuery, DefaultCardanoBlockWindowSize>;

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
				new_registrations: data.new_registrations,
				registrations_to_remove: data.registrations_to_remove,
				utxos: data.utxos,
				cardano_highest_block: data.latest_block,
			})
		}

		fn check_inherent(call: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			let Call::process_tokens {
				new_registrations,
				registrations_to_remove,
				utxos,
				cardano_highest_block,
			} = call
			else {
				return Ok(());
			};

			let parsed = Self::get_data_from_inherent_data(data).ok_or(InherentError::Other)?;

			if parsed.new_registrations != *new_registrations
				|| parsed.registrations_to_remove != *registrations_to_remove
				|| parsed.utxos != *utxos
				|| parsed.latest_block != *cardano_highest_block
			{
				return Err(InherentError::Other);
			}
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::process_tokens { .. })
		}

		fn is_inherent_required(data: &InherentData) -> Result<Option<Self::Error>, Self::Error> {
			Ok(if Self::get_data_from_inherent_data(data).is_some() {
				Some(InherentError::Other)
			} else {
				None
			})
		}
	}

	impl<T: Config> Pallet<T> {
		fn get_data_from_inherent_data(
			data: &InherentData,
		) -> Option<MidnightObservationTokenMovement> {
			let data = data
				.get_data::<MidnightObservationTokenMovement>(&INHERENT_IDENTIFIER)
				.expect("Token transfer data is not encoded correctly")?;

			Some(data)
		}

		pub fn get_registrations_for(
			wallet: BoundedCardanoAddress<T>,
		) -> BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress> {
			Registrations::<T>::get(wallet)
		}

		// Get the range of Cardano blocks to use for the next window of registrations and gencurrency utxos
		pub fn get_next_block_range() -> (i64, i64) {
			let min = LastCardanoBlock::<T>::get();
			let max = min + CardanoBlockWindowSize::<T>::get();
			(min, max)
		}

		pub fn is_registered(utxo_holder: &BoundedCardanoAddress<T>) -> bool {
			let registrations = Registrations::<T>::get(utxo_holder);
			registrations.len() == 1
		}

		#[allow(clippy::type_complexity)]
		fn handle_registration(
			cardano_address_raw: Vec<u8>,
			dust_address_raw: Vec<u8>,
		) -> Option<(
			BoundedCardanoAddress<T>,
			BoundedVec<BoundedDustAddress, T::MaxRegistrationsPerCardanoAddress>,
		)> {
			let cardano_address = match BoundedVec::try_from(cardano_address_raw) {
				Ok(addr) => addr,
				Err(_) => {
					Self::deposit_event(Event::<T>::InvalidCardanoAddress);
					return None;
				},
			};

			let dust_address: BoundedDustAddress = match dust_address_raw.try_into() {
				Ok(addr) => addr,
				Err(_) => {
					Self::deposit_event(Event::<T>::InvalidDustAddress);
					return None;
				},
			};

			let was_valid = Self::is_registered(&cardano_address);
			let mut registrations = Registrations::<T>::get(&cardano_address);
			let removed_old =
				if registrations.is_full() { Some(registrations.remove(0)) } else { None };

			if registrations.try_push(dust_address.clone()).is_err() {
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

			if let Some(removed_old) = removed_old {
				Self::deposit_event(Event::<T>::RemovedOld((cardano_address.clone(), removed_old)));
			}

			Some((cardano_address, registrations))
		}

		fn handle_removal(cardano_address_raw: Vec<u8>, dust_address_raw: Vec<u8>) {
			let cardano_address = match BoundedVec::try_from(cardano_address_raw) {
				Ok(addr) => addr,
				Err(_) => {
					Self::deposit_event(Event::<T>::InvalidCardanoAddress);
					return;
				},
			};

			let dust_address: BoundedDustAddress = match dust_address_raw.try_into() {
				Ok(addr) => addr,
				Err(_) => {
					Self::deposit_event(Event::<T>::InvalidDustAddress);
					return;
				},
			};

			let was_valid = Self::is_registered(&cardano_address);
			let mut dust_wallets = Registrations::<T>::get(&cardano_address);

			if let Some(index) = dust_wallets.iter().position(|x| x == &dust_address) {
				dust_wallets.remove(index);
			} else {
				Self::deposit_event(Event::<T>::AttemptedRemoveNonexistantElement);
			}

			if dust_wallets.is_empty() {
				Registrations::<T>::remove(&cardano_address);
			} else {
				Registrations::<T>::insert(&cardano_address, dust_wallets.clone());
			}

			let is_valid = Self::is_registered(&cardano_address);
			if was_valid != is_valid && is_valid {
				Self::deposit_event(Event::<T>::Registered((
					cardano_address.clone(),
					dust_wallets.clone(),
				)))
			}
			Self::deposit_event(Event::<T>::Removed((cardano_address, dust_wallets)));
		}

		fn emit_valid_utxos(utxos: Vec<MidnightRuntimeUtxoRepresentation>) {
			for utxo in utxos {
				let holder_address = match BoundedVec::try_from(utxo.holder_address.clone()) {
					Ok(addr) => addr,
					Err(_) => {
						Self::deposit_event(Event::<T>::InvalidCardanoAddress);
						continue;
					},
				};

				if Self::is_registered(&holder_address) {
					Self::deposit_event(Event::<T>::ValidUtxo(utxo));
				}
			}
		}
		fn update_cardano_block_window(cardano_highest_block: i64) {
			let last = LastCardanoBlock::<T>::get();
			let mut window = CardanoBlockWindowSize::<T>::get();

			let remaining = cardano_highest_block.saturating_sub(last);
			if remaining == 0 {
				return;
			}

			let adjusted = remaining.min(window);
			if adjusted < window {
				CardanoBlockWindowSize::<T>::set(adjusted);
				window = adjusted;
			}

			LastCardanoBlock::<T>::set(last + window);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn process_tokens(
			origin: OriginFor<T>,
			new_registrations: Vec<(Vec<u8>, Vec<u8>)>,
			registrations_to_remove: Vec<(Vec<u8>, Vec<u8>)>,
			utxos: Vec<MidnightRuntimeUtxoRepresentation>,
			cardano_highest_block: i64,
		) -> DispatchResult {
			ensure_none(origin)?;
			for (cardano, dust) in new_registrations {
				Self::handle_registration(cardano, dust);
			}

			for (cardano, dust) in registrations_to_remove {
				Self::handle_removal(cardano, dust);
			}

			Self::update_cardano_block_window(cardano_highest_block);

			Self::emit_valid_utxos(utxos);
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

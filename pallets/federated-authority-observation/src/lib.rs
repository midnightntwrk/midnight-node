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

//! # Federated Authority Observation Pallet
//!
//! This pallet provides mechanisms for observing federated authority changes from the main chain.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	dispatch::{Pays, PostDispatchInfo},
	pallet_prelude::*,
	traits::{ChangeMembers, SortedMembers},
};
use frame_system::pallet_prelude::*;
use midnight_primitives_federated_authority_observation::{
	AuthorityMemberPublicKey, FederatedAuthorityData, INHERENT_IDENTIFIER, InherentError,
	MainchainMember,
};
pub use pallet::*;
use sidechain_domain::{MainchainAddress, PolicyId};
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::storage]
	/// Script address for managing Council members on Cardano
	pub type MainChainCouncilAddress<T: Config> = StorageValue<_, MainchainAddress, ValueQuery>;

	#[pallet::storage]
	/// Policy ID for Council members on Cardano
	pub type MainChainCouncilPolicyId<T: Config> = StorageValue<_, PolicyId, ValueQuery>;

	#[pallet::storage]
	/// Script address for managing Council members on Cardano
	pub type MainChainTechnicalCommitteeAddress<T: Config> =
		StorageValue<_, MainchainAddress, ValueQuery>;

	#[pallet::storage]
	/// Policy ID for Technical Committee members on Cardano
	pub type MainChainTechnicalCommitteePolicyId<T: Config> = StorageValue<_, PolicyId, ValueQuery>;

	#[pallet::storage]
	/// Mainchain member identifiers for Council members
	pub type CouncilMainchainMembers<T: Config> =
		StorageValue<_, BoundedVec<MainchainMember, T::CouncilMaxMembers>, ValueQuery>;

	#[pallet::storage]
	/// Mainchain member identifiers for Technical Committee members
	pub type TechnicalCommitteeMainchainMembers<T: Config> =
		StorageValue<_, BoundedVec<MainchainMember, T::TechnicalCommitteeMaxMembers>, ValueQuery>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The MAX number of members for the Council
		#[pallet::constant]
		type CouncilMaxMembers: Get<u32>;
		/// The MAX number of members for the Technical Committee
		#[pallet::constant]
		type TechnicalCommitteeMaxMembers: Get<u32>;
		/// The receiver of the signal for when the Council membership has changed.
		type CouncilMembershipHandler: ChangeMembers<Self::AccountId>
			+ SortedMembers<Self::AccountId>;
		/// The receiver of the signal for when the Technical Committee membership has changed.
		type TechnicalCommitteeMembershipHandler: ChangeMembers<Self::AccountId>
			+ SortedMembers<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub council_address: MainchainAddress,
		pub council_policy_id: PolicyId,
		pub technical_committee_address: MainchainAddress,
		pub technical_committee_policy_id: PolicyId,
		pub council_members_mainchain: Vec<MainchainMember>,
		pub technical_committee_members_mainchain: Vec<MainchainMember>,
		#[serde(skip)]
		pub _config: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MainChainCouncilAddress::<T>::set(self.council_address.clone());
			MainChainCouncilPolicyId::<T>::set(self.council_policy_id.clone());
			MainChainTechnicalCommitteeAddress::<T>::set(self.technical_committee_address.clone());
			MainChainTechnicalCommitteePolicyId::<T>::set(
				self.technical_committee_policy_id.clone(),
			);

			// Set mainchain members
			let council_mainchain_members: BoundedVec<MainchainMember, T::CouncilMaxMembers> = self
				.council_members_mainchain
				.clone()
				.try_into()
				.expect("Council mainchain members exceeds max members");
			CouncilMainchainMembers::<T>::set(council_mainchain_members);

			let technical_committee_mainchain_members: BoundedVec<
				MainchainMember,
				T::TechnicalCommitteeMaxMembers,
			> = self
				.technical_committee_members_mainchain
				.clone()
				.try_into()
				.expect("Technical committee mainchain members exceeds max members");
			TechnicalCommitteeMainchainMembers::<T>::set(technical_committee_mainchain_members);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Council members reset
		CouncilMembersReset {
			members: BoundedVec<T::AccountId, T::CouncilMaxMembers>,
			members_mainchain: BoundedVec<MainchainMember, T::CouncilMaxMembers>,
		},
		/// Technical Committee members reset
		TechnicalCommitteeMembersReset {
			members: BoundedVec<T::AccountId, T::TechnicalCommitteeMaxMembers>,
			members_mainchain: BoundedVec<MainchainMember, T::TechnicalCommitteeMaxMembers>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Too many members.
		TooManyMembers,
		/// Membership set is empty
		EmptyMembers,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((
		T::WeightInfo::reset_members(T::CouncilMaxMembers::get(), T::TechnicalCommitteeMaxMembers::get()),
		DispatchClass::Mandatory
		))]
		#[allow(clippy::useless_conversion)]
		pub fn reset_members(
			origin: OriginFor<T>,
			council_authorities: Vec<(T::AccountId, MainchainMember)>,
			technical_committee_authorities: Vec<(T::AccountId, MainchainMember)>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let (council_account_ids, council_mainchain_members): (Vec<_>, Vec<_>) =
				council_authorities.into_iter().unzip();
			let (technical_committee_account_ids, technical_committee_mainchain_members): (
				Vec<_>,
				Vec<_>,
			) = technical_committee_authorities.into_iter().unzip();

			// Prepare Council members
			let mut council_members: BoundedVec<T::AccountId, T::CouncilMaxMembers> =
				BoundedVec::try_from(council_account_ids.clone())
					.map_err(|_| Error::<T>::TooManyMembers)?;

			// Make sure an empty set of members is not allowed
			ensure!(!council_members.is_empty(), Error::<T>::EmptyMembers);
			council_members.sort();

			let council_current_members = T::CouncilMembershipHandler::sorted_members();

			let mut actual_weight = Weight::zero();

			let council_members_have_changed =
				council_current_members.as_slice() != council_members.as_slice();

			// Prepare Council mainchain members
			let council_mainchain_members: BoundedVec<MainchainMember, T::CouncilMaxMembers> =
				BoundedVec::try_from(council_mainchain_members.clone())
					.map_err(|_| Error::<T>::TooManyMembers)?;

			// Make sure an empty set of mainchain members is not allowed
			ensure!(!council_mainchain_members.is_empty(), Error::<T>::EmptyMembers);

			let council_current_mainchain_members = CouncilMainchainMembers::<T>::get();

			let council_mainchain_members_have_changed =
				council_current_mainchain_members != council_mainchain_members;

			// If Council membership has changed
			if council_members_have_changed {
				T::CouncilMembershipHandler::set_members_sorted(
					&council_members[..],
					&council_current_members,
				);
			}

			// If Council mainchain membership has changed
			if council_mainchain_members_have_changed {
				CouncilMainchainMembers::<T>::put(&council_mainchain_members);
			}

			let council_has_changed =
				council_members_have_changed || council_mainchain_members_have_changed;

			if council_has_changed {
				Self::deposit_event(Event::<T>::CouncilMembersReset {
					members: council_members,
					members_mainchain: council_mainchain_members,
				});

				actual_weight =
					actual_weight.saturating_add(T::WeightInfo::reset_members_only_council(
						council_account_ids.len() as u32,
						technical_committee_account_ids.len() as u32,
					));
			}

			// Prepare Technical Committee members
			let mut technical_committee_members: BoundedVec<
				T::AccountId,
				T::TechnicalCommitteeMaxMembers,
			> = BoundedVec::try_from(technical_committee_account_ids.clone())
				.map_err(|_| Error::<T>::TooManyMembers)?;

			// Make sure an empty set of members is not allowed
			ensure!(!technical_committee_members.is_empty(), Error::<T>::EmptyMembers);
			technical_committee_members.sort();

			let technical_committee_current_members =
				T::TechnicalCommitteeMembershipHandler::sorted_members();

			let technical_committee_members_have_changed = technical_committee_current_members
				.as_slice()
				!= technical_committee_members.as_slice();

			// Prepare technical committee mainchain members
			let technical_committee_mainchain_members: BoundedVec<
				MainchainMember,
				T::TechnicalCommitteeMaxMembers,
			> = BoundedVec::try_from(technical_committee_mainchain_members.clone())
				.map_err(|_| Error::<T>::TooManyMembers)?;

			// Make sure an empty set of mainchain members is not allowed
			ensure!(!technical_committee_mainchain_members.is_empty(), Error::<T>::EmptyMembers);

			let technical_committee_current_mainchain_members =
				TechnicalCommitteeMainchainMembers::<T>::get();

			let technical_committee_mainchain_members_have_changed =
				technical_committee_current_mainchain_members
					!= technical_committee_mainchain_members;

			// If Technical Committee membership has changed
			if technical_committee_members_have_changed {
				T::TechnicalCommitteeMembershipHandler::set_members_sorted(
					&technical_committee_members[..],
					&technical_committee_current_members,
				);
			}

			// If Technical Committee mainchain membership has changed
			if technical_committee_mainchain_members_have_changed {
				TechnicalCommitteeMainchainMembers::<T>::put(
					&technical_committee_mainchain_members,
				);
			}

			let technical_committee_has_changed = technical_committee_members_have_changed
				|| technical_committee_mainchain_members_have_changed;

			if technical_committee_has_changed {
				Self::deposit_event(Event::<T>::TechnicalCommitteeMembersReset {
					members: technical_committee_members,
					members_mainchain: technical_committee_mainchain_members,
				});

				actual_weight = actual_weight.saturating_add(
					T::WeightInfo::reset_members_only_technical_committee(
						council_account_ids.len() as u32,
						technical_committee_account_ids.len() as u32,
					),
				);
			}

			// If nothing changed, return correct weight
			if !council_has_changed && !technical_committee_has_changed {
				actual_weight = T::WeightInfo::reset_members_none(
					council_account_ids.len() as u32,
					technical_committee_account_ids.len() as u32,
				);
			}

			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::No })
		}

		/// Changes the mainchain address for the Council
		#[pallet::call_index(1)]
		#[pallet::weight((T::WeightInfo::set_council_address(), DispatchClass::Operational))]
		pub fn set_council_address(
			origin: OriginFor<T>,
			address: MainchainAddress,
		) -> DispatchResult {
			ensure_root(origin)?;
			MainChainCouncilAddress::<T>::set(address);

			Ok(())
		}

		/// Changes the mainchain address for the Technical Committee
		#[pallet::call_index(2)]
		#[pallet::weight((T::WeightInfo::set_technical_committee_address(), DispatchClass::Operational))]
		pub fn set_technical_committee_address(
			origin: OriginFor<T>,
			address: MainchainAddress,
		) -> DispatchResult {
			ensure_root(origin)?;
			MainChainTechnicalCommitteeAddress::<T>::set(address);

			Ok(())
		}

		/// Changes the mainchain policy id for the Council
		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::set_council_policy_id(), DispatchClass::Operational))]
		pub fn set_council_policy_id(origin: OriginFor<T>, policy_id: PolicyId) -> DispatchResult {
			ensure_root(origin)?;
			MainChainCouncilPolicyId::<T>::set(policy_id);

			Ok(())
		}

		/// Changes the mainchain policy id for the Technical Committee
		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::set_technical_committee_policy_id(), DispatchClass::Operational))]
		pub fn set_technical_committee_policy_id(
			origin: OriginFor<T>,
			policy_id: PolicyId,
		) -> DispatchResult {
			ensure_root(origin)?;
			MainChainTechnicalCommitteePolicyId::<T>::set(policy_id);

			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &sp_inherents::InherentData) -> Option<Self::Call> {
			// Extract and validate the federated authority data from inherent
			let fed_auth_data = Self::get_data_from_inherent_data(data).unwrap_or_default()?;

			let council_authorities =
				Self::decode_auth_accounts(fed_auth_data.council_authorities).ok()?;

			let technical_committee_authorities =
				Self::decode_auth_accounts(fed_auth_data.technical_committee_authorities).ok()?;

			if !council_authorities.is_empty() && !technical_committee_authorities.is_empty() {
				Some(Call::reset_members { council_authorities, technical_committee_authorities })
			} else {
				None
			}
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::reset_members { .. })
		}

		fn check_inherent(
			_call: &Self::Call,
			data: &sp_inherents::InherentData,
		) -> Result<(), Self::Error> {
			// Validate the federated authority data from inherent
			if let Some(fed_auth_data) = Self::get_data_from_inherent_data(data)? {
				let _ = Self::decode_auth_accounts(fed_auth_data.council_authorities)?;
				let _ = Self::decode_auth_accounts(fed_auth_data.technical_committee_authorities)?;
			}

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn get_data_from_inherent_data(
			data: &InherentData,
		) -> Result<Option<FederatedAuthorityData>, InherentError> {
			data.get_data::<FederatedAuthorityData>(&INHERENT_IDENTIFIER)
				.map_err(|_| InherentError::DecodeFailed)
		}

		/// Transform `Vec<(AuthorityMemberPublicKey, MainchainMember)>` into `Vec<(T::AccountId, MainchainMember)>`
		fn decode_auth_accounts(
			auth_data: Vec<(AuthorityMemberPublicKey, MainchainMember)>,
		) -> Result<Vec<(T::AccountId, MainchainMember)>, InherentError> {
			auth_data
				.into_iter()
				.map(|(key, mainchain_member)| {
					T::AccountId::decode(&mut &key.0[..])
						.map(|account_id| (account_id, mainchain_member))
						.map_err(|_| {
							log::error!(
								target: "federated-authority-observation",
								"Failed to decode authority key: {:?}",
								key.0
							);
							InherentError::DecodeFailed
						})
				})
				.collect::<Result<Vec<_>, _>>()
		}
	}
}

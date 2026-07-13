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

//! # Federated Authority Observation Pallet
//!
//! This pallet provides mechanisms for observing federated authority changes from the main chain.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
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

	#[pallet::storage]
	pub type InherentExecutedThisBlock<T: Config> = StorageValue<_, bool, ValueQuery>;

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
		CouncilMembersReset { members: Vec<T::AccountId>, members_mainchain: Vec<MainchainMember> },
		/// Technical Committee members reset
		TechnicalCommitteeMembersReset {
			members: Vec<T::AccountId>,
			members_mainchain: Vec<MainchainMember>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Membership set is empty
		EmptyMembers,
		/// Duplicate Members
		DuplicatedMembers,
		/// Only one inherent is allowed per block
		InherentAlreadyExecuted,
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
		#[pallet::weight((
		T::WeightInfo::reset_members(T::CouncilMaxMembers::get(), T::TechnicalCommitteeMaxMembers::get()),
		DispatchClass::Mandatory
		))]
		#[allow(clippy::useless_conversion)]
		#[allow(clippy::unwrap_in_result)] // bounded vec conversions are infallible; input already bounded
		pub fn reset_members(
			origin: OriginFor<T>,
			council_authorities: BoundedVec<(T::AccountId, MainchainMember), T::CouncilMaxMembers>,
			technical_committee_authorities: BoundedVec<
				(T::AccountId, MainchainMember),
				T::TechnicalCommitteeMaxMembers,
			>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			ensure!(!InherentExecutedThisBlock::<T>::get(), Error::<T>::InherentAlreadyExecuted);
			InherentExecutedThisBlock::<T>::put(true);

			// Sort pairs by AccountId before unzipping to preserve the association
			// between AccountId and MainchainMember
			let mut council_pairs: Vec<_> = council_authorities.clone().into_inner();
			council_pairs.sort_by(|a, b| a.0.cmp(&b.0));
			let (council_members, council_mainchain_members): (Vec<_>, Vec<_>) =
				council_pairs.into_iter().unzip();

			let mut tc_pairs: Vec<_> = technical_committee_authorities.clone().into_inner();
			tc_pairs.sort_by(|a, b| a.0.cmp(&b.0));
			let (technical_committee_members, technical_committee_mainchain_members): (
				Vec<_>,
				Vec<_>,
			) = tc_pairs.into_iter().unzip();

			let council_members_len = council_members.len() as u32;
			let technical_committee_members_len = technical_committee_members.len() as u32;

			// Helper closure to return early with no-op weight
			let early_return = || {
				let actual_weight = T::WeightInfo::reset_members_none(
					council_members_len,
					technical_committee_members_len,
				);
				Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::No })
			};

			// ========== VALIDATION PHASE ==========
			// All validations are done upfront before any state changes

			// ---- Council validation ----
			if council_members.is_empty() {
				log::error!(
					target: "federated-authority-observation",
					"Council members cannot be empty"
				);
				return early_return();
			}

			if Self::has_duplicated_members(council_authorities) {
				log::error!(
					target: "federated-authority-observation",
					"Council has duplicated members"
				);
			}

			if council_mainchain_members.is_empty() {
				log::error!(
					target: "federated-authority-observation",
					"Council mainchain members cannot be empty"
				);
				return early_return();
			}

			// ---- Technical Committee validation ----
			if technical_committee_members.is_empty() {
				log::error!(
					target: "federated-authority-observation",
					"Technical Committee members cannot be empty"
				);
				return early_return();
			}

			if Self::has_duplicated_members(technical_committee_authorities) {
				log::error!(
					target: "federated-authority-observation",
					"Technical Committee has duplicated members"
				);
			}

			if technical_committee_mainchain_members.is_empty() {
				log::error!(
					target: "federated-authority-observation",
					"Technical Committee mainchain members cannot be empty"
				);
				return early_return();
			}

			// ========== STATE CHANGE PHASE ==========
			// All validations passed, now apply state changes

			// council_members and technical_committee_members are already sorted
			// from the pre-unzip sort above

			let mut actual_weight = Weight::zero();

			// ---- Council state changes ----
			let council_current_members = T::CouncilMembershipHandler::sorted_members();
			let council_members_have_changed =
				council_current_members.as_slice() != council_members.as_slice();

			if council_members_have_changed {
				T::CouncilMembershipHandler::set_members_sorted(
					&council_members[..],
					&council_current_members,
				);
			}

			let council_current_mainchain_members = CouncilMainchainMembers::<T>::get();
			let council_mainchain_members_have_changed =
				council_current_mainchain_members != council_mainchain_members;

			let council_mainchain_members_bound: BoundedVec<MainchainMember, T::CouncilMaxMembers> =
				council_mainchain_members
					.clone()
					.try_into()
					.expect("call arg council_authorities is bounded; qed");

			if council_mainchain_members_have_changed {
				CouncilMainchainMembers::<T>::put(&council_mainchain_members_bound);
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
						council_members_len,
						technical_committee_members_len,
					));
			}

			// ---- Technical Committee state changes ----
			let technical_committee_current_members =
				T::TechnicalCommitteeMembershipHandler::sorted_members();
			let technical_committee_members_have_changed = technical_committee_current_members
				.as_slice()
				!= technical_committee_members.as_slice();

			if technical_committee_members_have_changed {
				T::TechnicalCommitteeMembershipHandler::set_members_sorted(
					&technical_committee_members[..],
					&technical_committee_current_members,
				);
			}

			let technical_committee_current_mainchain_members =
				TechnicalCommitteeMainchainMembers::<T>::get();
			let technical_committee_mainchain_members_have_changed =
				technical_committee_current_mainchain_members
					!= technical_committee_mainchain_members;

			let technical_committee_mainchain_members_bound: BoundedVec<
				MainchainMember,
				T::TechnicalCommitteeMaxMembers,
			> = technical_committee_mainchain_members
				.clone()
				.try_into()
				.expect("call arg technical_committee_authorities is bounded; qed");

			if technical_committee_mainchain_members_have_changed {
				TechnicalCommitteeMainchainMembers::<T>::put(
					&technical_committee_mainchain_members_bound,
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
						council_members_len,
						technical_committee_members_len,
					),
				);
			}

			// If nothing changed, return correct weight
			if !council_has_changed && !technical_committee_has_changed {
				actual_weight = T::WeightInfo::reset_members_none(
					council_members_len,
					technical_committee_members_len,
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

			let council_authorities: BoundedVec<
				(T::AccountId, MainchainMember),
				T::CouncilMaxMembers,
			> = Self::decode_auth_members(fed_auth_data.council_authorities.authorities).ok()?;

			let technical_committee_authorities: BoundedVec<
				(T::AccountId, MainchainMember),
				T::TechnicalCommitteeMaxMembers,
			> = Self::decode_auth_members(fed_auth_data.technical_committee_authorities.authorities)
				.ok()?;

			let has_empty_members =
				council_authorities.is_empty() || technical_committee_authorities.is_empty();
			let has_duplicated_members = Self::has_duplicated_members(council_authorities.clone())
				|| Self::has_duplicated_members(technical_committee_authorities.clone());

			if !has_empty_members && !has_duplicated_members {
				Some(Call::reset_members { council_authorities, technical_committee_authorities })
			} else {
				None
			}
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::reset_members { .. })
		}

		fn check_inherent(
			call: &Self::Call,
			data: &sp_inherents::InherentData,
		) -> Result<(), Self::Error> {
			// Validate the federated authority data from inherent
			if let Some(fed_auth_data) = Self::get_data_from_inherent_data(data)? {
				let council_authorities = Self::decode_auth_members::<T::CouncilMaxMembers>(
					fed_auth_data.council_authorities.authorities,
				)?;
				let technical_committee_authorities =
					Self::decode_auth_members::<T::TechnicalCommitteeMaxMembers>(
						fed_auth_data.technical_committee_authorities.authorities,
					)?;

				let (expected_council_authorities, expected_technical_committee_authorities) =
					match call {
						Call::reset_members {
							council_authorities,
							technical_committee_authorities,
						} => (council_authorities, technical_committee_authorities),
						_ => return Ok(()),
					};

				if council_authorities != *expected_council_authorities {
					log::error!(
						target: "federated-authority-observation",
						"Council Authorities mismatch - expected {:?}, got {:?}",
						*expected_council_authorities,
						council_authorities
					);
					return Err(Self::Error::CouncilMembersMismatch);
				}

				if technical_committee_authorities != *expected_technical_committee_authorities {
					log::error!(
						target: "federated-authority-observation",
						"Technical Committee mismatch - expected {:?}, got {:?}",
						*expected_technical_committee_authorities,
						technical_committee_authorities
					);
					return Err(Self::Error::TechnicalCommitteeMembersMismatch);
				}
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

		/// Transform `Vec<(AuthorityMemberPublicKey, MainchainMember)>` into `BoundedVec<(T::AccountId, MainchainMember), MAX>`
		fn decode_auth_members<MAX: Get<u32>>(
			auth_data: Vec<(AuthorityMemberPublicKey, MainchainMember)>,
		) -> Result<BoundedVec<(T::AccountId, MainchainMember), MAX>, InherentError> {
			let members = auth_data
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
				.collect::<Result<Vec<(T::AccountId, _)>, _>>()?;

			members.clone().try_into().map_err(|_| {
				log::error!(
					target: "federated-authority-observation",
					"Too many members: {:?}",
					members
				);
				InherentError::TooManyMembers
			})
		}

		/// Check if there are duplicated members in the set
		fn has_duplicated_members<S: Ord, M, MAX>(members: BoundedVec<(S, M), MAX>) -> bool {
			use alloc::collections::BTreeSet;
			// Only check Substrate/Midnight members
			let (members, _): (Vec<_>, Vec<_>) = members.into_iter().unzip();
			let members_vec_len = members.len();
			let members_set: BTreeSet<S> = members.into_iter().collect();

			members_set.len() != members_vec_len
		}
	}
}

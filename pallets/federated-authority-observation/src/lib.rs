//! # Federated Authority Observation Pallet
//!
//! This pallet provides mechanisms for observing federated authority changes from the main chain.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::ChangeMembers};
use frame_system::pallet_prelude::*;
use midnight_primitives_federated_authority_observation::{
	AuthorityMemberPublicKey, FederatedAuthorityData, INHERENT_IDENTIFIER, InherentError,
};
pub use pallet::*;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The MAX number of members for the Council
		#[pallet::constant]
		type CouncilMaxMembers: Get<u32>;
		/// The MAX number of members for the Technical Committee
		#[pallet::constant]
		type TechnicalCommitteeMaxMembers: Get<u32>;
		/// The receiver of the signal for when the Council membership has changed.
		type CouncilMembershipChanged: ChangeMembers<Self::AccountId>;
		/// The receiver of the signal for when the Technical Committee membership has changed.
		type TechnicalCommitteeMembershipChanged: ChangeMembers<Self::AccountId>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
	/// Storage for the list of council authority public keys
	#[pallet::storage]
	#[pallet::getter(fn council_authorities)]
	#[pallet::unbounded]
	pub type CouncilAuthorities<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, T::CouncilMaxMembers>, ValueQuery>;

	/// Storage for the list of technical committee authority public keys
	#[pallet::storage]
	#[pallet::getter(fn technical_committee_authorities)]
	#[pallet::unbounded]
	pub type TechnicalCommitteeAuthorities<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, T::TechnicalCommitteeMaxMembers>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Federated authority data updated
		FederatedAuthorityMembersReset {
			council_members: BoundedVec<T::AccountId, T::CouncilMaxMembers>,
			technical_committee_members: BoundedVec<T::AccountId, T::TechnicalCommitteeMaxMembers>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Too many members.
		TooManyMembers,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((
		0,
		DispatchClass::Mandatory
		))]
		pub fn reset_members(
			origin: OriginFor<T>,
			council_authorities: Vec<T::AccountId>,
			technical_committee_authorities: Vec<T::AccountId>,
		) -> DispatchResult {
			ensure_none(origin)?;

			// Reset Council members
			let mut council_members: BoundedVec<T::AccountId, T::CouncilMaxMembers> =
				BoundedVec::try_from(council_authorities)
					.map_err(|_| Error::<T>::TooManyMembers)?;
			council_members.sort();
			CouncilAuthorities::<T>::mutate(|m| {
				T::CouncilMembershipChanged::set_members_sorted(&council_members[..], m);
				*m = council_members.clone();
			});

			// Reset Technical Committee members
			let mut technical_committee_members: BoundedVec<
				T::AccountId,
				T::TechnicalCommitteeMaxMembers,
			> = BoundedVec::try_from(technical_committee_authorities)
				.map_err(|_| Error::<T>::TooManyMembers)?;
			technical_committee_members.sort();
			TechnicalCommitteeAuthorities::<T>::mutate(|m| {
				T::TechnicalCommitteeMembershipChanged::set_members_sorted(
					&technical_committee_members[..],
					m,
				);
				*m = technical_committee_members.clone();
			});

			Self::deposit_event(Event::<T>::FederatedAuthorityMembersReset {
				council_members,
				technical_committee_members,
			});

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
				Self::decode_auth_accounts(fed_auth_data.council_authorities, "council").ok()?;
			let technical_committee_authorities = Self::decode_auth_accounts(
				fed_auth_data.technical_committee_authorities,
				"technical committee",
			)
			.ok()?;

			Some(Call::reset_members { council_authorities, technical_committee_authorities })
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
				let _ = Self::decode_auth_accounts(fed_auth_data.council_authorities, "council")?;
				let _ = Self::decode_auth_accounts(
					fed_auth_data.technical_committee_authorities,
					"technical committee",
				)?;
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

		/// Transform AuthorityMemberPublicKey into T::AccountId here
		fn decode_auth_accounts(
			auth_data: Vec<AuthorityMemberPublicKey>,
			body: &'static str,
		) -> Result<Vec<T::AccountId>, InherentError> {
			auth_data
				.into_iter()
				.map(|key| {
					T::AccountId::decode(&mut &key.0[..]).map_err(|_| {
						log::error!(
							target: "federated-authority-observation",
							"Failed to decode {body:?} authority key: {:?}",
							key.0
						);
						InherentError::DecodeFailed
					})
				})
				.collect::<Result<Vec<_>, _>>()
		}
	}
}

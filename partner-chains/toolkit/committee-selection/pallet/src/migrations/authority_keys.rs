//! Implements a re-usable migration for the authority keys type
//!
//! # Usage
//!
//! **Important**: This migration assumes that the runtime is using [pallet_session] and will
//! migrate that pallet's key storage as well.
//!
//! Authority keys migration is done by adding [AuthorityKeysMigration] to the runtime
//! migrations as part of the runtime upgrade that will change the key type.
//!
//! Preserve the old authority keys type and the old committee member type. Implement
//! [UpgradeAuthorityKeys] for the old keys type and [UpgradeCommitteeMember] for the old
//! committee member type.
//!
//! For example, if a chain that originally used Aura and Grandpa keys is being upgraded to
//! also use Beefy, the definitions could look like this:
//!
//! ```rust,ignore
//! impl_opaque_keys! {
//! 	pub struct LegacyAuthorityKeys {
//! 		pub aura: Aura,
//! 		pub grandpa: Grandpa,
//! 	}
//! }
//!
//! type LegacyCommitteeMember = authority_selection_inherents::CommitteeMember<CrossChainPublic, LegacyAuthorityKeys>;
//!
//! impl UpgradeAuthorityKeys<SessionKeys> for LegacyAuthorityKeys {
//! 	fn upgrade(self) -> SessionKeys {
//! 		SessionKeys {
//! 			aura: self.aura,
//! 			grandpa: self.grandpa,
//! 			beefy: ecdsa::Public::default().into(),
//! 		}
//! 	}
//! }
//!
//! impl UpgradeCommitteeMember<Runtime> for LegacyCommitteeMember {
//! 	fn upgrade(self) -> <Runtime as pallet_session_validator_management::Config>::CommitteeMember {
//! 		self.map_authority_keys(|old_keys| UpgradeAuthorityKeys::<SessionKeys>::upgrade(old_keys))
//! 	}
//! }
//! ```
//!
//! After implementing both traits, the migration can be added to the runtime's migration set:
//! ```rust,ignore
//! pub type Migrations = (
//! 	AuthorityKeysMigration<Runtime, LegacyCommitteeMember, LegacyAuthorityKeys, 0, 1>,
//! 	// ...other migrations
//! );
//! ```
//!
//! Note that [AuthorityKeysMigration] is parametrized by the session keys versions from which
//! and to which it migrates, to guarantee idempotency. Current session keys version can be
//! obtained by reading the [AuthorityKeysVersion] storage and by default starts as 0.

extern crate alloc;

use crate::*;
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::traits::OnRuntimeUpgrade;
use sp_core::Get;
use sp_runtime::BoundedVec;
use sp_runtime::traits::{Member, OpaqueKeys};

/// Infallible cast from old to current `T::AuthorityKeys`, used for session storage migration
pub trait UpgradeAuthorityKeys<NewAuthorityKeys> {
	/// Should cast the old session keys type to the new one
	fn upgrade(self) -> NewAuthorityKeys;
}

/// Infallible cast from old to current `T::CommitteeMember`, used for committee storage migration
pub trait UpgradeCommitteeMember<T: crate::Config> {
	/// Should cast the old committee member type to the new one
	fn upgrade(self) -> T::CommitteeMember;
}

/// Migrates existing committee members data in storage to use new type `CommitteeMember` and
/// the `pallet_session` keys storage to use new type `AuthorityKeys`.
///
/// This migration is versioned and will only be applied when on-chain session keys version
/// as read from [AuthorityKeysVersion] storage is equal to `FROM_VERSION` and will
/// set the version to `TO_VERSION`.
///
/// **Important**: This migration assumes that the runtime is using [pallet_session] and will
/// migrate that pallet's key storage as well.
pub struct AuthorityKeysMigration<
	T,
	OldCommitteeMember,
	OldAuthorityKeys,
	const FROM_VERSION: u32,
	const TO_VERSION: u32,
> where
	T: crate::Config,
	OldCommitteeMember: UpgradeCommitteeMember<T> + Member + Decode + Clone,
	OldAuthorityKeys: UpgradeAuthorityKeys<T::AuthorityKeys> + Member + Decode + Clone,
{
	_phantom: PhantomData<(T, OldCommitteeMember, OldAuthorityKeys)>,
}

impl<
		T: crate::Config,
		OldCommitteeMember,
		OldAuthorityKeys,
		const FROM_VERSION: u32,
		const TO_VERSION: u32,
	> AuthorityKeysMigration<T, OldCommitteeMember, OldAuthorityKeys, FROM_VERSION, TO_VERSION>
where
	OldCommitteeMember: UpgradeCommitteeMember<T> + Member + Decode + Clone,
	OldAuthorityKeys: UpgradeAuthorityKeys<T::AuthorityKeys> + Member + Decode + Clone,
{
	/// Casts a [BoundedVec] of old committee member values to the new ones
	fn upgrade_bounded_vec(
		old: BoundedVec<OldCommitteeMember, T::MaxValidators>,
	) -> BoundedVec<T::CommitteeMember, T::MaxValidators> {
		BoundedVec::truncate_from(old.into_iter().map(|old| old.upgrade()).collect::<Vec<_>>())
	}

	/// Casts old committee member values in a [CommitteeInfo] into new ones
	fn upgrade_committee_info(
		old: CommitteeInfo<T::ScEpochNumber, OldCommitteeMember, T::MaxValidators>,
	) -> CommitteeInfo<T::ScEpochNumber, T::CommitteeMember, T::MaxValidators> {
		CommitteeInfo { epoch: old.epoch, committee: Self::upgrade_bounded_vec(old.committee) }
	}
}

impl<T, OldCommitteeMember, OldAuthorityKeys, const FROM_VERSION: u32, const TO_VERSION: u32>
	OnRuntimeUpgrade
	for AuthorityKeysMigration<T, OldCommitteeMember, OldAuthorityKeys, FROM_VERSION, TO_VERSION>
where
	T: crate::Config + pallet_session::Config<Keys = <T as crate::Config>::AuthorityKeys>,
	OldCommitteeMember: UpgradeCommitteeMember<T> + Member + Decode + Clone,
	OldAuthorityKeys: UpgradeAuthorityKeys<T::AuthorityKeys> + OpaqueKeys + Member + Decode + Clone,
{
	fn on_runtime_upgrade() -> sp_runtime::Weight {
		let current_version = crate::AuthorityKeysVersion::<T>::get();

		let mut weight = T::DbWeight::get().reads_writes(1, 0);

		if TO_VERSION <= current_version {
			log::info!(
				"🚚 AuthorityKeysMigration {FROM_VERSION}->{TO_VERSION} can be removed; authority keys storage is already at version {current_version}."
			);
			return weight;
		}
		if current_version != FROM_VERSION {
			log::warn!(
				"🚚 AuthorityKeysMigration {FROM_VERSION}->{TO_VERSION} can not be applied to authority keys storage at version {current_version}."
			);
			return weight;
		}

		if let Some(new) = CurrentCommittee::<T>::translate::<
			CommitteeInfo<T::ScEpochNumber, OldCommitteeMember, T::MaxValidators>,
			_,
		>(|old| old.map(Self::upgrade_committee_info))
		.expect("Decoding of the old value must succeed")
		{
			CurrentCommittee::<T>::put(new);
			log::info!("🚚️ Migrated current committee storage to version {TO_VERSION}");
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
		}

		if let Some(new) = NextCommittee::<T>::translate::<
			CommitteeInfo<T::ScEpochNumber, OldCommitteeMember, T::MaxValidators>,
			_,
		>(|old| old.map(Self::upgrade_committee_info))
		.expect("Decoding of the old value must succeed")
		{
			NextCommittee::<T>::put(new);
			log::info!("🚚️ Migrated new committee storage to version {TO_VERSION}");
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
		}

		pallet_session::Pallet::<T>::upgrade_keys(|_id, old_keys| {
			OldAuthorityKeys::upgrade(old_keys)
		});
		weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));
		log::info!("🚚️ Migrated keys in pallet_session to version {TO_VERSION}");

		crate::AuthorityKeysVersion::<T>::set(TO_VERSION);
		weight = weight.saturating_add(T::DbWeight::get().reads_writes(0, 1));

		weight
	}
}

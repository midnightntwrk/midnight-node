//! Migrates `session-validator-management` storage from v1 to v2.
//!
//! This one migration adds [`crate::QueuedCommittee`] and changes the `AuthorityKeys` shape in
//! committee and [pallet_session] storage. V1 chains do not have `QueuedCommittee`; after
//! translating `CurrentCommittee`, the migration copies it to `QueuedCommittee` because the v1
//! session integration treated the current committee as both active and queued.
//!
//! # Usage
//!
//! For example, for a chain adding Beefy to Aura + Grandpa:
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
//! impl From<LegacyAuthorityKeys> for SessionKeys {
//! 	fn from(old: LegacyAuthorityKeys) -> Self {
//! 		let mut placeholder = [0u8; 33];
//! 		placeholder[1..].copy_from_slice(&old.aura.to_raw_vec());
//! 		SessionKeys { aura: old.aura, grandpa: old.grandpa, beefy: ecdsa::Public::from_raw(placeholder).into() }
//! 	}
//! }
//!
//! impl UpgradeCommitteeMember<Runtime> for LegacyCommitteeMember {
//! 	fn upgrade(self) -> <Runtime as pallet_session_validator_management::Config>::CommitteeMember {
//! 		self.map_authority_keys(Into::into)
//! 	}
//! }
//! ```
//!
//! Wired into `Runtime`'s `SingleBlockMigrations`:
//! ```rust,ignore
//! type SingleBlockMigrations = (
//! 	pallet_session_validator_management::migrations::v2::V1ToV2Migration<
//! 		Runtime,
//! 		LegacyCommitteeMember,
//! 		LegacyAuthorityKeys,
//! 	>,
//! 	// ...other migrations
//! );
//! ```
//!
//! The versioned gate makes the migration a no-op on chains already at v2, including
//! fresh-genesis chains with new-shaped bytes.

#[cfg(feature = "try-runtime")]
extern crate alloc;

use core::marker::PhantomData;
use frame_support::migrations::VersionedMigration;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use parity_scale_codec::{Decode, Encode};
use sp_core::Get;
use sp_runtime::BoundedVec;
use sp_runtime::traits::{Member, OpaqueKeys};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

use crate::CommitteeMember as CommitteeMemberT;
use crate::pallet::CommitteeInfo;

/// Infallible cast from old to current `T::CommitteeMember`, used for committee storage
/// migration. Not a plain `From`/`Into` because orphan rules block impls between two
/// instantiations of the foreign `CommitteeMember` type.
pub trait UpgradeCommitteeMember<T: crate::Config> {
	/// Should cast the old committee member type to the new one
	fn upgrade(self) -> T::CommitteeMember;
}

/// Combined v1-to-v2 committee and session-key migration.
pub type V1ToV2Migration<T, OldCommitteeMember, OldAuthorityKeys> = VersionedMigration<
	1,
	2,
	InnerMigrateV1ToV2<T, OldCommitteeMember, OldAuthorityKeys>,
	crate::pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Helper type used internally for migration. Use [`V1ToV2Migration`] in your runtime.
pub struct InnerMigrateV1ToV2<T, OldCommitteeMember, OldAuthorityKeys>(
	PhantomData<(T, OldCommitteeMember, OldAuthorityKeys)>,
);

type OldCommitteeInfo<T, OldCommitteeMember> = CommitteeInfo<
	<T as crate::Config>::ScEpochNumber,
	OldCommitteeMember,
	<T as crate::Config>::MaxValidators,
>;

impl<T, OldCommitteeMember, OldAuthorityKeys> UncheckedOnRuntimeUpgrade
	for InnerMigrateV1ToV2<T, OldCommitteeMember, OldAuthorityKeys>
where
	T: crate::Config + pallet_session::Config<Keys = <T as crate::Config>::AuthorityKeys>,
	OldCommitteeMember: UpgradeCommitteeMember<T>
		+ Member
		+ Decode
		+ Encode
		+ Clone
		+ CommitteeMemberT<AuthorityId = T::AuthorityId, AuthorityKeys = OldAuthorityKeys>,
	OldAuthorityKeys: Member + Decode + Encode + Clone + OpaqueKeys + Into<T::AuthorityKeys>,
	T::AuthorityKeys: OpaqueKeys,
{
	fn on_runtime_upgrade() -> sp_runtime::Weight {
		// The translations and current-committee read are unconditional. The queue seed always writes.
		let mut weight = T::DbWeight::get().reads_writes(3, 1);

		let current_translated =
			crate::CurrentCommittee::<T>::translate::<OldCommitteeInfo<T, OldCommitteeMember>, _>(
				|old| old.map(upgrade_committee_info::<T, OldCommitteeMember>),
			)
			.expect("Decoding of the old value must succeed");
		if current_translated.is_some() {
			weight = weight.saturating_add(T::DbWeight::get().writes(1));
		}

		let next_translated = crate::NextCommittee::<T>::translate::<
			OldCommitteeInfo<T, OldCommitteeMember>,
			_,
		>(|old| old.map(upgrade_committee_info::<T, OldCommitteeMember>))
		.expect("Decoding of the old value must succeed");
		if next_translated.is_some() {
			weight = weight.saturating_add(T::DbWeight::get().writes(1));
		}

		crate::QueuedCommittee::<T>::put(crate::CurrentCommittee::<T>::get());

		// `upgrade_keys` translates the entire `NextKeys` map (1 read + 1 write per entry) and
		// rewrites `KeyOwner` for every old/new key type per entry (pure writes). `QueuedKeys` is
		// a single `StorageValue`, translated once.
		//
		// Count `NextKeys` entries via `iter_keys` (no value decode) so the weight is correct
		// even when on-chain bytes still use the pre-upgrade `OldAuthorityKeys` shape.
		// `register_committee_keys` only adds keys for committee members and never removes them
		// when a validator rotates out, so the map may contain stale entries beyond the
		// current/next committee union.
		let validators = pallet_session::NextKeys::<T>::iter_keys().count() as u64;
		pallet_session::Pallet::<T>::upgrade_keys(|_id, old_keys: OldAuthorityKeys| {
			old_keys.into()
		});
		let old_key_types = OldAuthorityKeys::key_ids().len() as u64;
		let new_key_types = T::AuthorityKeys::key_ids().len() as u64;
		weight = weight.saturating_add(T::DbWeight::get().reads_writes(
			// One read per entry to count, then one per entry again during `translate`, plus
			// `QueuedKeys`.
			2 * validators + 1,
			validators * (1 + old_key_types + new_key_types) + 1,
		));

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// `CurrentCommittee`/`NextCommittee` `.get()` decodes as `T::CommitteeMember`, i.e. the
		// post-upgrade shape, but the on-chain bytes are still `OldCommitteeMember`. The same
		// applies to `pallet_session`'s
		// `NextKeys`/`QueuedKeys`, which still hold `OldAuthorityKeys` bytes, so all values are
		// read through `unhashed` with the old types. `QueuedCommittee` does not exist at v1.
		let current: OldCommitteeInfo<T, OldCommitteeMember> =
			frame_support::storage::unhashed::get_or_default(
				&crate::CurrentCommittee::<T>::hashed_key(),
			);
		let next: Option<OldCommitteeInfo<T, OldCommitteeMember>> =
			frame_support::storage::unhashed::get(&crate::NextCommittee::<T>::hashed_key());

		let next_keys: Vec<(T::ValidatorId, OldAuthorityKeys)> =
			pallet_session::NextKeys::<T>::iter_keys()
				.map(|validator| {
					let old_keys: OldAuthorityKeys = frame_support::storage::unhashed::get(
						&pallet_session::NextKeys::<T>::hashed_key_for(&validator),
					)
					.ok_or(sp_runtime::TryRuntimeError::Other(
						"session NextKeys entries must decode with the old keys type",
					))?;
					Ok((validator, old_keys))
				})
				.collect::<Result<_, sp_runtime::TryRuntimeError>>()?;

		// `QueuedKeys` is a `ValueQuery` storage: absent means empty.
		let queued_keys: Vec<(T::ValidatorId, OldAuthorityKeys)> =
			frame_support::storage::unhashed::get_or_default(
				&pallet_session::QueuedKeys::<T>::hashed_key(),
			);

		Ok((current, next, next_keys, queued_keys).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;

		let (old_current, old_next, old_next_keys, old_queued_keys): (
			OldCommitteeInfo<T, OldCommitteeMember>,
			Option<OldCommitteeInfo<T, OldCommitteeMember>>,
			Vec<(T::ValidatorId, OldAuthorityKeys)>,
			Vec<(T::ValidatorId, OldAuthorityKeys)>,
		) = Decode::decode(&mut state.as_slice()).map_err(|_| {
			sp_runtime::TryRuntimeError::Other("Previously encoded state should be decodable")
		})?;

		let new_current = crate::CurrentCommittee::<T>::get();
		ensure!(old_current.epoch == new_current.epoch, "current epoch should be preserved");
		ensure!(
			committee_fingerprint::<T, OldCommitteeMember>(&old_current.committee)
				== new_committee_fingerprint::<T>(&new_current.committee),
			"current committee membership should be preserved"
		);

		let new_queued = crate::QueuedCommittee::<T>::get();
		ensure!(
			new_queued.encode() == new_current.encode(),
			"queued committee should be seeded from current committee"
		);

		let new_next = crate::NextCommittee::<T>::get();
		ensure!(
			old_next.is_some() == new_next.is_some(),
			"next committee presence should be preserved"
		);
		if let (Some(old_next), Some(new_next)) = (old_next, new_next) {
			ensure!(old_next.epoch == new_next.epoch, "next epoch should be preserved");
			ensure!(
				committee_fingerprint::<T, OldCommitteeMember>(&old_next.committee)
					== new_committee_fingerprint::<T>(&new_next.committee),
				"next committee membership should be preserved"
			);
		}

		ensure!(
			pallet_session::NextKeys::<T>::iter_keys().count() == old_next_keys.len(),
			"session NextKeys entry count should be preserved"
		);
		for (validator, old_keys) in old_next_keys {
			let expected_keys: T::AuthorityKeys = old_keys.into();
			ensure!(
				pallet_session::NextKeys::<T>::get(&validator) == Some(expected_keys.clone()),
				"session NextKeys should be upgraded in place"
			);
			// Covers every key type, added ones included; a key shared between validators
			// leaves `KeyOwner` pointing at only one of them.
			for key_type in T::AuthorityKeys::key_ids() {
				ensure!(
					pallet_session::KeyOwner::<T>::get((
						*key_type,
						expected_keys.get_raw(*key_type).to_vec()
					)) == Some(validator.clone()),
					"KeyOwner should map each upgraded key back to its validator"
				);
			}
		}

		let expected_queued_keys: Vec<(T::ValidatorId, T::AuthorityKeys)> =
			old_queued_keys.into_iter().map(|(v, keys)| (v, keys.into())).collect();
		ensure!(
			pallet_session::QueuedKeys::<T>::get() == expected_queued_keys,
			"session QueuedKeys should be upgraded in place"
		);
		ensure!(
			frame_support::traits::StorageVersion::get::<crate::Pallet<T>>()
				== frame_support::traits::StorageVersion::new(2),
			"on-chain storage version should be 2"
		);

		Ok(())
	}
}

/// Maps an old committee to `(authority_id, upgraded_authority_keys)` pairs, for comparison
/// against the post-upgrade committee in [`InnerMigrateV1ToV2::post_upgrade`].
#[cfg(feature = "try-runtime")]
fn committee_fingerprint<T, OldCommitteeMember>(
	committee: &BoundedVec<OldCommitteeMember, T::MaxValidators>,
) -> Vec<(T::AuthorityId, T::AuthorityKeys)>
where
	T: crate::Config,
	OldCommitteeMember: Clone + CommitteeMemberT<AuthorityId = T::AuthorityId>,
	<OldCommitteeMember as CommitteeMemberT>::AuthorityKeys: Into<T::AuthorityKeys>,
{
	committee
		.iter()
		.cloned()
		.map(|m| (m.authority_id(), m.authority_keys().into()))
		.collect()
}

/// Maps the post-upgrade committee to `(authority_id, authority_keys)` pairs, for comparison
/// against [`committee_fingerprint`].
#[cfg(feature = "try-runtime")]
fn new_committee_fingerprint<T>(
	committee: &BoundedVec<T::CommitteeMember, T::MaxValidators>,
) -> Vec<(T::AuthorityId, T::AuthorityKeys)>
where
	T: crate::Config,
{
	committee
		.iter()
		.cloned()
		.map(|m| (m.authority_id(), m.authority_keys()))
		.collect()
}

fn upgrade_committee_info<T, OldCommitteeMember>(
	old: CommitteeInfo<T::ScEpochNumber, OldCommitteeMember, T::MaxValidators>,
) -> CommitteeInfo<T::ScEpochNumber, T::CommitteeMember, T::MaxValidators>
where
	T: crate::Config,
	OldCommitteeMember: Clone + UpgradeCommitteeMember<T>,
{
	CommitteeInfo {
		epoch: old.epoch,
		committee: BoundedVec::truncate_from(
			old.committee.into_iter().map(UpgradeCommitteeMember::upgrade).collect(),
		),
	}
}

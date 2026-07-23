//! Implements a re-usable storage migration for the `AuthorityKeys` type.
//!
//! **Important**: This migration assumes that the runtime is using [pallet_session] and will
//! migrate that pallet's key storage as well.
//!
//! Unlike [`super::v1::LegacyToV1Migration`], which is a one-shot migration fixed to `0 -> 1`,
//! [`AuthorityKeysMigration`] is generic over the old committee member/authority keys types and
//! over the `FROM`/`TO` storage versions, so it can be reused whenever a Partner Chain changes
//! the shape of its `AuthorityKeys`.
//!
//! # Usage
//!
//! Preserve the old authority keys type and implement [`Into`] for it, targeting the new
//! `T::AuthorityKeys` — this works directly since session key types are always locally defined
//! (via [`sp_runtime::impl_opaque_keys`]) in the consuming runtime. The old committee member type
//! needs [`UpgradeCommitteeMember`] instead of a plain `Into`/`From`: most runtimes reuse
//! `authority_selection_inherents::CommitteeMember<AuthorityId, AuthorityKeys>` directly as
//! `T::CommitteeMember`, and implementing `From`/`Into` between two instantiations of a type
//! that's foreign to the runtime crate would violate Rust's orphan rules.
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
//! impl From<LegacyAuthorityKeys> for SessionKeys {
//! 	fn from(old: LegacyAuthorityKeys) -> Self {
//! 		SessionKeys { aura: old.aura, grandpa: old.grandpa, beefy: ecdsa::Public::default().into() }
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
//! After implementing both, wire the migration into `Runtime`'s `SingleBlockMigrations`:
//! ```rust,ignore
//! type SingleBlockMigrations = (
//! 	pallet_session_validator_management::migrations::authority_keys::AuthorityKeysMigration<
//! 		Runtime,
//! 		LegacyCommitteeMember,
//! 		LegacyAuthorityKeys,
//! 		2, // pallet on-chain storage version before the migration (at wiring time)
//! 		3, // pallet on-chain storage version after the migration (at wiring time)
//! 	>,
//! 	// ...other migrations
//! );
//! ```
//!
//! **Important**: `FROM`/`TO` must reflect the pallet's on-chain storage version when this
//! migration is actually wired in, not the version declared in code at scaffold time. Remove the
//! migration from `SingleBlockMigrations` once all live networks have upgraded. If it remains wired
//! while a genesis reset leaves on-chain storage at `FROM` but stores new-shaped committee bytes,
//! the next upgrade will run `translate` with the old type and panic.

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
/// migration. See the module docs for why this can't just be a plain `From`/`Into` impl.
pub trait UpgradeCommitteeMember<T: crate::Config> {
	/// Should cast the old committee member type to the new one
	fn upgrade(self) -> T::CommitteeMember;
}

/// [`VersionedMigration`] parametrized for a Partner Chain's `AuthorityKeys` change.
///
/// `FROM`/`TO` are the pallet's on-chain storage versions before/after the migration when it is
/// wired in (see [`crate::pallet::Pallet`]'s `#[pallet::storage_version]` at that time).
pub type AuthorityKeysMigration<
	T,
	OldCommitteeMember,
	OldAuthorityKeys,
	const FROM: u16,
	const TO: u16,
> = VersionedMigration<
	FROM,
	TO,
	InnerMigrateAuthorityKeys<T, OldCommitteeMember, OldAuthorityKeys>,
	crate::pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Helper type used internally for migration. Use [`AuthorityKeysMigration`] in your runtime instead.
pub struct InnerMigrateAuthorityKeys<T, OldCommitteeMember, OldAuthorityKeys>(
	PhantomData<(T, OldCommitteeMember, OldAuthorityKeys)>,
);

type OldCommitteeInfo<T, OldCommitteeMember> = CommitteeInfo<
	<T as crate::Config>::ScEpochNumber,
	OldCommitteeMember,
	<T as crate::Config>::MaxValidators,
>;

impl<T, OldCommitteeMember, OldAuthorityKeys> UncheckedOnRuntimeUpgrade
	for InnerMigrateAuthorityKeys<T, OldCommitteeMember, OldAuthorityKeys>
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
		// `translate` always reads the value; it writes only when the value was present.
		let mut weight = T::DbWeight::get().reads(3);

		let current_translated =
			crate::CurrentCommittee::<T>::translate::<OldCommitteeInfo<T, OldCommitteeMember>, _>(
				|old| old.map(upgrade_committee_info::<T, OldCommitteeMember>),
			)
			.expect("Decoding of the old value must succeed");
		if current_translated.is_some() {
			weight = weight.saturating_add(T::DbWeight::get().writes(1));
		}

		let queued_translated = crate::QueuedCommittee::<T>::translate::<
			OldCommitteeInfo<T, OldCommitteeMember>,
			_,
		>(|old| old.map(upgrade_committee_info::<T, OldCommitteeMember>))
		.expect("Decoding of the old value must succeed");
		if queued_translated.is_some() {
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
		// `CurrentCommittee`/`QueuedCommittee`/`NextCommittee` `.get()` decodes as
		// `T::CommitteeMember`, i.e. the post-upgrade shape — but at this point the on-chain
		// bytes are still `OldCommitteeMember`. The same applies to `pallet_session`'s
		// `NextKeys`/`QueuedKeys`, which still hold `OldAuthorityKeys` bytes, so all values are
		// read through `unhashed` with the old types.
		let current: OldCommitteeInfo<T, OldCommitteeMember> =
			frame_support::storage::unhashed::get_or_default(
				&crate::CurrentCommittee::<T>::hashed_key(),
			);
		let queued: OldCommitteeInfo<T, OldCommitteeMember> =
			frame_support::storage::unhashed::get_or_default(
				&crate::QueuedCommittee::<T>::hashed_key(),
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

		Ok((current, queued, next, next_keys, queued_keys).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;

		let (old_current, old_queued, old_next, old_next_keys, old_queued_keys): (
			OldCommitteeInfo<T, OldCommitteeMember>,
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
		ensure!(old_queued.epoch == new_queued.epoch, "queued epoch should be preserved");
		ensure!(
			committee_fingerprint::<T, OldCommitteeMember>(&old_queued.committee)
				== new_committee_fingerprint::<T>(&new_queued.committee),
			"queued committee membership should be preserved"
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
		// Only key types present in both old and new keys are checked in `KeyOwner`: keys of a
		// newly added type may be identical across validators (e.g. a shared default), in which
		// case `upgrade_keys` leaves the entry pointing at whichever validator was processed last.
		let common_key_types: Vec<_> = T::AuthorityKeys::key_ids()
			.iter()
			.filter(|id| OldAuthorityKeys::key_ids().contains(id))
			.collect();
		for (validator, old_keys) in old_next_keys {
			let expected_keys: T::AuthorityKeys = old_keys.into();
			ensure!(
				pallet_session::NextKeys::<T>::get(&validator) == Some(expected_keys.clone()),
				"session NextKeys should be upgraded in place"
			);
			for key_type in &common_key_types {
				ensure!(
					pallet_session::KeyOwner::<T>::get((
						**key_type,
						expected_keys.get_raw(**key_type).to_vec()
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

		Ok(())
	}
}

/// Maps an old committee to `(authority_id, upgraded_authority_keys)` pairs, for comparison
/// against the post-upgrade committee in [`InnerMigrateAuthorityKeys::post_upgrade`].
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

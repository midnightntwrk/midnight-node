//! Implements storage migration of the `session-validator-management` pallet from v1 to v2.
//!
//! V2 adds the [`crate::QueuedCommittee`] storage, tracking the committee handed to
//! `pallet_session` at the last rotation, which becomes the effective validator set (and is
//! promoted to [`crate::CurrentCommittee`]) at the next rotation.
use frame_support::traits::UncheckedOnRuntimeUpgrade;
#[cfg(feature = "try-runtime")]
extern crate alloc;
#[cfg(feature = "try-runtime")]
use {alloc::vec::Vec, parity_scale_codec::Encode};

/// [frame_support::migrations::VersionedMigration] parametrized for v1 to v2 migration.
pub type V1ToV2Migration<T> = frame_support::migrations::VersionedMigration<
	1, // The migration will only execute when the on-chain storage version is 1
	2, // The on-chain storage version will be set to 2 after the migration is complete
	InnerMigrateV1ToV2<T>,
	crate::pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Helper type used internally for migration. Use [V1ToV2Migration] in your runtime instead.
pub struct InnerMigrateV1ToV2<T: crate::Config>(core::marker::PhantomData<T>);

impl<T: crate::pallet::Config> UncheckedOnRuntimeUpgrade for InnerMigrateV1ToV2<T> {
	fn on_runtime_upgrade() -> sp_runtime::Weight {
		use sp_core::Get;

		// V1 chains ran a session integration that applied committees immediately, so at
		// upgrade time `CurrentCommittee` is both the active and the queued validator set of
		// the session machinery.
		crate::QueuedCommittee::<T>::put(crate::CurrentCommittee::<T>::get());

		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(crate::CurrentCommittee::<T>::get().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;
		use parity_scale_codec::Decode;

		let current_committee_pre: crate::pallet::CommitteeInfo<
			T::ScEpochNumber,
			T::CommitteeMember,
			T::MaxValidators,
		> = Decode::decode(&mut state.as_slice()).map_err(|_| {
			sp_runtime::TryRuntimeError::Other("Previously encoded state should be decodable")
		})?;

		let queued_committee = crate::QueuedCommittee::<T>::get();

		ensure!(
			queued_committee.encode() == current_committee_pre.encode(),
			"queued committee should be initialized to the pre-upgrade current committee"
		);

		Ok(())
	}
}

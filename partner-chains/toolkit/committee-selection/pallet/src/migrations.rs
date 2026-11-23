//! Storage migrations of the committee selection pallet

/// The committees as stored at storage version 2, before they recorded a Cardano epoch
// `storage_alias` generates undocumented prefix types.
#[allow(missing_docs)]
pub mod v2 {
	use crate::{Config, Pallet};
	use frame_support::{BoundedVec, CloneNoBound, pallet_prelude::OptionQuery, storage_alias};
	use parity_scale_codec::{Decode, Encode};

	/// [crate::CommitteeInfo] as stored at version 2, without the Cardano epoch
	#[derive(CloneNoBound, Encode, Decode)]
	pub struct CommitteeInfoV2<ScEpochNumber: Clone, CommitteeMember: Clone, MaxValidators> {
		/// Epoch number the committee is selected for.
		pub epoch: ScEpochNumber,
		/// List of committee members.
		pub committee: BoundedVec<CommitteeMember, MaxValidators>,
	}

	impl<ScEpochNumber: Clone + Default, CommitteeMember: Clone, MaxValidators> Default
		for CommitteeInfoV2<ScEpochNumber, CommitteeMember, MaxValidators>
	{
		fn default() -> Self {
			Self { epoch: Default::default(), committee: BoundedVec::new() }
		}
	}

	/// [CommitteeInfoV2] of the runtime `T`
	pub type CommitteeInfoV2Of<T> = CommitteeInfoV2<
		<T as Config>::ScEpochNumber,
		<T as Config>::CommitteeMember,
		<T as Config>::MaxValidators,
	>;

	/// [crate::CurrentCommittee] at version 2
	#[storage_alias]
	pub type CurrentCommittee<T: Config> =
		StorageValue<Pallet<T>, CommitteeInfoV2Of<T>, OptionQuery>;

	/// [crate::QueuedCommittee] at version 2
	#[storage_alias]
	pub type QueuedCommittee<T: Config> =
		StorageValue<Pallet<T>, CommitteeInfoV2Of<T>, OptionQuery>;

	/// [crate::NextCommittee] at version 2
	#[storage_alias]
	pub type NextCommittee<T: Config> = StorageValue<Pallet<T>, CommitteeInfoV2Of<T>, OptionQuery>;
}

/// Version 3 adds the Cardano epoch that selected each committee to [crate::CommitteeInfo].
pub mod v3 {
	#[cfg(feature = "try-runtime")]
	use super::v2;
	use super::v2::CommitteeInfoV2Of;
	use crate::{CommitteeInfo, Config, CurrentCommittee, NextCommittee, Pallet, QueuedCommittee};
	use frame_support::{
		migrations::VersionedMigration,
		traits::{Get, UncheckedOnRuntimeUpgrade},
		weights::Weight,
	};
	use sp_std::marker::PhantomData;
	#[cfg(feature = "try-runtime")]
	use {
		parity_scale_codec::{Decode, Encode},
		sp_std::vec::Vec,
	};

	type CommitteeInfoOf<T> = CommitteeInfo<
		<T as Config>::ScEpochNumber,
		<T as Config>::CommitteeMember,
		<T as Config>::MaxValidators,
	>;

	fn upgrade<T: Config>(old: CommitteeInfoV2Of<T>) -> CommitteeInfoOf<T> {
		// Nothing on chain records which Cardano epoch selected these committees.
		CommitteeInfo { epoch: old.epoch, committee: old.committee, mc_epoch: None }
	}

	/// Re-encodes the stored committees with no Cardano epoch. Use [MigrateV2ToV3].
	pub struct InnerMigrateV2ToV3<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV2ToV3<T> {
		fn on_runtime_upgrade() -> Weight {
			// A failed decode would leave `ValueQuery` items reading as empty committees.
			CurrentCommittee::<T>::translate::<CommitteeInfoV2Of<T>, _>(|old| {
				old.map(upgrade::<T>)
			})
			.expect("CurrentCommittee must decode as version 2");
			QueuedCommittee::<T>::translate::<CommitteeInfoV2Of<T>, _>(|old| old.map(upgrade::<T>))
				.expect("QueuedCommittee must decode as version 2");
			NextCommittee::<T>::translate::<CommitteeInfoV2Of<T>, _>(|old| old.map(upgrade::<T>))
				.expect("NextCommittee must decode as version 2");
			T::DbWeight::get().reads_writes(3, 3)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((
				v2::CurrentCommittee::<T>::get(),
				v2::QueuedCommittee::<T>::get(),
				v2::NextCommittee::<T>::get(),
			)
				.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			use frame_support::ensure;
			type Committees<T> = (
				Option<CommitteeInfoV2Of<T>>,
				Option<CommitteeInfoV2Of<T>>,
				Option<CommitteeInfoV2Of<T>>,
			);
			let (current, queued, next): Committees<T> = Decode::decode(&mut state.as_slice())
				.map_err(|_| "pre-upgrade state must decode")?;
			let kept = |old: Option<CommitteeInfoV2Of<T>>, new: Option<CommitteeInfoOf<T>>| match (
				old, new,
			) {
				(None, None) => true,
				(Some(old), Some(new)) => {
					old.epoch.encode() == new.epoch.encode()
						&& old.committee.encode() == new.committee.encode()
						&& new.mc_epoch.is_none()
				},
				_ => false,
			};
			ensure!(
				kept(current, CurrentCommittee::<T>::exists().then(CurrentCommittee::<T>::get)),
				"CurrentCommittee must keep its epoch and members, with no Cardano epoch"
			);
			ensure!(
				kept(queued, QueuedCommittee::<T>::exists().then(QueuedCommittee::<T>::get)),
				"QueuedCommittee must keep its epoch and members, with no Cardano epoch"
			);
			ensure!(
				kept(next, NextCommittee::<T>::get()),
				"NextCommittee must keep its epoch and members, with no Cardano epoch"
			);
			Ok(())
		}
	}

	/// Migrates the committee selection pallet from storage version 2 to 3
	pub type MigrateV2ToV3<T> = VersionedMigration<
		2,
		3,
		InnerMigrateV2ToV3<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

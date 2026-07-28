//! Implements [pallet_session::SessionManager] and [pallet_session::ShouldEndSession] for [crate::Pallet].
//!
//! This implementation has lag of one additional PC epoch when applying committees to sessions:
//! stock `pallet_session` only applies a validator set returned from `SessionManager::new_session`
//! at the *following* rotation. To keep the on-chain view accurate, a rotation moves
//! [crate::NextCommittee] (selected) to [crate::QueuedCommittee] (handed to `pallet_session`,
//! pending application) and promotes the previously queued committee to [crate::CurrentCommittee]
//! (the effective validator set of the session that just started).
//!
//! To use it, wire [crate::Pallet] in runtime configuration of [`pallet_session`], and enable
//! `pallet_session`'s `historical` feature with the runtime implementing
//! [`pallet_session::historical::Config`]. Registering committee members' keys is done via
//! [`pallet_session::SessionInterface::set_keys`], which is intended for privileged/internal
//! callers and (unlike the `set_keys` extrinsic) does not require an ownership proof — a
//! validator's session keys are already authenticated off-chain as part of their Cardano
//! registration, so re-proving ownership on-chain here would be redundant.
use crate::CommitteeMember;
use frame_system::pallet_prelude::BlockNumberFor;
use log::{debug, info, warn};
use pallet_session::SessionInterface;
use sp_staking::SessionIndex;
use sp_std::collections::btree_set::BTreeSet;
use sp_std::vec::Vec;

impl<T: crate::Config + pallet_session::Config + pallet_session::historical::Config>
	pallet_session::SessionManager<T::AccountId> for crate::Pallet<T>
where
	<T as pallet_session::Config>::Keys: From<T::AuthorityKeys>,
{
	/// Sets the first validator-set by mapping the current committee from [crate::Pallet]
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		let committee = crate::Pallet::<T>::current_committee_storage().committee;
		provide_committee_accounts::<T>(&committee);
		register_committee_keys::<T>(&committee);
		Some(
			committee
				.into_iter()
				.map(|member| member.authority_id().into())
				.collect::<Vec<_>>(),
		)
	}

	/// Rotates the committee in [crate::Pallet] and plans this new committee as upcoming validator-set.
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		info!("Session manager: new_session {new_index}, rotating the committee");
		let new_committee = crate::Pallet::<T>::rotate_committee_to_next_epoch()?;

		provide_committee_accounts::<T>(&new_committee);
		register_committee_keys::<T>(&new_committee);

		let new_committee_accounts =
			new_committee.into_iter().map(|member| member.authority_id().into()).collect();

		Some(new_committee_accounts)
	}

	fn end_session(end_index: SessionIndex) {
		debug!("Session manager: End session {end_index}");
	}

	// Session is expected to be at least 1 block behind sidechain epoch.
	fn start_session(start_index: SessionIndex) {
		let epoch_number = T::current_epoch_number();
		debug!("Session manager: Start session {start_index}, epoch {epoch_number}");
	}
}

// Registers keys of new committee members in the session pallet. This is necessary, as the pallet
// requires the keys to be registered prior to session start and we do not wish to force block
// producers to do it manually.
pub(crate) fn register_committee_keys<
	T: crate::Config + pallet_session::Config + pallet_session::historical::Config,
>(
	new_committee: &[T::CommitteeMember],
) where
	<T as pallet_session::Config>::Keys: From<T::AuthorityKeys>,
{
	let mut keys_added: BTreeSet<T::AccountId> = BTreeSet::new();
	for member in new_committee.iter() {
		let account_id = member.authority_id().into();

		if keys_added.contains(&account_id) {
			continue;
		}

		keys_added.insert(account_id.clone());
		let keys = <T as pallet_session::Config>::Keys::from(member.authority_keys());
		let call_result =
			<pallet_session::Pallet<T> as SessionInterface>::set_keys(&account_id, keys);
		match call_result {
			Ok(_) => debug!("set_keys for {account_id:?}"),
			Err(e) => info!("Could not set_keys for {account_id:?}, error: {:?}", e),
		}
	}
}

// Ensures that all accounts tied to new committee members exist by incrementing their
// account provider counts. This is a necessary temporary solution, because we don't check
// whether a block producer's account exists or not, when selecting them to a committee.
// A proper solution would either be:
// - increasing provider count for an account for as long as it is in the active committee
//   and decreasing it afterwards, or
// - considering account existence when selecting the committee
// This will be addressed in later development.
pub(crate) fn provide_committee_accounts<T: crate::Config>(new_committee: &[T::CommitteeMember]) {
	let new_accs: BTreeSet<T::AccountId> =
		new_committee.iter().map(|m| m.authority_id().into()).collect();
	for account in new_accs {
		if !frame_system::Pallet::<T>::account_exists(&account) {
			frame_system::Pallet::<T>::inc_providers(&account);
		}
	}
}

/// Tries to end each session in the first block of each partner chains epoch in which the committee for the epoch is defined.
impl<T, EpochNumber> pallet_session::ShouldEndSession<BlockNumberFor<T>> for crate::Pallet<T>
where
	T: crate::Config<ScEpochNumber = EpochNumber>,
	EpochNumber: Clone + PartialOrd,
{
	fn should_end_session(n: BlockNumberFor<T>) -> bool {
		let current_epoch_number = T::current_epoch_number();
		// The queued committee is the most recently rotated one, so its epoch determines
		// whether a rotation is due for the current epoch.
		let queued_committee_epoch = crate::Pallet::<T>::queued_committee_storage().epoch;
		let next_committee_is_defined = crate::Pallet::<T>::next_committee().is_some();
		if current_epoch_number > queued_committee_epoch {
			if next_committee_is_defined {
				info!("Session manager: should_end_session({n:?}) = true");
				true
			} else {
				warn!(
					"Session manager: should_end_session({n:?}) 'current epoch' > 'committee epoch' but the next committee is not defined"
				);
				false
			}
		} else {
			false
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		CommitteeInfo, NextCommittee, QueuedCommittee,
		mock::{mock_pallet::CurrentEpoch, *},
		tests::increment_epoch,
	};
	use pallet_session::ShouldEndSession;
	pub const IRRELEVANT: u64 = 2;
	use sp_runtime::testing::UintAuthorityId;

	type Manager = crate::Pallet<Test>;

	#[test]
	fn should_end_session_if_last_one_ended_late_and_new_committee_is_defined() {
		let queued_committee_epoch = 100;
		let queued_committee = ids_and_keys_fn(&[ALICE]);
		let next_committee_epoch = 102;
		let next_committee = ids_and_keys_fn(&[BOB]);

		new_test_ext().execute_with(|| {
			QueuedCommittee::<Test>::put(CommitteeInfo {
				epoch: queued_committee_epoch,
				committee: queued_committee,
			});
			CurrentEpoch::<Test>::set(queued_committee_epoch + 2);
			assert!(!Manager::should_end_session(IRRELEVANT));
			NextCommittee::<Test>::put(CommitteeInfo {
				epoch: next_committee_epoch,
				committee: next_committee,
			});
			assert!(Manager::should_end_session(IRRELEVANT));
		});
	}

	#[test]
	fn genesis_registers_session_keys_externally() {
		use pallet_session::ExternallySetKeys;

		new_test_ext().execute_with(|| {
			assert!(ExternallySetKeys::<Test>::contains_key(&ALICE.authority_id));
			assert!(ExternallySetKeys::<Test>::contains_key(&BOB.authority_id));
			assert_eq!(
				Session::load_keys(&ALICE.authority_id),
				Some(SessionKeys { foo: UintAuthorityId(ALICE.authority_keys) })
			);
		});
	}

	#[test]
	fn register_session_keys_for_provided_authorities() {
		new_test_ext().execute_with(|| {
			set_validators_directly(&[DAVE, EVE], 1).unwrap();
			// By default, the session keys are not set for the account.
			assert_eq!(Session::load_keys(&DAVE.authority_id), None);
			assert_eq!(Session::load_keys(&EVE.authority_id), None);
			increment_epoch();

			start_session(1);

			// After setting the keys, they should be stored in the session.
			assert_eq!(
				Session::load_keys(&DAVE.authority_id),
				Some(SessionKeys { foo: UintAuthorityId(DAVE.authority_keys) })
			);
			assert_eq!(
				Session::load_keys(&EVE.authority_id),
				Some(SessionKeys { foo: UintAuthorityId(EVE.authority_keys) })
			);
		});
	}

	#[test]
	fn ends_one_session_per_epoch_and_applies_committee_next_session() {
		new_test_ext().execute_with(|| {
			assert_eq!(Session::current_index(), 0);
			// At genesis the current and the queued committee coincide.
			assert_eq!(SessionCommitteeManagement::current_committee_storage().epoch, 0);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee,
				ids_and_keys_fn(&[ALICE, BOB])
			);
			assert_eq!(SessionCommitteeManagement::queued_committee_storage().epoch, 0);
			increment_epoch();
			set_validators_directly(&[CHARLIE, DAVE], 1).unwrap();

			// Single session ends at the epoch boundary: the committee is rotated and queued,
			// but stock pallet_session only applies a queued validator-set from the next session,
			// so CHARLIE and DAVE are not active yet (one-epoch application lag). The current
			// committee keeps tracking the validator set actually applied by pallet_session.
			advance_one_block();
			assert_eq!(Session::current_index(), 1);
			assert_eq!(Session::validators(), vec![ALICE.authority_id, BOB.authority_id]);
			assert_eq!(SessionCommitteeManagement::queued_committee_storage().epoch, 1);
			assert_eq!(
				SessionCommitteeManagement::queued_committee_storage().committee,
				ids_and_keys_fn(&[CHARLIE, DAVE])
			);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee,
				ids_and_keys_fn(&[ALICE, BOB])
			);
			// The current committee's epoch is stamped with the epoch it serves in.
			assert_eq!(SessionCommitteeManagement::current_committee_storage().epoch, 1);

			// No additional session is forced within the epoch: the committee is not applied faster.
			for _i in 0..10 {
				advance_one_block();
				assert_eq!(Session::current_index(), 1);
				assert_eq!(Session::validators(), vec![ALICE.authority_id, BOB.authority_id]);
				assert_eq!(SessionCommitteeManagement::queued_committee_storage().epoch, 1);
				assert_eq!(
					SessionCommitteeManagement::current_committee_storage().committee,
					ids_and_keys_fn(&[ALICE, BOB])
				);
			}

			// Only the next epoch change ends another session, which applies the committee
			// queued at the previous boundary. The current committee follows.
			increment_epoch();
			set_validators_directly(&[CHARLIE, DAVE], 2).unwrap();
			advance_one_block();
			assert_eq!(Session::current_index(), 2);
			assert_eq!(Session::validators(), vec![CHARLIE.authority_id, DAVE.authority_id]);
			assert_eq!(SessionCommitteeManagement::queued_committee_storage().epoch, 2);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee,
				ids_and_keys_fn(&[CHARLIE, DAVE])
			);
			assert_eq!(SessionCommitteeManagement::current_committee_storage().epoch, 2);
		});
	}
}

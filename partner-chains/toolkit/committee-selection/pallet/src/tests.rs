use crate::mock::*;
use frame_support::{
	assert_ok,
	inherent::ProvideInherent,
	traits::{Hooks, UnfilteredDispatchable},
};

mod inherent_tests {
	use super::*;
	use crate::{CommitteeInfo, Error, pallet};
	use frame_support::assert_err;
	use sidechain_domain::byte_string::SizedByteString;
	use sp_runtime::DispatchError;
	use sp_session_validator_management::InherentError;

	#[test]
	fn changes_validator_vec_based_on_inherent_data() {
		new_test_ext().execute_with(|| {
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee,
				ids_and_keys_fn(&[ALICE, BOB])
			);
			test_validators_change_through_inherents(&[ALICE]);
		});
	}

	#[test]
	fn prevent_overwriting_committee() {
		new_test_ext().execute_with(|| {
			assert_err!(
				set_validators_directly(&[ALICE, BOB], 0),
				DispatchError::from(Error::<Test>::InvalidEpoch)
			);
			assert_err!(
				set_validators_directly(&[ALICE], 0),
				DispatchError::from(Error::<Test>::InvalidEpoch)
			);
		});
	}

	#[test]
	fn only_one_inherent_can_run_per_block() {
		new_test_ext().execute_with(|| {
			set_validators_directly(&[ALICE, BOB], 1).expect("Frist call should succeed");
			set_validators_directly(&[ALICE, BOB], 1).expect_err("Second call should fail");
		});
	}

	#[test]
	fn set_rejects_empty_committee() {
		new_test_ext().execute_with(|| {
			assert_err!(
				set_validators_directly(&[], 1),
				DispatchError::from(Error::<Test>::EmptyCommittee)
			);
			assert!(pallet::NextCommittee::<Test>::get().is_none());
		});
	}

	#[test]
	fn empty_selection_keeps_queued_committee() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			SELECT_EMPTY_COMMITTEE.with(|flag| flag.set(true));
			let queued_committee = pallet::QueuedCommittee::<Test>::get().committee;
			assert!(!queued_committee.is_empty());

			let (inherent_data, _) = create_inherent_data(&[]);
			let call =
				<SessionCommitteeManagement as ProvideInherent>::create_inherent(&inherent_data)
					.expect("create_inherent should fall back to the queued committee");
			let pallet::Call::set { validators, .. } = &call else {
				panic!("expected a set call, got {call:?}");
			};
			assert_eq!(*validators, queued_committee);
			assert_ok!(<SessionCommitteeManagement as ProvideInherent>::check_inherent(
				&call,
				&inherent_data
			));

			assert_ok!(call.dispatch_bypass_filter(RuntimeOrigin::none()));
			assert_eq!(
				pallet::NextCommittee::<Test>::get().map(|c| c.committee),
				Some(queued_committee)
			);
		});
	}

	#[test]
	fn check_inherent_rejects_empty_committee_from_empty_selection() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			SELECT_EMPTY_COMMITTEE.with(|flag| flag.set(true));
			let (inherent_data, selection_inputs_hash) = create_inherent_data(&[]);
			let call = pallet::Call::set {
				validators: Default::default(),
				for_epoch_number: pallet::QueuedCommittee::<Test>::get().epoch + 1,
				selection_inputs_hash,
				mc_epoch: MOCK_MC_EPOCH,
			};
			assert!(
				<SessionCommitteeManagement as ProvideInherent>::check_inherent(
					&call,
					&inherent_data
				)
				.is_err()
			);
		});
	}

	fn test_validators_change_through_inherents(new_validators: &[MockValidator]) {
		set_validators_through_inherents(new_validators);
		let mut expected_validators = ids_and_keys_fn(new_validators);

		expected_validators.sort();

		let mut actual_validators = SessionCommitteeManagement::next_committee_storage()
			.as_ref()
			.map(|c| c.committee.clone())
			.expect("Committee should be set after setting validators through inherents");
		actual_validators.sort();
		assert_eq!(actual_validators, expected_validators);
	}

	#[test]
	fn check_inherent_works() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			let validators = vec![ALICE];
			let call = create_inherent_set_validators_call(&validators).expect(
				"create inherent should create a validator set call if given new validators list",
			);
			assert_ok!(<SessionCommitteeManagement as ProvideInherent>::check_inherent(
				&call,
				&create_inherent_data(&validators).0
			));
		});
	}

	#[test]
	fn check_inherent_uses_next_committee_if_cannot_calculate_from_data() {
		// This is for the case NextCommittee was set already and committee cannot be selected from inherent data.
		// Block proposer has just rotated NextCommittee to CurrentCommittee and call is to "set" the NextCommittee.
		// Because committee could not be selected from the inherent data, it proposed its CurrentCommittee (after rotation).
		// From block verifier's perspective, it should be its NextCommittee, because it will rotate committee after checking inherents.
		new_test_ext().execute_with(|| {
			// Make sure that the CurrentCommittee is different to the NextCommittee.
			pallet::CurrentCommittee::<Test>::put(CommitteeInfo {
				committee: ids_and_keys_fn(&[CHARLIE, DAVE]),
				epoch: 42,
				mc_epoch: None,
			});
			pallet::NextCommittee::<Test>::put(CommitteeInfo {
				committee: ids_and_keys_fn(&[ALICE, BOB]),
				epoch: 43,
				mc_epoch: None,
			});
			let (inherent_data_from_which_committee_cannot_be_selected, selection_inputs_hash) =
				create_inherent_data(&[]);
			let set_call = pallet::Call::set {
				validators: ids_and_keys_fn(&[ALICE, BOB]),
				for_epoch_number: 43,
				selection_inputs_hash,
				mc_epoch: MOCK_MC_EPOCH,
			};
			assert_ok!(<SessionCommitteeManagement as ProvideInherent>::check_inherent(
				&set_call,
				&inherent_data_from_which_committee_cannot_be_selected
			));
		});
	}

	#[test]
	fn check_inherent_uses_current_committee_when_cannot_calculate_from_data_and_next_committee_is_not_set()
	 {
		// This case if for `check_inherent` verifying block number 1 when committee cannot be calculated from the inherent data.
		// It is the only situation when NextCommittee is not set.
		let genesis_validators = [ALICE, BOB];
		new_test_ext_with_genesis_initial_authorities(&genesis_validators).execute_with(|| {
			assert!(pallet::NextCommittee::<Test>::get().is_none());
			let (inherent_data_from_which_committee_cannot_be_selected, selection_inputs_hash) =
				create_inherent_data(&[]);
			let call = pallet::Call::set {
				validators: ids_and_keys_fn(&genesis_validators),
				for_epoch_number: 43,
				selection_inputs_hash,
				mc_epoch: MOCK_MC_EPOCH,
			};
			// Should be Ok, because check_inherent should use CurrentCommittee value.
			assert_ok!(<SessionCommitteeManagement as ProvideInherent>::check_inherent(
				&call,
				&inherent_data_from_which_committee_cannot_be_selected
			));
		});
	}

	#[test]
	fn check_inherent_error_includes_hash_if_correct() {
		let mut genesis_validators = [ALICE, BOB];
		new_test_ext_with_genesis_initial_authorities(&genesis_validators).execute_with(|| {
			assert!(pallet::NextCommittee::<Test>::get().is_none());
			let (inherent_data_from_which_committee_cannot_be_selected, selection_inputs_hash) =
				create_inherent_data(&genesis_validators);
			genesis_validators.reverse();
			let call = pallet::Call::set {
				validators: ids_and_keys_fn(&genesis_validators),
				for_epoch_number: 43,
				selection_inputs_hash: selection_inputs_hash.clone(),
				mc_epoch: MOCK_MC_EPOCH,
			};

			let expected_err =
				InherentError::InvalidValidatorsMatchingHash(selection_inputs_hash.clone());
			assert_eq!(
				<SessionCommitteeManagement as ProvideInherent>::check_inherent(
					&call,
					&inherent_data_from_which_committee_cannot_be_selected
				),
				Err(expected_err)
			);
		});
	}

	#[test]
	fn check_inherent_error_includes_both_hashes_if_different() {
		let mut genesis_validators = [ALICE, BOB];
		new_test_ext_with_genesis_initial_authorities(&genesis_validators).execute_with(|| {
			assert!(pallet::NextCommittee::<Test>::get().is_none());
			let (
				inherent_data_from_which_committee_cannot_be_selected,
				correct_selection_inputs_hash,
			) = create_inherent_data(&genesis_validators);
			let incorrect_selection_inputs_hash = SizedByteString([9u8; 32]);
			genesis_validators.reverse();
			let call = pallet::Call::set {
				validators: ids_and_keys_fn(&genesis_validators),
				for_epoch_number: 43,
				selection_inputs_hash: incorrect_selection_inputs_hash.clone(),
				mc_epoch: MOCK_MC_EPOCH,
			};

			let expected_err = InherentError::InvalidValidatorsHashMismatch(
				correct_selection_inputs_hash.clone(),
				incorrect_selection_inputs_hash.clone(),
			);
			assert_eq!(
				<SessionCommitteeManagement as ProvideInherent>::check_inherent(
					&call,
					&inherent_data_from_which_committee_cannot_be_selected
				),
				Err(expected_err)
			);
		});
	}
}

mod committee_rotation_tests {
	use super::*;
	#[test]
	fn test_current_committee_epoch() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			set_validators_through_inherents(&[ALICE]);
			let current_committee_epoch =
				SessionCommitteeManagement::current_committee_storage().epoch;
			assert_eq!(current_committee_epoch, current_epoch_number());
		});
	}

	#[test]
	fn test_next_committee() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();

			let next_committee = SessionCommitteeManagement::next_committee();
			assert_eq!(next_committee, None);

			set_validators_through_inherents(&[ALICE]);

			let next_committee = SessionCommitteeManagement::next_committee();
			assert_eq!(next_committee, Some(authority_ids(&[ALICE])));
		});
	}

	#[test]
	fn test_rotate_committee() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();

			// first committee is Alice and Bob (from the mock)
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE, BOB])
			);

			// first committee epoch is the one set in the `initialize_first_committee`
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().epoch,
				current_epoch_number()
			);

			// verify that the next committee is not set yet
			assert_eq!(SessionCommitteeManagement::next_committee(), None);

			// set the new committee
			set_validators_through_inherents(&[ALICE]);

			// verify that the next committee is set
			assert_eq!(SessionCommitteeManagement::next_committee(), Some(authority_ids(&[ALICE])));

			// increment the epoch (this only changes to mock value for T::current_epoch_number)
			increment_epoch();

			// rotate the committee
			assert_eq!(rotate_committee(), Some(vec![ALICE.authority_id]));

			// the rotated committee is queued (the session machinery applies it one session
			// later), while the current committee is the previously queued one
			assert_eq!(
				SessionCommitteeManagement::queued_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE])
			);
			assert_eq!(
				SessionCommitteeManagement::queued_committee_storage().epoch,
				current_epoch_number()
			);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE, BOB])
			);
			// the promoted committee's epoch is stamped with the epoch it starts serving in
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().epoch,
				current_epoch_number()
			);

			// verify that the next committee is not set
			assert_eq!(SessionCommitteeManagement::next_committee(), None);

			// the next rotation promotes the queued committee to the current one
			set_validators_through_inherents(&[BOB]);
			increment_epoch();
			assert_eq!(rotate_committee(), Some(vec![BOB.authority_id]));
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE])
			);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().epoch,
				current_epoch_number()
			);
		});
	}

	/// After epochs without any block, the first rotation catches up in one step: the committee
	/// that was due in a skipped epoch is queued stamped with the *current* epoch, the inherent
	/// selects the following committee for the epoch after it, and no second rotation is asked
	/// for in this epoch (a second BABE epoch change in one BABE epoch would be rejected by the
	/// BABE client).
	#[test]
	fn test_recover_from_stale_committee() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();

			// first committee is Alice and Bob (from the mock)
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE, BOB])
			);

			// verify that the next committee is not set yet
			assert_eq!(SessionCommitteeManagement::next_committee(), None);

			// set the new committee
			set_validators_through_inherents(&[ALICE]);

			// 3 epochs pass without any blocks
			increment_epoch();
			increment_epoch();
			increment_epoch();
			let current_epoch = current_epoch_number();

			// Alice and Bob, which were the initial committee, are still the committee (stale)
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE, BOB])
			);

			// Alice, which was the next committee, is still the next committee, selected for an
			// epoch that has already passed
			assert_eq!(SessionCommitteeManagement::next_committee(), Some(authority_ids(&[ALICE])));
			assert_eq!(
				SessionCommitteeManagement::next_committee_storage().unwrap().epoch,
				current_epoch - 2
			);

			// the first block after 3 epochs rotates to the next committee (Alice), which is
			// queued for application at the following session — stamped with the current epoch,
			// not the one it was selected for, so the pipeline is caught up in this one rotation
			assert_eq!(rotate_committee(), Some(vec![ALICE.authority_id]));
			assert_eq!(
				SessionCommitteeManagement::queued_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE])
			);
			assert_eq!(SessionCommitteeManagement::queued_committee_storage().epoch, current_epoch);
			// Alice and Bob, promoted from the queue, serve the current epoch and are stamped so
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE, BOB])
			);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().epoch,
				current_epoch
			);

			// the inherent now selects the committee for the *next* epoch, not for the skipped ones
			assert_eq!(SessionCommitteeManagement::next_committee(), None);
			assert_eq!(
				SessionCommitteeManagement::get_next_unset_epoch_number(),
				current_epoch + 1
			);
			set_validators_through_inherents(&[CHARLIE]);
			assert_eq!(
				SessionCommitteeManagement::next_committee_storage().unwrap().epoch,
				current_epoch + 1
			);

			// and no further rotation is due in this epoch
			assert!(!<SessionCommitteeManagement as pallet_session::ShouldEndSession<u64>>::should_end_session(2));

			// the next epoch rotates as usual: Alice serves, Charlie is queued
			increment_epoch();
			assert!(<SessionCommitteeManagement as pallet_session::ShouldEndSession<u64>>::should_end_session(3));
			assert_eq!(rotate_committee(), Some(vec![CHARLIE.authority_id]));
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE])
			);
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().epoch,
				current_epoch_number()
			);
			assert_eq!(
				SessionCommitteeManagement::queued_committee_storage().epoch,
				current_epoch_number()
			);
		});
	}

	#[test]
	fn store_committee_only_up_to_next_epoch() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			set_validators_through_inherents(&[BOB, ALICE]);

			// setting it again should not change the committee
			assert_eq!(create_inherent_set_validators_call(&[ALICE]), None);
		});
	}

	#[test]
	fn do_not_rotate_committee_if_next_committee_is_not_defined() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();

			// verify that the next committee is not set
			assert_eq!(SessionCommitteeManagement::next_committee(), None);

			// rotation should not happen
			assert_eq!(rotate_committee(), None);

			// verify that the committee has not changed
			assert_eq!(
				SessionCommitteeManagement::current_committee_storage().committee.clone(),
				ids_and_keys_fn(&[ALICE, BOB])
			);
		});
	}
}

pub(crate) fn increment_epoch() {
	mock_pallet::CurrentEpoch::<Test>::put(current_epoch_number() + 1);
}

fn set_epoch(epoch: u64) {
	mock_pallet::CurrentEpoch::<Test>::put(epoch);
}

// in real life first epoch will be something much bigger than 0, that's why it is here
const ARBITRARY_FIRST_EPOCH: u64 = 189374234;
fn initialize_first_committee() {
	set_epoch(ARBITRARY_FIRST_EPOCH);
	SessionCommitteeManagement::on_initialize(1);
}

fn rotate_committee() -> Option<Vec<u64>> {
	let committee = SessionCommitteeManagement::rotate_committee_to_next_epoch()?;
	Some(committee.into_iter().map(|(id, _)| id).collect())
}

mod mc_epoch {
	use super::*;
	use crate::{
		CommitteeInfo,
		migrations::{v2, v3},
		pallet,
	};
	use frame_support::{BoundedVec, inherent::InherentData, traits::UncheckedOnRuntimeUpgrade};
	use parity_scale_codec::Encode;
	use sidechain_domain::McEpochNumber;
	use sp_session_validator_management::InherentError;

	fn selection_data(validators: &[MockValidator], mc_epoch: u32) -> InherentData {
		create_inherent_data_with_mc_epoch(validators, McEpochNumber(mc_epoch)).0
	}

	fn call_mc_epoch(call: &pallet::Call<Test>) -> McEpochNumber {
		let pallet::Call::set { mc_epoch, .. } = call else { panic!("expected a set call") };
		*mc_epoch
	}

	#[test]
	fn a_selected_committee_points_to_the_cardano_epoch_of_its_data() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			let data = selection_data(&[ALICE], 501);
			let call = SessionCommitteeManagement::create_inherent(&data).unwrap();
			assert_eq!(call_mc_epoch(&call), McEpochNumber(501));
			assert_ok!(SessionCommitteeManagement::check_inherent(&call, &data));
			assert_ok!(call.dispatch_bypass_filter(RuntimeOrigin::none()));
			assert_eq!(
				pallet::NextCommittee::<Test>::get().unwrap().mc_epoch,
				Some(McEpochNumber(501))
			);
		});
	}

	#[test]
	fn the_pointer_travels_with_the_committee_to_current() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			let data = selection_data(&[ALICE], 501);
			let call = SessionCommitteeManagement::create_inherent(&data).unwrap();
			assert_ok!(call.dispatch_bypass_filter(RuntimeOrigin::none()));

			increment_epoch();
			rotate_committee();
			assert_eq!(pallet::QueuedCommittee::<Test>::get().mc_epoch, Some(McEpochNumber(501)));
			assert_eq!(pallet::CurrentCommittee::<Test>::get().mc_epoch, None);

			increment_epoch();
			rotate_committee();
			let current = pallet::CurrentCommittee::<Test>::get();
			assert_eq!(current.committee, ids_and_keys_fn(&[ALICE]));
			assert_eq!(current.mc_epoch, Some(McEpochNumber(501)));
		});
	}

	#[test]
	fn a_kept_committee_keeps_the_cardano_epoch_of_its_members() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			pallet::QueuedCommittee::<Test>::mutate(|queued| {
				queued.mc_epoch = Some(McEpochNumber(499))
			});
			SELECT_EMPTY_COMMITTEE.with(|flag| flag.set(true));

			let data = selection_data(&[], 501);
			let call = SessionCommitteeManagement::create_inherent(&data).unwrap();
			assert_eq!(call_mc_epoch(&call), McEpochNumber(499));
			assert_ok!(SessionCommitteeManagement::check_inherent(&call, &data));
		});
	}

	#[test]
	fn a_kept_committee_stored_without_an_epoch_takes_the_provided_one() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			assert_eq!(pallet::QueuedCommittee::<Test>::get().mc_epoch, None);
			SELECT_EMPTY_COMMITTEE.with(|flag| flag.set(true));

			let data = selection_data(&[], 501);
			let call = SessionCommitteeManagement::create_inherent(&data).unwrap();
			assert_eq!(call_mc_epoch(&call), McEpochNumber(501));
			assert_ok!(SessionCommitteeManagement::check_inherent(&call, &data));
		});
	}

	#[test]
	fn a_block_pointing_elsewhere_than_the_verifier_computes_is_rejected() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			let call = SessionCommitteeManagement::create_inherent(&selection_data(&[ALICE], 501))
				.unwrap();
			assert_eq!(
				SessionCommitteeManagement::check_inherent(&call, &selection_data(&[ALICE], 500)),
				Err(InherentError::InvalidMcEpoch(McEpochNumber(501), McEpochNumber(500)))
			);
		});
	}

	#[test]
	#[should_panic(expected = "the node predates runtime API version 3")]
	fn inputs_without_the_cardano_epoch_cannot_select_a_committee() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			SessionCommitteeManagement::create_inherent(&create_inputs_only_inherent_data(&[
				ALICE,
			]));
		});
	}

	#[test]
	fn a_block_verified_with_inputs_alone_fails_with_a_readable_error() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			let call = SessionCommitteeManagement::create_inherent(&selection_data(&[ALICE], 501))
				.unwrap();
			assert_eq!(
				SessionCommitteeManagement::check_inherent(
					&call,
					&create_inputs_only_inherent_data(&[ALICE])
				),
				Err(InherentError::UndecodableSelectionInherentData)
			);
		});
	}

	#[test]
	fn the_selection_inputs_hash_covers_the_inputs_alone() {
		new_test_ext().execute_with(|| {
			initialize_first_committee();
			let call = SessionCommitteeManagement::create_inherent(&selection_data(&[ALICE], 501))
				.unwrap();
			let pallet::Call::set { selection_inputs_hash, .. } = &call else { panic!() };
			assert_eq!(
				*selection_inputs_hash,
				create_inherent_data_with_mc_epoch(&[ALICE], McEpochNumber(1)).1
			);
		});
	}

	#[test]
	fn migration_keeps_the_committees_and_points_them_nowhere() {
		new_test_ext().execute_with(|| {
			let old = |epoch: u64, members: &[MockValidator]| v2::CommitteeInfoV2Of::<Test> {
				epoch,
				committee: BoundedVec::truncate_from(ids_and_keys_fn(members).into_inner()),
			};
			v2::CurrentCommittee::<Test>::put(old(7, &[ALICE]));
			v2::QueuedCommittee::<Test>::put(old(8, &[BOB]));
			v2::NextCommittee::<Test>::put(old(9, &[CHARLIE]));

			v3::InnerMigrateV2ToV3::<Test>::on_runtime_upgrade();

			let expected = |epoch: u64, members: &[MockValidator]| CommitteeInfo {
				epoch,
				committee: ids_and_keys_fn(members),
				mc_epoch: None,
			};
			let encoded = |info: CommitteeInfo<_, _, _>| info.encode();
			assert_eq!(
				encoded(pallet::CurrentCommittee::<Test>::get()),
				encoded(expected(7, &[ALICE]))
			);
			assert_eq!(
				encoded(pallet::QueuedCommittee::<Test>::get()),
				encoded(expected(8, &[BOB]))
			);
			assert_eq!(
				pallet::NextCommittee::<Test>::get().map(encoded),
				Some(encoded(expected(9, &[CHARLIE])))
			);
		});
	}
}

#[cfg(feature = "try-runtime")]
mod try_runtime {
	use super::*;
	use crate::migrations::{v2, v3};
	use frame_support::{BoundedVec, traits::UncheckedOnRuntimeUpgrade};

	#[test]
	fn the_v3_checks_accept_the_migrated_state() {
		new_test_ext().execute_with(|| {
			let old = |epoch: u64, members: &[MockValidator]| v2::CommitteeInfoV2Of::<Test> {
				epoch,
				committee: BoundedVec::truncate_from(ids_and_keys_fn(members).into_inner()),
			};
			v2::CurrentCommittee::<Test>::put(old(7, &[ALICE]));
			v2::QueuedCommittee::<Test>::put(old(8, &[BOB]));
			v2::NextCommittee::<Test>::kill();

			let state = v3::InnerMigrateV2ToV3::<Test>::pre_upgrade().unwrap();
			v3::InnerMigrateV2ToV3::<Test>::on_runtime_upgrade();
			v3::InnerMigrateV2ToV3::<Test>::post_upgrade(state).unwrap();
		});
	}
}

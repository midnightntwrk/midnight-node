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

use crate::{Error, Event, mock::*};
use frame_support::inherent::ProvideInherent;
use frame_support::{BoundedVec, assert_noop, assert_ok};
use midnight_primitives_federated_authority_observation::{
	AuthorityMemberPublicKey, FederatedAuthorityData, INHERENT_IDENTIFIER,
};
use parity_scale_codec::Encode;
use sidechain_domain::McBlockHash;
use sp_inherents::InherentData;
use sp_runtime::traits::Dispatchable;

// Helper function to create inherent data
fn create_inherent_data(council: Vec<u64>, technical_committee: Vec<u64>) -> InherentData {
	let mut inherent_data = InherentData::new();

	let council_keys: Vec<AuthorityMemberPublicKey> =
		council.into_iter().map(|id| AuthorityMemberPublicKey(id.encode())).collect();

	let tc_keys: Vec<AuthorityMemberPublicKey> = technical_committee
		.into_iter()
		.map(|id| AuthorityMemberPublicKey(id.encode()))
		.collect();

	let fed_auth_data = FederatedAuthorityData {
		council_authorities: council_keys,
		technical_committee_authorities: tc_keys,
		mc_block_hash: McBlockHash([0u8; 32]),
	};

	inherent_data
		.put_data(INHERENT_IDENTIFIER, &fed_auth_data)
		.expect("Failed to put inherent data");

	inherent_data
}

#[test]
fn reset_council_and_tc_members_works() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];

		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			council_members.clone(),
			tc_members.clone(),
		));

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), council_members);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), tc_members);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), council_members);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			tc_members
		);

		// Verify events were emitted
		System::assert_has_event(
			Event::CouncilMembersReset { members: BoundedVec::try_from(council_members).unwrap() }
				.into(),
		);
		System::assert_has_event(
			Event::TechnicalCommitteeMembersReset {
				members: BoundedVec::try_from(tc_members).unwrap(),
			}
			.into(),
		);
	});
}

#[test]
fn reset_members_requires_none_origin() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];

		// Should fail with signed origin
		assert_noop!(
			FederatedAuthorityObservation::reset_members(
				frame_system::RawOrigin::Signed(1).into(),
				council_members.clone(),
				tc_members.clone(),
			),
			sp_runtime::DispatchError::BadOrigin
		);

		// Should fail with root origin
		assert_noop!(
			FederatedAuthorityObservation::reset_members(
				frame_system::RawOrigin::Root.into(),
				council_members,
				tc_members,
			),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn reset_members_fails_with_too_many_council_members() {
	new_test_ext().execute_with(|| {
		// Create more members than the max
		let max_members = CouncilMaxMembers::get() as u64;
		let too_many_members: Vec<u64> = (0..max_members + 1).collect();
		let tc_members = vec![4, 5, 6];

		assert_noop!(
			FederatedAuthorityObservation::reset_members(
				frame_system::RawOrigin::None.into(),
				too_many_members,
				tc_members,
			),
			Error::<Test>::TooManyMembers
		);
	});
}

#[test]
fn reset_members_fails_with_too_many_technical_committee_members() {
	new_test_ext().execute_with(|| {
		// Create more members than the max
		let council_members = vec![1, 2, 3];
		let max_members = TechnicalCommitteeMaxMembers::get() as u64;
		let too_many_members: Vec<u64> = (0..max_members + 1).collect();

		assert_noop!(
			FederatedAuthorityObservation::reset_members(
				frame_system::RawOrigin::None.into(),
				council_members,
				too_many_members,
			),
			Error::<Test>::TooManyMembers
		);
	});
}

#[test]
fn reset_members_sorts_members() {
	new_test_ext().execute_with(|| {
		let unsorted_council = vec![3, 1, 2];
		let sorted_council = vec![1, 2, 3];
		let unsorted_tc = vec![6, 4, 5];
		let sorted_tc = vec![4, 5, 6];

		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			unsorted_council,
			unsorted_tc,
		));

		// Verify members are sorted
		assert_eq!(CouncilMembership::members().to_vec(), sorted_council);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), sorted_tc);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), sorted_council);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			sorted_tc
		);
	});
}

#[test]
fn no_event_when_same_members() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];

		// Set initial members
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			council_members.clone(),
			tc_members.clone(),
		));

		// Reset events
		System::reset_events();

		// Call with same members
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			council_members.clone(),
			tc_members.clone(),
		));

		// Members should remain unchanged
		assert_eq!(CouncilMembership::members().to_vec(), council_members);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), tc_members);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), council_members);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			tc_members
		);

		// No events should be emitted since members didn't change
		assert_eq!(System::events().len(), 0);
	});
}

#[test]
fn create_inherent_works_when_council_changes() {
	new_test_ext().execute_with(|| {
		let initial_council = vec![10, 11, 12];
		let initial_tc = vec![13, 14, 15];
		let new_council = vec![1, 2, 3];
		let new_tc = vec![4, 5, 6];

		// Initialize with some members first
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			initial_council,
			initial_tc,
		));

		// Now create inherent with different members
		let inherent_data = create_inherent_data(new_council.clone(), new_tc.clone());

		let call = FederatedAuthorityObservation::create_inherent(&inherent_data);
		assert!(call.is_some(), "Should create inherent when members change");

		if let Some(call) = call {
			let runtime_call = RuntimeCall::FederatedAuthorityObservation(call);
			assert_ok!(runtime_call.dispatch(frame_system::RawOrigin::None.into()));
		}

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), new_council);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), new_tc);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), new_council);
		assert_eq!(pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(), new_tc);
	});
}

#[test]
fn create_inherent_with_same_members_emits_no_events() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];

		// Initialize with some members first
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			council_members.clone(),
			tc_members.clone(),
		));

		// Reset events
		System::reset_events();

		// Create inherent data with same members
		let inherent_data = create_inherent_data(council_members, tc_members);
		let call = FederatedAuthorityObservation::create_inherent(&inherent_data);

		// Call is created but should not emit events when dispatched since members are the same
		assert!(call.is_some(), "Inherent call should be created");

		if let Some(call) = call {
			let runtime_call = RuntimeCall::FederatedAuthorityObservation(call);
			assert_ok!(runtime_call.dispatch(frame_system::RawOrigin::None.into()));
		}

		// No events should be emitted since members didn't change
		assert_eq!(System::events().len(), 0);
	});
}

#[test]
fn create_inherent_works_when_only_council_changes() {
	new_test_ext().execute_with(|| {
		let initial_council = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];
		let new_council = vec![7, 8, 9];

		// Set initial state
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			initial_council,
			tc_members.clone(),
		));

		// Create inherent with changed council but same TC
		let inherent_data = create_inherent_data(new_council.clone(), tc_members.clone());
		let call = FederatedAuthorityObservation::create_inherent(&inherent_data);

		assert!(call.is_some(), "Should create inherent when council changes");

		if let Some(call) = call {
			let runtime_call = RuntimeCall::FederatedAuthorityObservation(call);
			assert_ok!(runtime_call.dispatch(frame_system::RawOrigin::None.into()));
		}

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), new_council);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), tc_members);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), new_council);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			tc_members
		);
	});
}

#[test]
fn create_inherent_works_when_only_technical_committee_changes() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let initial_tc = vec![4, 5, 6];
		let new_tc = vec![7, 8, 9];

		// Set initial state
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			council_members.clone(),
			initial_tc,
		));

		// Create inherent with same council but changed TC
		let inherent_data = create_inherent_data(council_members.clone(), new_tc.clone());
		let call = FederatedAuthorityObservation::create_inherent(&inherent_data);

		assert!(call.is_some(), "Should create inherent when TC changes");

		if let Some(call) = call {
			let runtime_call = RuntimeCall::FederatedAuthorityObservation(call);
			assert_ok!(runtime_call.dispatch(frame_system::RawOrigin::None.into()));
		}

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), council_members);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), new_tc);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), council_members);
		assert_eq!(pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(), new_tc);
	});
}

#[test]
fn membership_changed_callbacks_are_called() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];

		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			council_members.clone(),
			tc_members.clone(),
		));

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), council_members);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), tc_members);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), council_members);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			tc_members
		);

		// Verify that sufficients were incremented for all members
		// This is done by MembershipHandler via frame_system::inc_sufficients
		for member in &council_members {
			let account = frame_system::Pallet::<Test>::account(member);
			assert!(
				account.sufficients == 1,
				"Council member {} should have sufficients > 0",
				member
			);
		}

		for member in &tc_members {
			let account = frame_system::Pallet::<Test>::account(member);
			assert!(account.sufficients == 1, "TC member {} should have sufficients > 0", member);
		}
	});
}

#[test]
fn empty_council_members_list_fails() {
	new_test_ext().execute_with(|| {
		let tc_members = vec![4, 5, 6];

		// Attempting to reset with empty council list should fail with EmptyMembers
		assert_noop!(
			FederatedAuthorityObservation::reset_members(
				frame_system::RawOrigin::None.into(),
				vec![],
				tc_members,
			),
			Error::<Test>::EmptyMembers
		);
	});
}

#[test]
fn empty_tc_members_list_fails() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];

		// Attempting to reset with empty TC list should fail with EmptyMembers
		assert_noop!(
			FederatedAuthorityObservation::reset_members(
				frame_system::RawOrigin::None.into(),
				council_members,
				vec![],
			),
			Error::<Test>::EmptyMembers
		);
	});
}

#[test]
fn duplicate_members_are_allowed() {
	new_test_ext().execute_with(|| {
		// In real scenarios, duplicates should be filtered before reaching the pallet
		// But the pallet itself doesn't prevent them
		let members_with_duplicates = vec![1, 2, 2, 3];
		let sorted_members_with_duplicates = vec![1, 2, 2, 3];
		let tc_members = vec![4, 5, 6];

		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			members_with_duplicates,
			tc_members,
		));

		// After sorting, duplicates remain
		assert_eq!(CouncilMembership::members().to_vec(), sorted_members_with_duplicates);
	});
}

#[test]
fn inherent_check_validates_data() {
	new_test_ext().execute_with(|| {
		let initial_council = vec![10, 11, 12];
		let initial_tc = vec![13, 14, 15];
		let new_council = vec![1, 2, 3];
		let new_tc = vec![4, 5, 6];

		// Initialize with some members first
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			initial_council,
			initial_tc,
		));

		// Create inherent data with different members
		let inherent_data = create_inherent_data(new_council, new_tc);
		let call = FederatedAuthorityObservation::create_inherent(&inherent_data);

		assert!(call.is_some());

		// check_inherent should not error with valid data
		if let Some(call) = call {
			assert_ok!(FederatedAuthorityObservation::check_inherent(&call, &inherent_data));
		}
	});
}

#[test]
fn is_inherent_identifies_reset_members_call() {
	new_test_ext().execute_with(|| {
		let council_members = vec![1, 2, 3];
		let tc_members = vec![4, 5, 6];

		let call = crate::Call::<Test>::reset_members {
			council_authorities: council_members,
			technical_committee_authorities: tc_members,
		};

		assert!(FederatedAuthorityObservation::is_inherent(&call));
	});
}

#[test]
fn multiple_consecutive_resets_work() {
	new_test_ext().execute_with(|| {
		let first_council = vec![1, 2, 3];
		let first_tc = vec![4, 5, 6];
		let second_council = vec![7, 8, 9];
		let second_tc = vec![10, 11, 12];

		// First reset
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			first_council,
			first_tc,
		));

		// Second reset
		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			second_council.clone(),
			second_tc.clone(),
		));

		// Verify the second set of members is active
		assert_eq!(CouncilMembership::members().to_vec(), second_council);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), second_tc);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), second_council);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			second_tc
		);
	});
}

#[test]
fn membership_handler_integration_test() {
	new_test_ext().execute_with(|| {
		// Initial state - no members
		assert_eq!(CouncilMembership::members().len(), 0);
		assert_eq!(TechnicalCommitteeMembership::members().len(), 0);

		// Reset with initial members
		let initial_council = vec![1, 2, 3];
		let initial_tc = vec![4, 5, 6];

		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			initial_council.clone(),
			initial_tc.clone(),
		));

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), initial_council);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), initial_tc);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), initial_council);
		assert_eq!(
			pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(),
			initial_tc
		);

		// Verify sufficients were incremented for initial members
		for member in &initial_council {
			let account = frame_system::Pallet::<Test>::account(member);
			assert_eq!(
				account.sufficients, 1,
				"Council member {} should have 1 sufficient",
				member
			);
		}

		// Update members - some old, some new
		let new_council = vec![2, 3, 7]; // 1 is removed, 7 is added, 2 and 3 remain
		let new_tc = vec![5, 8]; // 4 and 6 are removed, 8 is added, 5 remains

		assert_ok!(FederatedAuthorityObservation::reset_members(
			frame_system::RawOrigin::None.into(),
			new_council.clone(),
			new_tc.clone(),
		));

		// Verify members were set via MembershipHandler in both the membership and collective pallets
		assert_eq!(CouncilMembership::members().to_vec(), new_council);
		assert_eq!(TechnicalCommitteeMembership::members().to_vec(), new_tc);
		assert_eq!(pallet_collective::Members::<Test, CouncilCollective>::get(), new_council);
		assert_eq!(pallet_collective::Members::<Test, TechnicalCommitteeCollective>::get(), new_tc);

		// Define removed, added, and continuing members for clearer assertions
		let removed_council_member = 1;
		let removed_tc_members = vec![4, 6];
		let added_council_member = 7;
		let added_tc_member = 8;
		let continuing_council_member = 2;
		let continuing_tc_member = 5;

		// Verify sufficients for outgoing members were decremented
		let account_1 = frame_system::Pallet::<Test>::account(removed_council_member);
		assert_eq!(
			account_1.sufficients, 0,
			"Removed council member {} should have 0 sufficients",
			removed_council_member
		);

		for member in &removed_tc_members {
			let account = frame_system::Pallet::<Test>::account(member);
			assert_eq!(
				account.sufficients, 0,
				"Removed TC member {} should have 0 sufficients",
				member
			);
		}

		// Verify sufficients for new members were incremented
		let account_7 = frame_system::Pallet::<Test>::account(added_council_member);
		assert_eq!(
			account_7.sufficients, 1,
			"New council member {} should have 1 sufficient",
			added_council_member
		);

		let account_8 = frame_system::Pallet::<Test>::account(added_tc_member);
		assert_eq!(
			account_8.sufficients, 1,
			"New TC member {} should have 1 sufficient",
			added_tc_member
		);

		// Verify sufficients for continuing members remain at 1
		let account_2 = frame_system::Pallet::<Test>::account(continuing_council_member);
		assert_eq!(
			account_2.sufficients, 1,
			"Continuing council member {} should still have 1 sufficient",
			continuing_council_member
		);

		let account_5 = frame_system::Pallet::<Test>::account(continuing_tc_member);
		assert_eq!(
			account_5.sufficients, 1,
			"Continuing TC member {} should still have 1 sufficient",
			continuing_tc_member
		);
	});
}

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

use crate::{Config, Error, Event, MotionInfo, Motions, mock::*, weights::WeightInfo};
use frame_support::{
	BoundedBTreeSet, assert_noop, assert_ok,
	dispatch::{DispatchErrorWithPostInfo, Pays, PostDispatchInfo},
};
use pallet_collective::Proposals;
use sp_core::H256;
use sp_runtime::traits::{Dispatchable, Hash};
use sp_std::boxed::Box;

// Helper functions to reduce code duplication

fn create_remark_call(data: Vec<u8>) -> Box<RuntimeCall> {
	Box::new(RuntimeCall::System(frame_system::Call::remark { remark: data }))
}

fn get_motion_hash(call: &RuntimeCall) -> H256 {
	<Test as frame_system::Config>::Hashing::hash_of(call)
}

fn council_origin() -> RuntimeOrigin {
	pallet_collective::RawOrigin::<u64, pallet_collective::Instance1>::Members(2, 3).into()
}

fn tech_origin() -> RuntimeOrigin {
	pallet_collective::RawOrigin::<u64, pallet_collective::Instance2>::Members(2, 3).into()
}

fn invalid_council_origin() -> RuntimeOrigin {
	pallet_collective::RawOrigin::<u64, pallet_collective::Instance1>::Members(1, 3).into()
}

#[test]
fn motion_approve_creates_new_motion() {
	new_test_ext().execute_with(|| {
		// The actual call we want to execute with Root origin eventually
		let actual_call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&actual_call);

		// Council needs to internally vote to approve calling FederatedAuthority::motion_approve
		// The proposal in the collective is to call FederatedAuthority::motion_approve with actual_call
		let fed_auth_call =
			Box::new(RuntimeCall::FederatedAuthority(crate::Call::motion_approve {
				call: actual_call.clone(),
			}));

		// Account 1 (Council member) proposes that Council calls FederatedAuthority::motion_approve
		// Note: When executed, the collective will dispatch with Members(threshold, member_count) origin
		let propose_call = RuntimeCall::Council(pallet_collective::Call::propose {
			threshold: 2, // 2 out of 3 council members need to vote yes - will create Members(2, 3) origin when executed
			proposal: fed_auth_call.clone(),
			length_bound: 1000,
		});
		assert_ok!(propose_call.dispatch(RuntimeOrigin::signed(1)));

		// Get the proposal hash and index
		let proposal_hash = get_motion_hash(&fed_auth_call);
		let proposals = Proposals::<Test, pallet_collective::Instance1>::get();
		let proposal_index = proposals.iter().position(|h| *h == proposal_hash).unwrap() as u32;

		// Account 1 (proposer) votes yes
		let vote_call_1 = RuntimeCall::Council(pallet_collective::Call::vote {
			proposal: proposal_hash,
			index: proposal_index,
			approve: true,
		});
		assert_ok!(vote_call_1.dispatch(RuntimeOrigin::signed(1)));

		// Account 2 votes yes (now we have 2/3 which passes the threshold)
		let vote_call_2 = RuntimeCall::Council(pallet_collective::Call::vote {
			proposal: proposal_hash,
			index: proposal_index,
			approve: true,
		});
		assert_ok!(vote_call_2.dispatch(RuntimeOrigin::signed(2)));

		// Fast-forward past the voting period
		run_to_block(MOTION_DURATION + 1);

		// Now close the proposal to execute it with the Council origin
		let close_call = RuntimeCall::Council(pallet_collective::Call::close {
			proposal_hash,
			index: proposal_index,
			proposal_weight_bound: frame_support::weights::Weight::from_parts(
				10_000_000_000,
				65536,
			),
			length_bound: 10000,
		});
		assert_ok!(close_call.dispatch(RuntimeOrigin::signed(2)));

		// Now the FederatedAuthority::motion_approve should have been called with council origin
		// Let's check the federated authority storage
		let motion = Motions::<Test>::get(motion_hash);

		assert!(motion.is_some(), "Motion should exist in storage after Council approval");
		let motion = motion.unwrap();
		assert_eq!(motion.approvals.len(), 1);
		assert!(motion.approvals.contains(&COUNCIL_PALLET_ID));

		// ends_block is current block (MOTION_DURATION + 1) + MOTION_DURATION
		assert_eq!(motion.ends_block, (MOTION_DURATION + 1) + MOTION_DURATION);
		assert_eq!(motion.call, *actual_call);
	});
}

#[test]
fn motion_approve_adds_approval_to_existing_motion() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		// First approval from Council
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));

		// Second approval from Technical Committee
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call.clone()));

		let motion = Motions::<Test>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len(), 2);
		assert!(motion.approvals.contains(&COUNCIL_PALLET_ID));
		assert!(motion.approvals.contains(&TECHNICAL_COMMITTEE_PALLET_ID));
	});
}

#[test]
fn motion_approve_fails_if_already_approved_by_same_authority() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);

		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));
		assert_noop!(
			FederatedAuthority::motion_approve(council_origin(), call),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(
						<Test as Config>::WeightInfo::motion_approve_already_approved(1)
					),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionAlreadyApproved.into()
			}
		);
	});
}

#[test]
fn motion_approve_fails_from_unauthorized_origin() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);

		// Fails for a signed Origin
		assert_noop!(
			FederatedAuthority::motion_approve(RuntimeOrigin::signed(1), call.clone()),
			sp_runtime::DispatchError::BadOrigin
		);

		// Fails for a unsigned Origin
		assert_noop!(
			FederatedAuthority::motion_approve(RuntimeOrigin::none(), call.clone()),
			sp_runtime::DispatchError::BadOrigin
		);

		// Fails for an expected Authority Body but with the wrong approvals proportion (1/3)
		assert_noop!(
			FederatedAuthority::motion_approve(invalid_council_origin(), call),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn motion_approve_fails_when_exceeding_max_authorities() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		let mut approvals = BoundedBTreeSet::new();

		// Fill approvals with `MAX_NUM_BODIES`
		for i in 1..=MAX_NUM_BODIES {
			approvals.try_insert(i).unwrap();
		}

		Motions::<Test>::insert(
			motion_hash,
			MotionInfo { approvals, ends_block: 20, call: *call.clone() },
		);

		// Trying to increase `approvals` should fail as it is already full with `MAX_NUM_BODIES` length
		assert_noop!(
			FederatedAuthority::motion_approve(council_origin(), call),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(
						<Test as Config>::WeightInfo::motion_approve_exceeds_bounds()
					),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionApprovalExceedsBounds.into()
			}
		);
	});
}

#[test]
fn motion_revoke_removes_approval() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call));

		assert_ok!(FederatedAuthority::motion_revoke(council_origin(), motion_hash));

		let motion = Motions::<Test>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len(), 1);
		assert!(!motion.approvals.contains(&COUNCIL_PALLET_ID)); // Council removed
		assert!(motion.approvals.contains(&TECHNICAL_COMMITTEE_PALLET_ID)); // TechnicalCommittee still there

		assert_eq!(
			last_event(),
			RuntimeEvent::FederatedAuthority(Event::MotionRevoked {
				motion_hash,
				auth_id: COUNCIL_PALLET_ID
			})
		);
	});
}

#[test]
fn motion_revoke_removes_motion_when_last_approval_removed() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call));

		assert_ok!(FederatedAuthority::motion_revoke(council_origin(), motion_hash));

		assert!(Motions::<Test>::get(motion_hash).is_none());

		let events = federated_authority_events();
		let motion_approved = events.iter().find(|e| matches!(e, Event::MotionApproved { motion_hash: mh, auth_id: COUNCIL_PALLET_ID } if *mh == motion_hash));
		let motion_revoked = events.iter().find(|e| matches!(e, Event::MotionRevoked { motion_hash: mh, auth_id: COUNCIL_PALLET_ID } if *mh == motion_hash));
		let motion_removed = events.iter().find(|e| matches!(e, Event::MotionRemoved { motion_hash: mh } if *mh == motion_hash));

		assert!(motion_approved.is_some(), "MotionApproved event should be emitted");
		assert!(motion_revoked.is_some(), "MotionRevoked event should be emitted");
		assert!(motion_removed.is_some(), "MotionRemoved event should be emitted");
	});
}

#[test]
fn motion_revoke_fails_if_not_approved_by_authority() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call));

		assert_noop!(
			FederatedAuthority::motion_revoke(tech_origin(), motion_hash),
			// Error::<Test>::MotionApprovalMissing
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(
						<Test as Config>::WeightInfo::motion_revoke_approval_missing(1)
					),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionApprovalMissing.into()
			}
		);
	});
}

#[test]
fn motion_revoke_fails_if_motion_not_found() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let motion_hash = H256::from([1u8; 32]);

		assert_noop!(
			FederatedAuthority::motion_revoke(council_origin(), motion_hash),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_revoke_not_found()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionNotFound.into()
			}
		);
	});
}

#[test]
fn motion_revoke_fails_from_unauthorized_origin() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let motion_hash = H256::from([1u8; 32]);

		// Fails for a signed origin
		assert_noop!(
			FederatedAuthority::motion_revoke(RuntimeOrigin::signed(1), motion_hash),
			sp_runtime::DispatchError::BadOrigin
		);

		// Fails for an unsigned origin
		assert_noop!(
			FederatedAuthority::motion_revoke(RuntimeOrigin::none(), motion_hash),
			sp_runtime::DispatchError::BadOrigin
		);

		// Fails for an expected Authority Body but with the wrong approvals proportion (1/3)
		assert_noop!(
			FederatedAuthority::motion_revoke(invalid_council_origin(), motion_hash),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn motion_close_dispatches_when_approved() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		// Both Council and TechnicalCommittee approve (unanimous = 2/2)
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call));

		// Now can close the approved motion
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(1), motion_hash));

		assert!(Motions::<Test>::get(motion_hash).is_none());

		// Check that both MotionDispatched and MotionRemoved events were emitted
		let events = federated_authority_events();
		let dispatched_event = events.iter().find(|e| matches!(e, Event::MotionDispatched { .. }));
		let removed_event = events.iter().find(|e| matches!(e, Event::MotionRemoved { .. }));

		assert!(dispatched_event.is_some(), "MotionDispatched event should be emitted");
		assert!(removed_event.is_some(), "MotionRemoved event should be emitted");
	});
}

#[test]
fn motion_close_removes_expired_motion() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		// Only one approval (not enough for unanimous requirement)
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call));

		// Cannot close before expiry (not approved and not expired)
		assert_noop!(
			FederatedAuthority::motion_close(RuntimeOrigin::signed(1), motion_hash),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_close_still_ongoing()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionNotEnded.into()
			}
		);

		// Fast forward to expiry
		run_to_block(1 + MOTION_DURATION);

		// Now can close the expired motion
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(1), motion_hash));

		assert!(Motions::<Test>::get(motion_hash).is_none());

		// Should emit both MotionExpired and MotionRemoved events
		let events = federated_authority_events();
		assert!(
			events.iter().any(|e| matches!(e, Event::MotionExpired { .. })),
			"MotionExpired event should be emitted"
		);
		assert!(
			events.iter().any(|e| matches!(e, Event::MotionRemoved { .. })),
			"MotionRemoved event should be emitted"
		);
	});
}

#[test]
fn motion_close_fails_if_not_approved_and_not_expired() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		// Only Council approves (not unanimous)
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call));

		// Try to close before expiry (should fail)
		run_to_block(10);

		assert_noop!(
			FederatedAuthority::motion_close(RuntimeOrigin::signed(1), motion_hash),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_close_still_ongoing()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionNotEnded.into()
			}
		);
	});
}

#[test]
fn motion_close_fails_if_motion_not_found() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let motion_hash = H256::from([1u8; 32]);

		assert_noop!(
			FederatedAuthority::motion_close(RuntimeOrigin::signed(1), motion_hash),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_close_not_found()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionNotFound.into()
			}
		);
	});
}

#[test]
fn motion_close_fails_from_unsigned_origin() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let motion_hash = H256::from([1u8; 32]);

		// Fails for an unsigned origin
		assert_noop!(
			FederatedAuthority::motion_close(RuntimeOrigin::none(), motion_hash),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn federated_authority_proportion_works() {
	use crate::{FederatedAuthorityEnsureProportionAtLeast, FederatedAuthorityProportion};

	// Test unanimous proportion (1/1)
	type Unanimous = FederatedAuthorityEnsureProportionAtLeast<1, 1>;
	assert!(!Unanimous::reached_proportion(0, 2));
	assert!(!Unanimous::reached_proportion(1, 2));
	assert!(Unanimous::reached_proportion(2, 2));

	// Test 2/3 proportion
	type TwoThirds = FederatedAuthorityEnsureProportionAtLeast<2, 3>;
	assert!(!TwoThirds::reached_proportion(1, 6));
	assert!(!TwoThirds::reached_proportion(3, 6));
	assert!(TwoThirds::reached_proportion(4, 6));
	assert!(TwoThirds::reached_proportion(6, 6));
}

#[test]
fn motion_approve_fails_after_motion_ended() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let call = create_remark_call(vec![1, 2, 3]);

		// Council approves the motion
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));

		// Fast-forward past the motion end time
		run_to_block(MOTION_DURATION + 2);

		// Try to approve from Technical Committee after motion has ended
		assert_noop!(
			FederatedAuthority::motion_approve(tech_origin(), call.clone()),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_approve_ended()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionHasEnded.into()
			}
		);
	});
}

#[test]
fn motion_revoke_fails_after_motion_ended() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let call = create_remark_call(vec![1, 2, 3]);
		let motion_hash = get_motion_hash(&call);

		// Both authorities approve the motion
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call.clone()));

		// Fast-forward past the motion end time
		run_to_block(MOTION_DURATION + 2);

		// Try to revoke from Council after motion has ended
		assert_noop!(
			FederatedAuthority::motion_revoke(council_origin(), motion_hash),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_revoke_ended()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionHasEnded.into()
			}
		);

		// Try to revoke from Technical Committee after motion has ended
		assert_noop!(
			FederatedAuthority::motion_revoke(tech_origin(), motion_hash),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(<Test as Config>::WeightInfo::motion_revoke_ended()),
					pays_fee: Pays::No
				},
				error: Error::<Test>::MotionHasEnded.into()
			}
		);

		// Verify motion still has both approvals
		let motion = Motions::<Test>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len(), 2);
		assert!(motion.approvals.contains(&COUNCIL_PALLET_ID));
		assert!(motion.approvals.contains(&TECHNICAL_COMMITTEE_PALLET_ID));

		// Motion can still be closed since it's approved and ended
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(1), motion_hash));
	});
}

#[test]
fn multiple_concurrent_motions_work() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let call1 = create_remark_call(vec![1]);
		let call2 = create_remark_call(vec![2]);
		let call3 = create_remark_call(vec![3]);

		let motion_hash1 = get_motion_hash(&call1);
		let motion_hash2 = get_motion_hash(&call2);
		let motion_hash3 = get_motion_hash(&call3);

		// Create all motions with different authorities
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call1.clone()));
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call2.clone()));
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call3.clone()));

		// Add second approvals to make them unanimous
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call1));
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call2));
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call3));

		// Fast forward to end of motion period
		run_to_block(1 + MOTION_DURATION);

		// All motions should be closeable now
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(10), motion_hash1));
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(10), motion_hash2));
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(10), motion_hash3));

		assert!(Motions::<Test>::get(motion_hash1).is_none());
		assert!(Motions::<Test>::get(motion_hash2).is_none());
		assert!(Motions::<Test>::get(motion_hash3).is_none());

		// All three motions should have MotionDispatched and MotionRemoved events
		let events = federated_authority_events();
		let dispatched_count =
			events.iter().filter(|e| matches!(e, Event::MotionDispatched { .. })).count();
		let removed_count =
			events.iter().filter(|e| matches!(e, Event::MotionRemoved { .. })).count();
		assert_eq!(dispatched_count, 3, "Should have 3 MotionDispatched events");
		assert_eq!(removed_count, 3, "Should have 3 MotionRemoved events");
	});
}

#[test]
fn motion_dispatchs_with_root_origin() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		// A call that requires `ensure_root(origin)`
		let call = Box::new(RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0 }));

		let motion_hash = get_motion_hash(&call);

		// Both authorities need to approve for unanimous decision
		assert_ok!(FederatedAuthority::motion_approve(council_origin(), call.clone()));
		assert_ok!(FederatedAuthority::motion_approve(tech_origin(), call));

		// Fast forward to end of motion period
		run_to_block(1 + MOTION_DURATION);

		// Motion is approved, anyone can close it
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(10), motion_hash));

		// Motion was removed even though call failed
		assert!(Motions::<Test>::get(motion_hash).is_none());

		let events = federated_authority_events();
		// events[0] & events[1] are `Event::MotionApproved`
		assert_eq!(events[2], Event::MotionDispatched { motion_hash, motion_result: Ok(()) });
	});
}

#[test]
fn complete_collective_to_federated_flow() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		// This test demonstrates the complete flow from collective voting to federated execution

		// Step 1: Define the actual call we want to execute with Root origin
		let actual_call = create_remark_call(vec![42, 43, 44]);
		let motion_hash = get_motion_hash(&actual_call);

		// Step 2: Council internally votes to approve the federated motion
		// The proposal is to call FederatedAuthority::motion_approve
		let council_fed_call =
			Box::new(RuntimeCall::FederatedAuthority(crate::Call::motion_approve {
				call: actual_call.clone(),
			}));

		// Council member 1 proposes
		assert_ok!(
			RuntimeCall::Council(pallet_collective::Call::propose {
				threshold: 2,
				proposal: council_fed_call.clone(),
				length_bound: 1000,
			})
			.dispatch(RuntimeOrigin::signed(1))
		);

		let council_proposal_hash = get_motion_hash(&council_fed_call);
		let council_proposals = Proposals::<Test, pallet_collective::Instance1>::get();
		let council_proposal_index =
			council_proposals.iter().position(|h| *h == council_proposal_hash).unwrap() as u32;

		// Council member 1 (proposer) votes yes
		assert_ok!(
			RuntimeCall::Council(pallet_collective::Call::vote {
				proposal: council_proposal_hash,
				index: council_proposal_index,
				approve: true,
			})
			.dispatch(RuntimeOrigin::signed(1))
		);

		// Council member 2 votes yes (reaching 2/3 majority)
		assert_ok!(
			RuntimeCall::Council(pallet_collective::Call::vote {
				proposal: council_proposal_hash,
				index: council_proposal_index,
				approve: true,
			})
			.dispatch(RuntimeOrigin::signed(2))
		);

		// Close the Council proposal to execute FederatedAuthority::motion_approve
		assert_ok!(
			RuntimeCall::Council(pallet_collective::Call::close {
				proposal_hash: council_proposal_hash,
				index: council_proposal_index,
				proposal_weight_bound: frame_support::weights::Weight::from_parts(
					10_000_000_000,
					65536
				),
				length_bound: 10000,
			})
			.dispatch(RuntimeOrigin::signed(2))
		);

		// Step 3: Technical Committee also internally votes
		let tech_fed_call =
			Box::new(RuntimeCall::FederatedAuthority(crate::Call::motion_approve {
				call: actual_call.clone(),
			}));

		// Tech member 4 proposes
		assert_ok!(
			RuntimeCall::TechnicalCommittee(pallet_collective::Call::propose {
				threshold: 2,
				proposal: tech_fed_call.clone(),
				length_bound: 1000,
			})
			.dispatch(RuntimeOrigin::signed(4))
		);

		let tech_proposal_hash = get_motion_hash(&tech_fed_call);
		let tech_proposals = Proposals::<Test, pallet_collective::Instance2>::get();
		let tech_proposal_index =
			tech_proposals.iter().position(|h| *h == tech_proposal_hash).unwrap() as u32;

		// Tech member 4 (proposer) votes yes
		assert_ok!(
			RuntimeCall::TechnicalCommittee(pallet_collective::Call::vote {
				proposal: tech_proposal_hash,
				index: tech_proposal_index,
				approve: true,
			})
			.dispatch(RuntimeOrigin::signed(4))
		);

		// Tech member 5 votes yes (reaching 2/3 majority)
		assert_ok!(
			RuntimeCall::TechnicalCommittee(pallet_collective::Call::vote {
				proposal: tech_proposal_hash,
				index: tech_proposal_index,
				approve: true,
			})
			.dispatch(RuntimeOrigin::signed(5))
		);

		// Close the Technical Committee proposal to execute FederatedAuthority::motion_approve
		assert_ok!(
			RuntimeCall::TechnicalCommittee(pallet_collective::Call::close {
				proposal_hash: tech_proposal_hash,
				index: tech_proposal_index,
				proposal_weight_bound: frame_support::weights::Weight::from_parts(
					10_000_000_000,
					65536
				),
				length_bound: 10000,
			})
			.dispatch(RuntimeOrigin::signed(5))
		);

		// Step 4: Verify the federated motion now has both approvals
		let motion = Motions::<Test>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len(), 2);
		assert!(motion.approvals.contains(&COUNCIL_PALLET_ID)); // Council
		assert!(motion.approvals.contains(&TECHNICAL_COMMITTEE_PALLET_ID)); // TechnicalCommittee

		// Step 5: Any user can now close the motion to execute it with Root origin
		assert_ok!(FederatedAuthority::motion_close(RuntimeOrigin::signed(100), motion_hash));

		// Step 6: Verify the motion was executed
		assert!(Motions::<Test>::get(motion_hash).is_none());
		let events = federated_authority_events();
		let dispatched_event = events.iter().find(|e| matches!(e, Event::MotionDispatched { .. }));
		assert!(dispatched_event.is_some(), "MotionDispatched event should be emitted");
	});
}

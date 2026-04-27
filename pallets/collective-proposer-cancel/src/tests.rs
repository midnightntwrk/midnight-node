// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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
use frame_support::{assert_err, assert_ok, dispatch::GetDispatchInfo};
use parity_scale_codec::Encode;
use sp_runtime::traits::Hash as _;

fn make_remark_call(payload: u8) -> RuntimeCall {
	RuntimeCall::System(frame_system::Call::remark { remark: vec![payload] })
}

fn propose(who: u64, call: RuntimeCall) -> sp_core::H256 {
	let length_bound = call.encoded_size() as u32;
	let hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
	assert_ok!(Council::propose(RuntimeOrigin::signed(who), 2, Box::new(call), length_bound,));
	hash
}

#[test]
fn proposer_can_cancel_their_own_proposal() {
	new_test_ext().execute_with(|| {
		let hash = propose(1, make_remark_call(7));

		// Sanity: proposal exists.
		assert!(pallet_collective::ProposalOf::<Test, CouncilCollective>::contains_key(hash));

		assert_ok!(ProposerCancel::cancel_proposal(RuntimeOrigin::signed(1), hash));

		// Proposal is gone.
		assert!(!pallet_collective::ProposalOf::<Test, CouncilCollective>::contains_key(hash));
		assert!(!pallet_collective::Voting::<Test, CouncilCollective>::contains_key(hash));
		assert!(!pallet_collective::CostOf::<Test, CouncilCollective>::contains_key(hash));

		let events = proposer_cancel_events();
		assert_eq!(events.len(), 1);
		assert!(matches!(
			events[0],
			Event::ProposalCancelled { proposal_hash, who: 1 } if proposal_hash == hash
		));
	});
}

#[test]
fn non_proposer_cannot_cancel() {
	new_test_ext().execute_with(|| {
		let hash = propose(1, make_remark_call(7));

		assert_err!(
			ProposerCancel::cancel_proposal(RuntimeOrigin::signed(2), hash),
			Error::<Test, CouncilCollective>::NotProposer,
		);

		// Original proposal and its cost record are intact.
		assert!(pallet_collective::ProposalOf::<Test, CouncilCollective>::contains_key(hash));
		assert!(pallet_collective::CostOf::<Test, CouncilCollective>::contains_key(hash));
	});
}

#[test]
fn cancelling_a_missing_proposal_errors() {
	new_test_ext().execute_with(|| {
		let bogus_hash = <Test as frame_system::Config>::Hashing::hash_of(&b"nope".to_vec());

		assert_err!(
			ProposerCancel::cancel_proposal(RuntimeOrigin::signed(1), bogus_hash),
			Error::<Test, CouncilCollective>::ProposalMissing,
		);
	});
}

#[test]
fn cancel_after_close_errors_with_proposal_missing() {
	new_test_ext().execute_with(|| {
		let call = make_remark_call(11);
		let length_bound = call.encoded_size() as u32;
		let hash = propose(1, call.clone());

		// Reach the threshold (2 of 3 members vote aye).
		assert_ok!(Council::vote(RuntimeOrigin::signed(1), hash, 0, true));
		assert_ok!(Council::vote(RuntimeOrigin::signed(2), hash, 0, true));
		assert_ok!(Council::close(
			RuntimeOrigin::signed(3),
			hash,
			0,
			call.get_dispatch_info().call_weight,
			length_bound,
		));

		// Proposal is closed; ProposalOf no longer holds it.
		assert!(!pallet_collective::ProposalOf::<Test, CouncilCollective>::contains_key(hash));

		assert_err!(
			ProposerCancel::cancel_proposal(RuntimeOrigin::signed(1), hash),
			Error::<Test, CouncilCollective>::ProposalMissing,
		);
	});
}

#[test]
fn proposer_cannot_cancel_twice() {
	new_test_ext().execute_with(|| {
		let hash = propose(1, make_remark_call(13));

		assert_ok!(ProposerCancel::cancel_proposal(RuntimeOrigin::signed(1), hash));

		assert_err!(
			ProposerCancel::cancel_proposal(RuntimeOrigin::signed(1), hash),
			Error::<Test, CouncilCollective>::ProposalMissing,
		);
	});
}

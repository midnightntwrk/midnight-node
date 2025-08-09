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

use crate::{Config, RuntimeUpgradeVotes, mock};
use crate::{Event, mock::*};
use assert_matches::assert_matches;
use frame_support::{
	BoundedVec, assert_ok, storage,
	traits::{PreimageProvider, PreimageRecipient, QueryPreimage},
};

use midnight_primitives_upgrade::{InherentType, UpgradeProposal};
use sp_core::H256;
use sp_runtime::traits::BlakeTwo256;
use sp_runtime::{app_crypto::sp_core, bounded_vec, testing::UintAuthorityId};
use sp_storage::well_known_keys;

fn upgrade_from_str(spec_version: u32, hash: &str) -> InherentType {
	let hash = hex::decode(hash).unwrap();
	midnight_primitives_upgrade::UpgradeProposal::new(
		spec_version,
		sp_core::H256::from_slice(&hash),
	)
}

// Any steps required before voting can take place
fn voting_preconditions() -> H256 {
	use sp_core::Hasher;
	let code = sp_io::storage::get(well_known_keys::CODE).unwrap();
	let runtime_hash = BlakeTwo256::hash(&code);

	<pallet_preimage::Pallet<Test> as PreimageProvider<H256>>::request_preimage(&runtime_hash);

	assert!(Preimage::is_requested(&runtime_hash));
	runtime_hash
}

#[test]
fn allows_voting() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let requested_preimage_for = voting_preconditions();
		let upgrade = UpgradeProposal::new(4000, requested_preimage_for);
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		let votes = RuntimeUpgradeVotes::<Test>::get();
		let expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![(upgrade.clone(), 1)];
		assert_eq!(votes, expected_votes);
		let events = upgrade_events();
		let voter: <Test as Config>::AuthorityId = UintAuthorityId(1).to_public_key();

		let _expected_event: Event<Test> = Event::Voted { voter: voter.clone(), target: upgrade };

		assert_matches!(&events[0], _expected_event);
	})
}

#[test]
fn ignores_votes_when_no_preimage() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(1);
		voting_preconditions();

		let upgrade = upgrade_from_str(
			4000,
			// Wrong preimage! Voting won't happen
			"2d8c2f6d978ca21712b5f6de36c9d31fa8e96a4fa5d8ff8b0188dfb9e7c171bb",
		);
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		let votes = RuntimeUpgradeVotes::<Test>::get();
		let expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![];
		assert_eq!(votes, expected_votes);
	})
}

#[test]
fn does_not_upgrade_if_insufficient_votes() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(3);
		start_session(1);

		let requested_preimage_for = voting_preconditions();
		let code = storage::unhashed::get_raw(well_known_keys::CODE).unwrap();

		<Preimage as PreimageRecipient<H256>>::note_preimage(BoundedVec::truncate_from(code));

		let upgrade = UpgradeProposal::new(4000, requested_preimage_for);

		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));

		let expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![(upgrade, 5)];
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Session::current_index(), 1);
		start_session(2);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(3);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(4);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert!(Preimage::is_requested(&requested_preimage_for));

		start_session(5);

		let empty_upgrade_state: BoundedVec<
			(UpgradeProposal, u32),
			<Test as Config>::MaxVoteTargets,
		> = bounded_vec![];

		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), empty_upgrade_state);
		assert!(!Preimage::is_requested(&requested_preimage_for));

		let events = upgrade_events();
		assert_matches!(events[0], Event::NoConsensusOnUpgrade);
	})
}

#[test]
fn checks_votes_at_end_of_period_and_performs_upgrade() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(3);
		start_session(1);

		let requested_preimage_for = voting_preconditions();
		let code = storage::unhashed::get_raw(well_known_keys::CODE).unwrap();

		<Preimage as PreimageRecipient<H256>>::note_preimage(BoundedVec::truncate_from(
			code.clone(),
		));

		let upgrade = UpgradeProposal::new(4000, requested_preimage_for);

		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));

		let expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![(upgrade.clone(), 6)];
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert_eq!(Session::current_index(), 1);
		start_session(2);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(3);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(4);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));

		start_session(5);

		let empty_upgrade_state: BoundedVec<
			(UpgradeProposal, u32),
			<Test as Config>::MaxVoteTargets,
		> = bounded_vec![];

		// All related storage items are cleaned up
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), empty_upgrade_state);
		assert!(!Preimage::is_requested(&requested_preimage_for));
		assert_eq!(Preimage::get_preimage(&requested_preimage_for), None);

		let now: u64 = System::block_number();
		let scheduled_for: u64 = now + UpgradeDelay::get() as u64;

		let events = upgrade_events();
		assert_eq!(
			events[0],
			Event::UpgradeScheduled {
				runtime_hash: upgrade.clone().runtime_hash,
				spec_version: upgrade.spec_version,
				scheduled_for
			}
		);
	})
}

#[test]
fn does_nothing_if_preimage_is_not_found() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(3);
		start_session(1);

		let requested_preimage_for = voting_preconditions();
		let code = storage::unhashed::get_raw(well_known_keys::CODE).unwrap();

		<Preimage as PreimageRecipient<H256>>::note_preimage(BoundedVec::truncate_from(
			code.clone(),
		));

		let upgrade = UpgradeProposal::new(4000, requested_preimage_for);

		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));
		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));

		let expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![(upgrade, 6)];
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert_eq!(Session::current_index(), 1);
		start_session(2);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(3);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(4);
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));

		<Preimage as PreimageProvider<H256>>::unrequest_preimage(&requested_preimage_for);

		start_session(5);

		let empty_upgrade_state: BoundedVec<
			(UpgradeProposal, u32),
			<Test as Config>::MaxVoteTargets,
		> = bounded_vec![];

		// All related storage items are cleaned up
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), empty_upgrade_state);
		assert!(!Preimage::is_requested(&requested_preimage_for));
		assert_eq!(Preimage::get_preimage(&requested_preimage_for), None);

		let events = upgrade_events();
		assert_matches!(
			events[0],
			Event::NoUpgradePreimageNotRequested { preimage_hash } if preimage_hash == requested_preimage_for
		);
	})
}

#[test]
fn does_not_count_non_incrementing_votes() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(3);
		start_session(1);

		let requested_preimage_for = voting_preconditions();
		let _code = storage::unhashed::get_raw(well_known_keys::CODE).unwrap();
		let upgrade = UpgradeProposal::new(0, requested_preimage_for);

		assert_ok!(Upgrade::propose_or_vote_upgrade(RuntimeOrigin::none(), upgrade.clone()));

		let expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![];
		assert_eq!(RuntimeUpgradeVotes::<Test>::get(), expected_votes);
	})
}

#[test]
fn does_nothing_if_no_votes() {
	mock::new_test_ext().execute_with(|| {
		System::set_block_number(3);
		start_session(1);

		let requested_preimage_for = voting_preconditions();
		let code = storage::unhashed::get_raw(well_known_keys::CODE).unwrap();

		<Preimage as PreimageRecipient<H256>>::note_preimage(BoundedVec::truncate_from(
			code.clone(),
		));

		let upgrade = UpgradeProposal::new(4000, requested_preimage_for);

		let _expected_votes: BoundedVec<(UpgradeProposal, u32), <Test as Config>::MaxVoteTargets> =
			bounded_vec![(upgrade, 6)];
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert_eq!(Session::current_index(), 1);
		start_session(2);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(3);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));
		start_session(4);
		assert_eq!(Preimage::get_preimage(&requested_preimage_for).unwrap(), code.clone());
		assert!(Preimage::is_requested(&requested_preimage_for));

		start_session(5);

		let events = upgrade_events();
		assert_matches!(events[0], Event::NoVotes);
	})
}

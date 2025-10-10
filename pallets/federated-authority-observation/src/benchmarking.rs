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

//! Benchmarking setup for pallet-federated-authority-observation

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as FederatedAuthorityObservation;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

/// Helper function to generate a list of accounts
fn generate_accounts<T: Config>(count: u32) -> Vec<T::AccountId>
where
	T::AccountId: From<u64>,
{
	(0..count).map(|i| (i as u64).into()).collect()
}

/// Helper function to set up initial members for both committees
fn setup_initial_members<T: Config>(council_count: u32, tc_count: u32)
where
	T::AccountId: From<u64>,
{
	let council_members = generate_accounts::<T>(council_count);
	let tc_members = generate_accounts::<T>(tc_count + 1000); // Offset to avoid overlap

	let _ = FederatedAuthorityObservation::<T>::reset_members(
		RawOrigin::None.into(),
		Some(council_members),
		Some(tc_members),
	);
}

#[benchmarks(
	where
		T::AccountId: From<u64>,
)]
mod benchmarks {
	use super::*;

	/// Benchmark resetting only Council members
	/// Variable `n`: Number of council members to reset
	#[benchmark]
	fn reset_council_members(n: Linear<1, 100>) {
		// Setup: Create initial state with some members
		setup_initial_members::<T>(10, 10);

		let new_council_members = generate_accounts::<T>(n);

		#[extrinsic_call]
		reset_members(RawOrigin::None, Some(new_council_members), None);

		// Verify the members were set correctly
		let current_members = T::CouncilMembershipHandler::sorted_members();
		assert_eq!(current_members.len(), n as usize);
	}

	/// Benchmark resetting only Technical Committee members
	/// Variable `n`: Number of technical committee members to reset
	#[benchmark]
	fn reset_technical_committee_members(n: Linear<1, 100>) {
		// Setup: Create initial state with some members
		setup_initial_members::<T>(10, 10);

		let new_tc_members = generate_accounts::<T>(n + 1000); // Offset to avoid overlap

		#[extrinsic_call]
		reset_members(RawOrigin::None, None, Some(new_tc_members));

		// Verify the members were set correctly
		let current_members = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(current_members.len(), n as usize);
	}

	/// Benchmark resetting both Council and Technical Committee members
	/// Variable `n`: Number of members for each committee
	#[benchmark]
	fn reset_both_committees(n: Linear<1, 100>) {
		// Setup: Create initial state with some members
		setup_initial_members::<T>(10, 10);

		let new_council_members = generate_accounts::<T>(n);
		let new_tc_members = generate_accounts::<T>(n + 1000); // Offset to avoid overlap

		#[extrinsic_call]
		reset_members(RawOrigin::None, Some(new_council_members), Some(new_tc_members));

		// Verify both were set correctly
		let council_current = T::CouncilMembershipHandler::sorted_members();
		let tc_current = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(council_current.len(), n as usize);
		assert_eq!(tc_current.len(), n as usize);
	}

	/// Benchmark the worst case: Maximum members for both committees
	#[benchmark]
	fn reset_both_committees_max() {
		// Setup: Create initial state with some members
		setup_initial_members::<T>(10, 10);

		let max_council = T::CouncilMaxMembers::get();
		let max_tc = T::TechnicalCommitteeMaxMembers::get();

		let new_council_members = generate_accounts::<T>(max_council);
		let new_tc_members = generate_accounts::<T>(max_tc + 1000); // Offset to avoid overlap

		#[extrinsic_call]
		reset_members(RawOrigin::None, Some(new_council_members), Some(new_tc_members));

		// Verify both were set correctly
		let council_current = T::CouncilMembershipHandler::sorted_members();
		let tc_current = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(council_current.len(), max_council as usize);
		assert_eq!(tc_current.len(), max_tc as usize);
	}

	/// Benchmark resetting with unsorted input (worst case for sorting)
	/// Variable `n`: Number of council members (will be in reverse order)
	#[benchmark]
	fn reset_unsorted_members(n: Linear<1, 100>) {
		// Setup: Create initial state with some members
		setup_initial_members::<T>(10, 10);

		// Create members in reverse order to test worst-case sorting
		let mut new_council_members = generate_accounts::<T>(n);
		new_council_members.reverse();

		#[extrinsic_call]
		reset_members(RawOrigin::None, Some(new_council_members), None);

		// Verify the members were set and sorted correctly
		let current_members = T::CouncilMembershipHandler::sorted_members();
		assert_eq!(current_members.len(), n as usize);

		// Verify they are sorted
		for i in 1..current_members.len() {
			assert!(current_members[i - 1] <= current_members[i]);
		}
	}

	/// Benchmark no-op call (both None parameters)
	#[benchmark]
	fn reset_none() {
		// Setup: Create initial state with some members
		setup_initial_members::<T>(10, 10);

		#[extrinsic_call]
		reset_members(RawOrigin::None, None, None);

		// Verify nothing changed
		let council_current = T::CouncilMembershipHandler::sorted_members();
		let tc_current = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(council_current.len(), 10);
		assert_eq!(tc_current.len(), 10);
	}

	/// Benchmark transition from small to large membership
	/// This tests the membership change callbacks with many incoming members
	#[benchmark]
	fn reset_small_to_large() {
		// Setup: Create initial state with minimal members
		setup_initial_members::<T>(1, 1);

		let max_council = T::CouncilMaxMembers::get();
		let new_council_members = generate_accounts::<T>(max_council);

		#[extrinsic_call]
		reset_members(RawOrigin::None, Some(new_council_members), None);

		// Verify the transition happened
		let current_members = T::CouncilMembershipHandler::sorted_members();
		assert_eq!(current_members.len(), max_council as usize);
	}

	/// Benchmark transition from large to small membership
	/// This tests the membership change callbacks with many outgoing members
	#[benchmark]
	fn reset_large_to_small() {
		// Setup: Create initial state with maximum members
		let max_council = T::CouncilMaxMembers::get();
		setup_initial_members::<T>(max_council, 10);

		let new_council_members = generate_accounts::<T>(1);

		#[extrinsic_call]
		reset_members(RawOrigin::None, Some(new_council_members), None);

		// Verify the transition happened
		let current_members = T::CouncilMembershipHandler::sorted_members();
		assert_eq!(current_members.len(), 1);
	}

	impl_benchmark_test_suite!(
		FederatedAuthorityObservation,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}

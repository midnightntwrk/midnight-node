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

use super::*;

use crate::Pallet as FederatedAuthorityObservation;
use frame_benchmarking::{account, v2::*};
use frame_system::RawOrigin;

/// Helper function to generate a list of accounts
fn generate_accounts<T: Config>(count: u32) -> Vec<T::AccountId> {
	(0..count).map(|i| account("member", i, 0)).collect()
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark resetting only Council members
	/// Variable `a`: Number of council members to reset
	/// Variable `b`: Number of technical committee members (unchanged from existing)
	#[benchmark]
	fn reset_members_only_council(
		a: Linear<1, { T::CouncilMaxMembers::get() - 1 }>,
		b: Linear<1, { T::TechnicalCommitteeMaxMembers::get() - 1 }>,
	) {
		// Setup: Create initial state with some members
		let initial_council = generate_accounts::<T>(a + 1);
		let initial_tc = generate_accounts::<T>(b);

		let _ = FederatedAuthorityObservation::<T>::reset_members(
			RawOrigin::None.into(),
			initial_council,
			initial_tc.clone(),
		);

		// Create new council members
		let new_council_members = generate_accounts::<T>(a);

		#[extrinsic_call]
		reset_members(RawOrigin::None, new_council_members, initial_tc);

		// Verify the council members were changed
		let current_council = T::CouncilMembershipHandler::sorted_members();
		assert_eq!(current_council.len(), a as usize);
	}

	/// Benchmark resetting only Technical Committee members
	/// Variable `a`: Number of council members (unchanged from existing)
	/// Variable `b`: Number of technical committee members to reset
	#[benchmark]
	fn reset_members_only_technical_committee(
		a: Linear<1, { T::CouncilMaxMembers::get() - 1 }>,
		b: Linear<1, { T::TechnicalCommitteeMaxMembers::get() - 1 }>,
	) {
		// Setup: Create initial state with some members
		let initial_council = generate_accounts::<T>(a);
		let initial_tc = generate_accounts::<T>(b + 1);

		let _ = FederatedAuthorityObservation::<T>::reset_members(
			RawOrigin::None.into(),
			initial_council.clone(),
			initial_tc,
		);

		// Create new TC members
		let new_tc_members = generate_accounts::<T>(b);

		#[extrinsic_call]
		reset_members(RawOrigin::None, initial_council, new_tc_members);

		// Verify the TC members were changed
		let current_tc = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(current_tc.len(), b as usize);
	}

	/// Benchmark resetting both Council and Technical Committee members
	/// Variable `a`: Number of council members to reset
	/// Variable `b`: Number of technical committee members to reset
	#[benchmark]
	fn reset_members(
		a: Linear<1, { T::CouncilMaxMembers::get() - 1 }>,
		b: Linear<1, { T::TechnicalCommitteeMaxMembers::get() - 1 }>,
	) {
		// Setup: Create initial state with some members
		let initial_council = generate_accounts::<T>(a + 1);
		let initial_tc = generate_accounts::<T>(b + 1);

		let _ = FederatedAuthorityObservation::<T>::reset_members(
			RawOrigin::None.into(),
			initial_council,
			initial_tc,
		);

		// Create new members for both committees
		let new_council_members = generate_accounts::<T>(a);
		let new_tc_members = generate_accounts::<T>(b);

		#[extrinsic_call]
		reset_members(RawOrigin::None, new_council_members, new_tc_members);

		// Verify both were changed
		let council_current = T::CouncilMembershipHandler::sorted_members();
		let tc_current = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(council_current.len(), a as usize);
		assert_eq!(tc_current.len(), b as usize);
	}

	/// Benchmark no-op call (no changes for either committee)
	#[benchmark]
	fn reset_members_none(
		a: Linear<1, { T::CouncilMaxMembers::get() }>,
		b: Linear<1, { T::TechnicalCommitteeMaxMembers::get() }>,
	) {
		// Setup: Create initial state with some members
		let council_members = generate_accounts::<T>(a);
		let tc_members = generate_accounts::<T>(b);

		let _ = FederatedAuthorityObservation::<T>::reset_members(
			RawOrigin::None.into(),
			council_members.clone(),
			tc_members.clone(),
		);

		#[extrinsic_call]
		reset_members(RawOrigin::None, council_members.clone(), tc_members.clone());

		// Verify nothing changed
		let council_current = T::CouncilMembershipHandler::sorted_members();
		let tc_current = T::TechnicalCommitteeMembershipHandler::sorted_members();
		assert_eq!(council_current.len(), a as usize);
		assert_eq!(tc_current.len(), b as usize);
	}

	impl_benchmark_test_suite!(
		FederatedAuthorityObservation,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}

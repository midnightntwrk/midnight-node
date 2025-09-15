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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::{
	BoundedVec,
	traits::{PreimageProvider, QueryPreimage},
};
use frame_system::RawOrigin;
use sp_core::{H256, Hasher};
use sp_runtime::{app_crypto::sp_core, traits::BlakeTwo256};
use sp_std::prelude::*;
use sp_storage::well_known_keys;

fn runtime_hash() -> H256 {
	let code = sp_io::storage::get(well_known_keys::CODE).unwrap();
	let runtime_hash = BlakeTwo256::hash(&code);
	runtime_hash
}

// Set in the state all conditions required before a vote can be cast: preimage must exist
fn voting_preconditions<T: Config>() {
	let code_hash = runtime_hash();
	<T::Preimage as PreimageProvider<H256>>::request_preimage(&code_hash);
	assert!(T::Preimage::is_requested(&code_hash));
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	// Normal scenario where a vote will be processed
	fn propose_or_vote_upgrade() {
		voting_preconditions::<T>();
		let code_hash = runtime_hash();

		let upgrade = UpgradeProposal { spec_version: 7000, runtime_hash: code_hash };

		#[extrinsic_call]
		_(RawOrigin::None, upgrade.clone());

		let votes = RuntimeUpgradeVotes::<T>::get();
		let expected_votes: BoundedVec<(UpgradeProposal, u32), <T as Config>::MaxVoteTargets> =
			BoundedVec::truncate_from(vec![(upgrade, 1)]);
		assert_eq!(votes, expected_votes);
	}

	#[benchmark]
	fn on_session_end() {
		let code_hash = runtime_hash();
		let upgrade = UpgradeProposal { spec_version: 7000, runtime_hash: code_hash };
		let upgrades = BoundedVec::truncate_from(vec![(upgrade, 7)]);

		RuntimeUpgradeVotes::<T>::set(upgrades);
		let session_end_at_block = 288;
		#[block]
		{
			Pallet::<T>::on_session_end(session_end_at_block);
		}
		let expected: BoundedVec<(UpgradeProposal, u32), T::MaxVoteTargets> =
			BoundedVec::truncate_from(vec![]);
		assert_eq!(RuntimeUpgradeVotes::<T>::get(), expected);
	}
}

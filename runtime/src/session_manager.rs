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

use core::marker::PhantomData;
use frame_system::pallet_prelude::BlockNumberFor;
use log::info;
use sidechain_domain::ScEpochNumber;
use sp_session_validator_management::CommitteeMember as _;
use sp_staking::SessionIndex;
use sp_std::vec::Vec;

pub struct ValidatorManagementSessionManager<T> {
	_phantom: PhantomData<T>,
}

impl<T> ValidatorManagementSessionManager<T> {
	pub const fn new() -> Self {
		Self { _phantom: PhantomData }
	}
}

impl<T> Default for ValidatorManagementSessionManager<T> {
	fn default() -> Self {
		Self::new()
	}
}

/// SessionManager, which takes committee from pallet_session_validator_management.
impl<T: pallet_session_validator_management::Config + pallet_sidechain::Config>
	pallet_partner_chains_session::SessionManager<T::AccountId, T::AuthorityKeys>
	for ValidatorManagementSessionManager<T>
{
	fn new_session_genesis(
		_new_index: SessionIndex,
	) -> Option<Vec<(T::AccountId, T::AuthorityKeys)>> {
		Some(
			pallet_session_validator_management::Pallet::<T>::current_committee_storage()
				.committee
				.into_iter()
				.map(|member| (member.authority_id().into(), member.authority_keys()))
				.collect::<Vec<_>>(),
		)
	}

	// Instead of Some((*).expect) we could just use (*). However, we rather panic in presence of
	// important programming errors.
	fn new_session(new_index: SessionIndex) -> Option<Vec<(T::AccountId, T::AuthorityKeys)>> {
		info!("New session {new_index}");
		Some(
			pallet_session_validator_management::Pallet::<T>::rotate_committee_to_next_epoch()
				.expect(
					"Session should never end without current epoch validators defined. \
				Check ShouldEndSession implementation or if it is used before starting new session",
				)
				.into_iter()
				.map(|member| (member.authority_id().into(), member.authority_keys()))
				.collect(),
		)
	}

	fn end_session(end_index: SessionIndex) {
		info!("End session {end_index}");
	}

	// Session is expected to be at least 1 block behind sidechain epoch.
	fn start_session(start_index: SessionIndex) {
		let epoch_number = T::current_epoch_number();
		info!("Start session {start_index}, epoch {epoch_number}");
	}
}

/// This implementation tries to end each session in the first block of each sidechain epoch in which
/// the committee for the epoch is defined.
impl<T> pallet_partner_chains_session::ShouldEndSession<BlockNumberFor<T>>
	for ValidatorManagementSessionManager<T>
where
	T: pallet_sidechain::Config,
	T: pallet_session_validator_management::Config<ScEpochNumber = ScEpochNumber>,
{
	fn should_end_session(_n: BlockNumberFor<T>) -> bool {
		let current_epoch_number = T::current_epoch_number();

		current_epoch_number
			> pallet_session_validator_management::Pallet::<T>::current_committee_storage().epoch
			&& pallet_session_validator_management::Pallet::<T>::next_committee().is_some()
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		mock::{Test, advance_block, alice, new_test_ext, until_epoch},
		session_manager::ValidatorManagementSessionManager,
	};
	use pallet_partner_chains_session::ShouldEndSession;

	pub const IRRELEVANT: u64 = 2;
	#[test]
	fn should_end_session_if_last_one_ended_late_and_new_committee_is_defined() {
		new_test_ext().execute_with(|| {
			advance_block();
			until_epoch(2, &|| {});
			assert!(!ValidatorManagementSessionManager::<Test>::should_end_session(IRRELEVANT));
			crate::tests::set_committee_through_inherent_data(&[alice()]);
			assert!(ValidatorManagementSessionManager::<Test>::should_end_session(IRRELEVANT));
		});
	}
}

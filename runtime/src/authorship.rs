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

use frame_support::traits::FindAuthor;
use sp_session_validator_management::CommitteeMember;

pub fn current_block_author_aura_index<T: pallet_aura::Config>() -> Option<usize> {
	let digest = <frame_system::Pallet<T>>::digest();
	let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());
	pallet_aura::Pallet::<T>::find_author(pre_runtime_digests).map(|i| i as usize)
}

pub fn current_block_author<
	T: pallet_aura::Config + pallet_session_validator_management::Config,
>() -> <T as pallet_session_validator_management::Config>::AuthorityId {
	let author = current_block_author_aura_index::<T>()
		.expect("Each aura block should have an author encoded in the digest");
	pallet_session_validator_management::Pallet::<T>::get_current_authority_round_robin(author)
		.expect(
			"Aura authorities must match session committee management, thus aura index can't be too big",
		)
		.authority_id()
}

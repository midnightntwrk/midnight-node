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

//! The producer of the block being executed.
//!
//! BABE import accepts only blocks whose author index names a seat of the epoch's authorities,
//! which are the current committee, so the lookup cannot fail on a correct chain. If it does, the
//! block gets no producer and a defensive failure is reported instead of halting the chain.

use authority_selection_inherents::CommitteeMember;
use core::marker::PhantomData;
use frame_support::traits::{Defensive, FindAuthor};
use midnight_primitives_block_production::{Attribution, BlockProducer, producer_of};
use sidechain_domain::ScEpochNumber;

/// [`BlockProducer`] over the current committee and the active consensus engine of `T`. Costs two
/// storage reads: the digest and the current committee.
pub struct CommitteeBlockProducer<T>(PhantomData<T>);

impl<T, AuthorityId, AuthorityKeys> BlockProducer for CommitteeBlockProducer<T>
where
	T: pallet_consensus_engine::Config
		+ pallet_session_validator_management::Config<
			ScEpochNumber = ScEpochNumber,
			CommitteeMember = CommitteeMember<AuthorityId, AuthorityKeys>,
		>,
	AuthorityId: Clone,
	AuthorityKeys: Clone,
{
	fn current() -> Attribution {
		let digest = frame_system::Pallet::<T>::digest();
		let seat = pallet_consensus_engine::Pallet::<T>::find_author(
			digest.logs.iter().filter_map(|item| item.as_pre_runtime()),
		);
		let committee =
			pallet_session_validator_management::Pallet::<T>::current_committee_storage();
		let member = seat
			.and_then(|seat| committee.committee.get(seat as usize))
			.defensive_proof("the node imports only blocks whose author is a committee seat");
		Attribution { epoch: committee.epoch.into(), producer: member.map(producer_of) }
	}
}

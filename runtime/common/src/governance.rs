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

use frame_support::traits::{
	ChangeMembers, InitializeMembers, SortedMembers, UnfilteredDispatchable,
};
use pallet_collective::{DefaultVote, MemberCount};
use sp_runtime::traits::Dispatchable;
use sp_std::{marker::PhantomData, vec::Vec};

/// Wrapper struct to handle frame_system sufficients and delegate
/// `InitializeMembers` and `ChangeMembers` calls to `P`.
pub struct MembershipHandler<T, P>(PhantomData<(T, P)>)
where
	T: frame_system::Config,
	P: InitializeMembers<T::AccountId> + ChangeMembers<T::AccountId>;

impl<T, P> InitializeMembers<T::AccountId> for MembershipHandler<T, P>
where
	T: frame_system::Config,
	P: InitializeMembers<T::AccountId> + ChangeMembers<T::AccountId>,
{
	fn initialize_members(members: &[T::AccountId]) {
		// First, delegate to P's implementation
		<P as InitializeMembers<T::AccountId>>::initialize_members(members);

		// Then, increase sufficients for all members
		for who in members {
			frame_system::Pallet::<T>::inc_sufficients(who);
		}
	}
}

impl<T, P> ChangeMembers<T::AccountId> for MembershipHandler<T, P>
where
	T: frame_system::Config,
	P: ChangeMembers<T::AccountId> + InitializeMembers<T::AccountId>,
{
	fn change_members_sorted(
		incoming: &[T::AccountId],
		outgoing: &[T::AccountId],
		new: &[T::AccountId],
	) {
		// First, delegate to P's implementation
		<P as ChangeMembers<T::AccountId>>::change_members_sorted(incoming, outgoing, new);

		// Then, handle sufficients
		for who in incoming {
			frame_system::Pallet::<T>::inc_sufficients(who);
		}
		for who in outgoing {
			frame_system::Pallet::<T>::dec_sufficients(who);
		}
	}
}

/// Default votes will be always NO for abstentions
pub struct AlwaysNo;
impl DefaultVote for AlwaysNo {
	fn default_vote(
		_prime_vote: Option<bool>,
		_yes_votes: MemberCount,
		_no_votes: MemberCount,
		_len: MemberCount,
	) -> bool {
		false
	}
}

/// Generic handler for membership observation that dispatches reset_members to a pallet_membership instance
/// The `I` parameter should be the pallet_membership instance (e.g., pallet_membership::Instance1)
pub struct MembershipObservationHandler<T, I>(PhantomData<(T, I)>);

impl<T, I> ChangeMembers<T::AccountId> for MembershipObservationHandler<T, I>
where
	T: frame_system::Config + pallet_membership::Config<I>,
	I: 'static,
	T::RuntimeCall: From<pallet_membership::Call<T, I>> + Dispatchable,
{
	fn change_members_sorted(
		_incoming: &[T::AccountId],
		_outgoing: &[T::AccountId],
		sorted_new: &[T::AccountId],
	) {
		let call = pallet_membership::Call::<T, I>::reset_members { members: sorted_new.to_vec() };

		let _ = call.dispatch_bypass_filter(frame_system::RawOrigin::None.into());
	}
}

impl<T, I> SortedMembers<T::AccountId> for MembershipObservationHandler<T, I>
where
	T: frame_system::Config + pallet_membership::Config<I>,
	I: 'static,
{
	fn sorted_members() -> Vec<T::AccountId> {
		pallet_membership::Members::<T, I>::get().to_vec()
	}

	fn count() -> usize {
		pallet_membership::Members::<T, I>::decode_len().unwrap_or(0)
	}
}

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

//! Allow the original proposer of a `pallet_collective` proposal to cancel
//! their own proposal early.
//!
//! `pallet_collective::disapprove_proposal` and `kill` both require Root,
//! which on this chain is only reachable through a successful federated
//! motion (a 5-day vote). That leaves a proposer who realises their proposal
//! is wrong with no way to withdraw it short of organising enough NO votes
//! or waiting out the voting window.
//!
//! This pallet provides a `cancel_proposal` extrinsic gated on `Signed`. The
//! caller must match the proposer recorded in `pallet_collective::CostOf`.
//! For that lookup to work, the runtime must configure
//! `pallet_collective::Config::Consideration` with a type whose `is_none()`
//! returns `false` — `runtime_common::governance::RecordProposer` is a
//! no-deposit `MaybeConsideration` impl suited to this. With the default
//! `()` Consideration, `CostOf` is never written, and every call to
//! `cancel_proposal` returns `ProposerNotRecorded`.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::*, traits::Consideration};
	use frame_system::pallet_prelude::*;
	use pallet_collective::WeightInfo as CollectiveWeightInfo;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>:
		frame_system::Config<RuntimeEvent: From<Event<Self, I>>> + pallet_collective::Config<I>
	{
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The proposal does not exist in the collective. It may have already
		/// been closed, disapproved, or never been proposed.
		ProposalMissing,
		/// No proposer is recorded for this proposal. The runtime is
		/// configured with a `Consideration` that does not write `CostOf`
		/// (e.g. unit `()`); without a recorded proposer we cannot prove the
		/// caller's right to cancel.
		ProposerNotRecorded,
		/// The caller is not the original proposer of this proposal.
		NotProposer,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The original proposer cancelled their own proposal.
		ProposalCancelled { proposal_hash: T::Hash, who: T::AccountId },
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Cancel a `pallet_collective` proposal early. May only be called by
		/// the account that originally submitted the proposal.
		#[pallet::call_index(0)]
		#[pallet::weight((
			<T as pallet_collective::Config<I>>::WeightInfo::disapprove_proposal(
				<T as pallet_collective::Config<I>>::MaxProposals::get(),
			),
			DispatchClass::Operational,
		))]
		pub fn cancel_proposal(
			origin: OriginFor<T>,
			proposal_hash: T::Hash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			ensure!(
				pallet_collective::ProposalOf::<T, I>::contains_key(proposal_hash),
				Error::<T, I>::ProposalMissing,
			);

			let (proposer, _) = pallet_collective::CostOf::<T, I>::get(proposal_hash)
				.ok_or(Error::<T, I>::ProposerNotRecorded)?;
			ensure!(who == proposer, Error::<T, I>::NotProposer);

			// Release any held deposit. No-op for unit `()` / `RecordProposer`;
			// real-deposit configurations get their cost returned here.
			if let Some((owner, cost)) = pallet_collective::CostOf::<T, I>::take(proposal_hash) {
				<_ as Consideration<T::AccountId, u32>>::drop(cost, &owner)?;
			}

			let proposal_count =
				pallet_collective::Pallet::<T, I>::do_disapprove_proposal(proposal_hash);

			Self::deposit_event(Event::ProposalCancelled { proposal_hash, who });

			Ok(Some(<T as pallet_collective::Config<I>>::WeightInfo::disapprove_proposal(
				proposal_count,
			))
			.into())
		}
	}
}

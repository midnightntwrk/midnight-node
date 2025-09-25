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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod weights;

pub use weights::*;

use frame_support::inherent::IsFatalError;
use frame_support::traits::Bounded;

use midnight_primitives_upgrade::{
	INHERENT_IDENTIFIER, UpgradeProposal, UpgradeProposalInherentData,
};
use parity_scale_codec::Encode;

pub use pallet::*;

/// Type for Scale-encoded data provided by the block author
pub type CallOf<T> = <T as frame_system::Config>::RuntimeCall;
pub type BoundedCallOf<T> = Bounded<CallOf<T>, <T as frame_system::Config>::Hashing>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_support::sp_runtime::{RuntimeAppPublic, traits::BlakeTwo256};
	use frame_system::SetCode;
	use frame_system::pallet_prelude::*;
	use itertools::Itertools;
	use sp_core::H256;
	use sp_runtime::{Saturating, traits::Hash};
	use sp_staking::SessionIndex;
	use sp_version::RuntimeVersion;

	use frame_support::{
		BoundedVec, PalletId,
		pallet_prelude::Get,
		traits::{
			PreimageProvider, PreimageRecipient, QueryPreimage, StorePreimage,
			schedule::{DispatchTime, HIGHEST_PRIORITY, v3::Named as ScheduleNamed},
		},
	};
	use scale_info::prelude::vec;

	/// Maximum size of preimage we can store is 4mb.
	pub const MAX_PREIMAGE_SIZE: u32 = 4 * 1024 * 1024;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type AuthorityId: Member
			+ Parameter
			+ RuntimeAppPublic
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;

		type MaxValidators: Get<u32>;
		/// Maximum amount of concurrent upgrades which can be voted on at once
		type MaxVoteTargets: Get<u32>;
		type Preimage: QueryPreimage<H = BlakeTwo256>
			+ StorePreimage
			+ PreimageProvider<H256>
			+ PreimageRecipient<H256>;

		/// The Lottery's pallet id
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Overarching type of all pallets origins.
		type PalletsOrigin: From<frame_system::RawOrigin<Self::AccountId>>;

		/// The Scheduler.
		type Scheduler: ScheduleNamed<
				BlockNumberFor<Self>,
				CallOf<Self>,
				Self::PalletsOrigin,
				Hasher = Self::Hashing,
			>;
		type SetCode: SetCode<Self>;
		type SessionsPerVotingPeriod: Get<u32>;

		#[pallet::constant]
		/// Number of blocks before any given scheduled upgrade occurs.
		type UpgradeDelay: Get<BlockNumberFor<Self>>;

		#[pallet::constant]
		/// Percentage of the current validator set who must vote on the upgrade in order for it to pass
		type UpgradeVoteThreshold: Get<sp_arithmetic::Percent>;
		type ValidatorSet: Get<BoundedVec<Self::AuthorityId, Self::MaxValidators>>;

		/// Information on runtime weights.
		type WeightInfo: WeightInfo;
		/// Get current Runtime Version
		fn spec_version() -> RuntimeVersion;
		/// Get authority for the current block
		fn current_authority() -> Option<Self::AuthorityId>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Signal an issue when attempting a runtime upgrade, in a context where pallet errors are not accessible
		CouldNotScheduleRuntimeUpgrade { runtime_hash: H256, spec_version: u32 },
		/// No votes were made this round
		NoVotes,
		/// Code upgrade managed by this pallet was scheduled
		UpgradeScheduled { runtime_hash: H256, spec_version: u32, scheduled_for: BlockNumberFor<T> },
		/// Validators could not agree on an upgrade, and voting will be reset
		NoConsensusOnUpgrade,
		/// Upgrade was not performed because a preimage of the upgrade request was not found
		NoUpgradePreimageMissing { preimage_hash: H256 },
		/// Upgrade was not performed because the request for its preimage was not found
		NoUpgradePreimageNotRequested { preimage_hash: H256 },
		/// An upgrade was attempted, but the call size exceeded the configured bounds
		UpgradeCallTooLarge { runtime_hash: H256, spec_version: u32 },
		/// A validator has voted on an upgrade
		Voted { voter: T::AuthorityId, target: UpgradeProposal },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Inherent transaction requires current authority information, but this was not able to be retrived from AURA
		CouldNotLoadCurrentAuthority,
		/// An error occurred when calling a runtime upgrade
		RuntimeUpgradeError,
		/// Limit for votes was exceeded
		VoteThresholdExceeded,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type RuntimeUpgradeVotes<T: Config> =
		StorageValue<_, BoundedVec<(UpgradeProposal, u32), T::MaxVoteTargets>, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Vote on a proposed runtime upgrade that is represented by an onchain preimage request
		///
		/// This call should be invoked exactly once per block due to its inherent nature.
		///
		/// The dispatch origin for this call must be _None_.
		///
		/// This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware
		/// that changing the complexity of this call could result exhausting the resources in a
		/// block to execute any other calls.

		#[pallet::call_index(0)]
		#[pallet::weight((
		T::WeightInfo::propose(),
		DispatchClass::Mandatory
		))]
		pub fn propose_or_vote_upgrade(
			origin: OriginFor<T>,
			upgrade: UpgradeProposal,
		) -> DispatchResult {
			ensure_none(origin)?;
			let UpgradeProposal {
				spec_version: proposed_spec_version,
				runtime_hash: proposed_upgrade_hash,
			} = upgrade;
			let current_spec_version = T::spec_version().spec_version;

			// If incremented, we are potentially proposing an upgrade
			if proposed_spec_version > current_spec_version {
				// Only proceed if the proposed upgrade does exist
				if T::Preimage::is_requested(&proposed_upgrade_hash) {
					let mut votes = RuntimeUpgradeVotes::<T>::get();
					// Current authority should be us, since the context is an inherent submitted by the author. This is useful for tracking only, and not vote uniqueness
					let current_authority =
						T::current_authority().ok_or(Error::<T>::CouldNotLoadCurrentAuthority)?;

					// Find whether there is a vote open for our proposed spec version, matched by id and hash
					if let Some(vote_index) = votes.iter().position(|(existing_proposal, _)| {
						existing_proposal.spec_version == proposed_spec_version
							&& existing_proposal.runtime_hash == proposed_upgrade_hash
					}) {
						votes[vote_index].1 += 1;
					} else {
						// Cast the first vote for proposed upgrade
						votes
							.try_push((upgrade.clone(), 1))
							.map_err(|_| Error::<T>::VoteThresholdExceeded)?;
					}

					RuntimeUpgradeVotes::<T>::set(votes);
					// Emit event with voter information, for the purpose of voter tracking
					Self::deposit_event(Event::Voted { voter: current_authority, target: upgrade });
				}
			}
			Ok(())
		}
	}

	// This pallet provides an inherent, as such it implements ProvideInherent trait
	// https://paritytech.github.io/substrate/master/frame_support/inherent/trait.ProvideInherent.html
	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = NoFatalError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			// create and return the extrinsic call if the data could be read and decoded
			Self::get_and_decode_data(data)
				.map(|upgrade| Self::Call::propose_or_vote_upgrade { upgrade })
		}

		// Determine if a call is an inherent extrinsic
		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::propose_or_vote_upgrade { .. })
		}
	}

	impl<T: Config> Pallet<T> {
		fn try_enact_proposed_runtime_upgrade(new_runtime_hash: &H256, spec_version: u32) {
			// Get requested preimage from pallet preimage and call the runtime upgrade with its bytes
			if !T::Preimage::is_requested(new_runtime_hash) {
				Self::deposit_event(Event::NoUpgradePreimageNotRequested {
					preimage_hash: *new_runtime_hash,
				});
				return;
			}

			// Try to get the preimage; if it's not available, emit an event and return early.
			let preimage = match T::Preimage::get_preimage(new_runtime_hash) {
				Some(preimage) => preimage,
				None => {
					Self::deposit_event(Event::NoUpgradePreimageMissing {
						preimage_hash: *new_runtime_hash,
					});
					return;
				},
			};

			let call = frame_system::Call::<T>::set_code { code: preimage };
			let bounded_call = match BoundedVec::try_from(call.encode()) {
				Ok(bounded) => bounded,
				Err(_err) => {
					Self::deposit_event(Event::UpgradeCallTooLarge {
						runtime_hash: *new_runtime_hash,
						spec_version,
					});
					return; // Early return to stop further execution
				},
			};
			let bounded_call_inline = Bounded::Inline(bounded_call);

			// Get unique upgrade id based on hash(pallet_id, spec_version, runtime_hash)
			let mut id = T::PalletId::get().encode();
			id.extend_from_slice(&new_runtime_hash.encode());
			id.extend_from_slice(&spec_version.encode());
			let id_hash = T::Hashing::hash(&id).encode();
			let mut upgrade_id: [u8; 32] = [0; 32];
			upgrade_id.copy_from_slice(&id_hash.as_slice()[0..32]);

			let now = frame_system::Pallet::<T>::block_number();
			let scheduled_for = now.saturating_add(T::UpgradeDelay::get());

			let result = T::Scheduler::schedule_named(
				upgrade_id,
				// Schedule for point in the future, offset by configured delay amount
				DispatchTime::At(scheduled_for),
				None,
				HIGHEST_PRIORITY,
				<T as Config>::PalletsOrigin::from(frame_system::RawOrigin::Root),
				bounded_call_inline,
			);

			if result.is_ok() {
				// Emit event to signal that the upgrade has been scheduled
				Self::deposit_event(Event::UpgradeScheduled {
					runtime_hash: *new_runtime_hash,
					spec_version,
					scheduled_for,
				});
			} else {
				Self::deposit_event(Event::CouldNotScheduleRuntimeUpgrade {
					runtime_hash: *new_runtime_hash,
					spec_version,
				});
			}
		}

		// Logic to perform at the end of every session
		pub fn on_session_end(session: SessionIndex) {
			if session > 1 && session.is_multiple_of(T::SessionsPerVotingPeriod::get()) {
				// Get votes, if there are valid ones
				let votes = RuntimeUpgradeVotes::<T>::get();
				let max_votes = votes.iter().max_set_by_key(|i| i.1);

				match max_votes.len() {
					len if len > 1 => {
						// Tie condition, we should clear the votes and restart
						Self::deposit_event(Event::NoConsensusOnUpgrade);
						Self::clean_votes(votes);
					},
					1 => {
						// In this case, there is a clear winner
						let (votes_info, validators_voted_on) = max_votes[0];
						let validators = T::ValidatorSet::get();
						let required_threshold =
							T::UpgradeVoteThreshold::get() * validators.len() as u32;

						// Check if highest exceeds required_threshold
						if validators_voted_on > &required_threshold {
							// If so, try to perform that runtime upgrade
							Self::try_enact_proposed_runtime_upgrade(
								&votes_info.runtime_hash,
								votes_info.spec_version,
							);
						} else {
							// Consensus was not reached in time. We should clear the votes and restart, since new validators potentially will rotate in.
							Self::deposit_event(Event::NoConsensusOnUpgrade);
						}
						// In all cases, we clear the votes and start fresh
						Self::clean_votes(votes);
					},
					_ => {
						// No votes; do nothing
						Self::deposit_event(Event::NoVotes);
					},
				}
			}
		}

		// Clean all preimages associated with this pallet, as well as voting state
		fn clean_votes(votes: BoundedVec<(UpgradeProposal, u32), T::MaxVoteTargets>) {
			for (upgrade_proposal, _) in votes.iter() {
				// Explicitly check whether each of preimage request and preimages exist before attempting to remove, or else it will panic
				if T::Preimage::is_requested(&upgrade_proposal.runtime_hash) {
					T::Preimage::unrequest_preimage(&upgrade_proposal.runtime_hash);
				}

				if T::Preimage::have_preimage(&upgrade_proposal.runtime_hash) {
					T::Preimage::unnote_preimage(&upgrade_proposal.runtime_hash);
				}
			}
			RuntimeUpgradeVotes::<T>::kill();
		}

		pub fn get_and_decode_data(data: &InherentData) -> Option<UpgradeProposal> {
			data.upgrade_proposal_inherent_data()
				.expect("Runtime upgrade proposal inherent data not correctly encoded")
		}
	}
}

#[derive(Encode)]
pub struct NoFatalError;
impl IsFatalError for NoFatalError {
	fn is_fatal_error(&self) -> bool {
		false
	}
}

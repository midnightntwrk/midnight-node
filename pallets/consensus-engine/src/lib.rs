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

//! Pallet driving the consensus-engine change. It must work together with a compatible node.
//!
//! The chain starts on AURA (`Aura`) and progresses through a sequence
//! of states as it arms, schedules, and performs a flip to BABE block
//! production. The [`ConsensusEngineApi`](midnight_primitives_consensus_engine::ConsensusEngineApi)
//! runtime API surfaces which engine is active for a given state.
//!
//! Once governance has armed BABE, the node is expected to start emitting BABE
//! `PreRuntimeDigest`s that signal secondary slots, using the same authority index as computed
//! by the AURA logic. This should be done once the majority of validators have registered their
//! BABE keys.
//!
//! A further governance action is then required to schedule the update. It should be scheduled
//! only after observing that a finalized block contains a BABE `PreRuntimeDigest`. This
//! information is not available in the runtime, so we rely on a manual action here.
//!
//! Once scheduled, the pallet performs the flip at the last block of the epoch.
//! If the last slot of epoch is empty, then migration is postponed to the last block of the epoch.
//! The 'migration' is supposed to initialize pallet-babe state and transits to the final state `Babe`.
//! The first block of the next epoch is authored with BABE.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub use weights::WeightInfo;

mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use crate::WeightInfo;
	use frame_support::ConsensusEngineId;
	use frame_support::pallet_prelude::*;
	use frame_support::traits::FindAuthor;
	use frame_support::traits::OnTimestampSet;
	use frame_system::pallet_prelude::*;
	use midnight_primitives_consensus_engine::ActiveEngine;
	use sp_consensus_aura::digests::CompatibleDigestItem as AuraCompatibleDigestItem;
	use sp_consensus_babe::digests::CompatibleDigestItem as _;
	use sp_consensus_slots::Slot;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	/// Bootstrap randomness for BABE's genesis epoch at the consensus flip. Mirrors
	/// pallet-babe's own genesis default (zero).
	const BABE_GENESIS_RANDOMNESS: sp_consensus_babe::Randomness = [0u8; 32];

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_aura::Config + pallet_babe::Config {
		/// Origin permitted to drive state transitions.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Midnight (sidechain) epochs should be aligned with BABE epochs,
		/// so they require the same lenght. The flip is performed at an epoch boundary so they stay aligned.
		#[pallet::constant]
		type EpochDuration: Get<u64>;

		/// Weight information for this pallet's extrinsics.
		type WeightInfo: WeightInfo;
	}

	/// The consensus-engine transition state machine.
	#[derive(
		Debug,
		Default,
		Clone,
		Copy,
		PartialEq,
		Eq,
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
	)]
	pub enum State {
		/// AURA block production, the baseline state before any transition is armed.
		#[default]
		Aura,
		/// A flip to BABE has been armed but not yet scheduled. Node is supposed to add PreRuntimeDigest of BABE Secondary Plain slots in this state.
		ArmedBabe,
		/// The flip to BABE is armed to take effect at the last block of an epoch.
		/// Blocks are still produced with AURA until the flip actually commits.
		ScheduledFlip,
		/// The post flip state, migration happened, consensus is BABE.
		Babe,
	}

	impl State {
		/// The consensus engine that is active while in this state.
		pub fn active_engine(&self) -> ActiveEngine {
			match self {
				State::Aura | State::ArmedBabe | State::ScheduledFlip => ActiveEngine::Aura,
				State::Babe => ActiveEngine::Babe,
			}
		}
	}

	/// The current consensus-engine transition state.
	#[pallet::storage]
	pub type EngineState<T: Config> = StorageValue<_, State, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Drives the automatic, non-governance part of the state machine each block.
		///
		/// The current slot is read from the AURA pre-runtime digest (the validated,
		/// authoritative slot while AURA is producing).
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			match EngineState::<T>::get() {
				// Before arming, the node must not emit BABE pre-digests. A block
				// carrying one would let `pallet-babe` self-initialize its genesis
				// epoch prematurely (see `arm_babe_storage`), so reject it. This
				// is deterministic — every node reads the same header — so a
				// misbehaving author's block is rejected on import, not just locally.
				State::Aura => {
					assert!(
						!Self::has_aura_pre_digest_before_babe_pre_digest(),
						"Unique BABE pre-runtime digest present after AURA in state 'Aura'",
					);
				},
				State::ScheduledFlip => {
					if let Some(slot) = Self::current_slot_from_aura_digest()
						&& Self::is_last_slot_of_epoch(slot)
					{
						Self::migrate_to_babe(slot);
						EngineState::<T>::put(State::Babe);
					}
				},
				_ => {},
			}
			<T as Config>::WeightInfo::on_initialize()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Arm the flip to BABE: move `Aura` to `ArmedBabe`.
		///
		/// Governance-gated. A no-op unless the engine is currently `Aura`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::arm_babe())]
		pub fn arm_babe(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			if EngineState::<T>::get() == State::Aura {
				// Pre-seed BABE before the node starts emitting BABE pre-digests, so
				// pallet-babe does not prematurely self-initialize its genesis epoch.
				Self::set_sentinel_babe_genesis_slot();
				EngineState::<T>::put(State::ArmedBabe);
			}
			Ok(())
		}

		/// Schedule the flip to BABE: move `ArmedBabe` to `ScheduledFlip`.
		///
		/// Governance-gated. A no-op unless the engine is currently `ArmedBabe`.
		/// The flip itself commits automatically at the next epoch boundary; see
		/// [`Hooks::on_initialize`].
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_flip())]
		pub fn schedule_flip(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			if EngineState::<T>::get() == State::ArmedBabe {
				EngineState::<T>::put(State::ScheduledFlip);
			}
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The consensus engine currently active, derived from [`EngineState`].
		pub fn active_engine() -> ActiveEngine {
			EngineState::<T>::get().active_engine()
		}

		/// Sets `GenesisSlot` to a non-zero sentinel so pallet-babe's `initialize`
		/// does not self-initialize a genesis epoch and deposit a bogus `NextEpochData`
		/// digest into a header we cannot retract.
		fn set_sentinel_babe_genesis_slot() {
			pallet_babe::GenesisSlot::<T>::put(Slot::from(u64::MAX));
			log::info!(
				target: "consensus-engine",
				"BABE armed: pre-seeded pallet-babe GenesisSlot to suppress premature genesis init.",
			);
		}

		/// Bootstrap `pallet-babe` for its genesis epoch at the flip and log the transition.
		///
		/// `slot` is the last slot of the ending epoch (the current block's slot).
		/// BABE's genesis is the first slot of the next epoch, so its epoch
		/// boundaries stay aligned with the sidechain epochs.
		fn migrate_to_babe(slot: Slot) {
			let babe_genesis_slot = Self::next_epoch_start(slot);

			// BABE and sidechain epochs boundaries should align
			pallet_babe::GenesisSlot::<T>::put(babe_genesis_slot);
			pallet_babe::CurrentSlot::<T>::put(babe_genesis_slot);
			pallet_babe::EpochIndex::<T>::put(0);

			pallet_babe::Randomness::<T>::put(BABE_GENESIS_RANDOMNESS);
			pallet_babe::NextRandomness::<T>::put(BABE_GENESIS_RANDOMNESS);

			log::info!(
				target: "consensus-engine",
				"Consensus engine flip at the last slot ({:?}) of the epoch; \
				BABE genesis slot {:?}, entering Babe state.",
				slot,
				babe_genesis_slot,
			);

			// This will prevent each last-of-epoch block to be committeed,
			// but doesn't stop the chain completly.
			panic!("Issue #1742 adds BABE keys to the runtime");
		}

		fn current_slot_from_aura_digest() -> Option<Slot> {
			frame_system::Pallet::<T>::digest()
				.logs
				.iter()
				.find_map(AuraCompatibleDigestItem::<()>::as_aura_pre_digest)
		}

		/// Returns `true` when the current block's digest carries exactly one BABE
		/// pre-runtime digest, that digest appears *after* an AURA pre-runtime digest,
		/// and its slot matches that AURA digest's slot.
		///
		/// This is the shape a node emits while still on AURA once it has begun
		/// signalling BABE secondary slots at the same slot — the situation the
		/// `State::Aura` guard rejects. Any other arrangement returns `false`: no BABE
		/// digest, a BABE digest before any AURA digest, a slot mismatch, or more than
		/// one BABE digest.
		pub(crate) fn has_aura_pre_digest_before_babe_pre_digest() -> bool {
			let mut aura_slot = None;
			let mut babe_present = false;
			for log in frame_system::Pallet::<T>::digest().logs.iter() {
				if let Some(slot) = AuraCompatibleDigestItem::<()>::as_aura_pre_digest(log) {
					aura_slot = Some(slot);
					continue;
				};

				if let Some(babe) = log.as_babe_pre_digest() {
					if let Some(slot) = aura_slot {
						if babe.slot() != slot {
							// BABE slot different to AURA slot
							return false;
						}
						if babe_present {
							// BABE pre-digest is not unique
							return false;
						}
						babe_present = true
					} else {
						// BABE pre-digest before AURA
						return false;
					}
				}
			}
			babe_present
		}

		fn is_last_slot_of_epoch(slot: Slot) -> bool {
			let duration = <T as Config>::EpochDuration::get().max(1);
			(u64::from(slot) + 1) % duration == 0
		}

		/// The first slot of the epoch after the one containing `slot`.
		fn next_epoch_start(slot: Slot) -> Slot {
			let duration = <T as Config>::EpochDuration::get().max(1);
			let slot = u64::from(slot);
			Slot::from((slot / duration + 1) * duration)
		}
	}

	impl<T: Config> FindAuthor<u32> for Pallet<T> {
		fn find_author<'a, I>(digests: I) -> Option<u32>
		where
			I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
		{
			match Self::active_engine() {
				ActiveEngine::Aura => {
					<pallet_aura::Pallet<T> as FindAuthor<u32>>::find_author(digests)
				},
				ActiveEngine::Babe => {
					<pallet_babe::Pallet<T> as FindAuthor<u32>>::find_author(digests)
				},
			}
		}
	}

	impl<T: Config> OnTimestampSet<T::Moment> for Pallet<T> {
		fn on_timestamp_set(moment: T::Moment) {
			match Self::active_engine() {
				ActiveEngine::Aura => {
					<pallet_aura::Pallet<T> as OnTimestampSet<T::Moment>>::on_timestamp_set(moment)
				},
				ActiveEngine::Babe => {
					<pallet_babe::Pallet<T> as OnTimestampSet<T::Moment>>::on_timestamp_set(moment)
				},
			}
		}
	}
}

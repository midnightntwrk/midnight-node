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

//! Tests for the consensus-engine pallet.

use crate::{Error, State, mock::*, pallet::EngineState};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use midnight_primitives_consensus_engine::ActiveEngine;
use sp_consensus_slots::Slot;
use sp_runtime::DispatchError;

/// Run the pallet's `on_initialize` hook for the current block.
fn on_initialize() {
	ConsensusEngine::on_initialize(System::block_number());
}

#[test]
fn default_state_is_baseline_aura() {
	new_test_ext().execute_with(|| {
		assert_eq!(EngineState::<Test>::get(), State::Aura);
		assert_eq!(ConsensusEngine::active_engine(), ActiveEngine::Aura);
	});
}

#[test]
fn arm_babe_from_baseline() {
	new_test_ext().execute_with(|| {
		// Before arming, BABE's GenesisSlot is unset (its ValueQuery default of 0).
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(0));
		assert_ok!(ConsensusEngine::arm_babe(RuntimeOrigin::root()));
		assert_eq!(EngineState::<Test>::get(), State::ArmedBabe);
		// Arming pre-seeds pallet-babe's GenesisSlot to a sentinel so it does not
		// self-initialize its genesis epoch prematurely.
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(u64::MAX));
		// `ArmedBabe` still authors with AURA.
		assert_eq!(ConsensusEngine::active_engine(), ActiveEngine::Aura);
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest present in state 'Aura'")]
fn baseline_rejects_blocks_with_babe_pre_digest() {
	new_test_ext().execute_with(|| {
		// Default state is `Aura`; a block carrying a BABE pre-digest is rejected.
		start_block_with_babe_pre_digest(100);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest present in state 'Aura'")]
fn baseline_rejects_babe_before_aura() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![babe_pre_digest(100), aura_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest present in state 'Aura'")]
fn baseline_rejects_babe_only() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![babe_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest present in state 'Aura'")]
fn baseline_rejects_mismatched_babe_slot() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest(101)]);
		on_initialize();
	});
}

/// Initialize a block carrying `logs` and evaluate the AURA-then-BABE guard directly.
fn aura_before_babe(logs: Vec<sp_runtime::DigestItem>) -> bool {
	new_test_ext().execute_with(|| {
		start_block_with_logs(logs);
		ConsensusEngine::has_aura_pre_digest_before_babe_pre_digest()
	})
}

#[test]
fn aura_then_matching_babe_is_detected() {
	// The rejected shape: AURA followed by a single BABE digest at the same slot.
	assert!(aura_before_babe(vec![aura_pre_digest(100), babe_pre_digest(100)]));
}

#[test]
fn aura_then_matching_babe_with_unrelated_digest_is_detected() {
	// A pre-runtime digest for another engine between the two is ignored.
	assert!(aura_before_babe(vec![
		aura_pre_digest(100),
		unrelated_pre_digest(),
		babe_pre_digest(100),
	]));
}

#[test]
fn empty_digest_is_not_detected() {
	assert!(!aura_before_babe(vec![]));
}

#[test]
fn aura_only_is_not_detected() {
	assert!(!aura_before_babe(vec![aura_pre_digest(100)]));
}

#[test]
fn babe_only_is_not_detected() {
	// A BABE digest with no preceding AURA digest.
	assert!(!aura_before_babe(vec![babe_pre_digest(100)]));
}

#[test]
fn babe_before_aura_is_not_detected() {
	assert!(!aura_before_babe(vec![babe_pre_digest(100), aura_pre_digest(100)]));
}

#[test]
fn babe_with_mismatched_slot_is_not_detected() {
	assert!(!aura_before_babe(vec![aura_pre_digest(100), babe_pre_digest(101)]));
}

#[test]
fn duplicate_babe_digest_is_not_detected() {
	// Two BABE digests (even matching the AURA slot) are not the unique-digest shape.
	assert!(!aura_before_babe(vec![
		aura_pre_digest(100),
		babe_pre_digest(100),
		babe_pre_digest(100),
	]));
}

#[test]
fn babe_pre_digest_is_allowed_once_armed() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ArmedBabe);
		// Once armed the node is expected to emit BABE pre-digests; no rejection.
		start_block_with_babe_pre_digest(100);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::ArmedBabe);
	});
}

#[test]
fn armed_allows_aura_only_blocks() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ArmedBabe);
		// Old binaries may omit the BABE digest; that is still valid during the armed window.
		start_block_at_slot(100);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::ArmedBabe);
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest must match AURA slot during transition")]
fn armed_rejects_mismatched_babe_slot() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ArmedBabe);
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest(101)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest must match AURA slot during transition")]
fn scheduled_rejects_mismatched_babe_slot() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		// Mid-epoch: flip does not run, but a mismatched BABE digest is still rejected.
		start_block_with_logs(vec![aura_pre_digest(1400), babe_pre_digest(1401)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest must match AURA slot during transition")]
fn armed_rejects_babe_before_aura() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ArmedBabe);
		start_block_with_logs(vec![babe_pre_digest(100), aura_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
fn arm_babe_requires_governance_origin() {
	new_test_ext().execute_with(|| {
		assert_noop!(ConsensusEngine::arm_babe(RuntimeOrigin::signed(1)), DispatchError::BadOrigin);
		assert_noop!(ConsensusEngine::arm_babe(RuntimeOrigin::none()), DispatchError::BadOrigin);
		assert_eq!(EngineState::<Test>::get(), State::Aura);
	});
}

#[test]
fn arm_babe_is_rejected_from_other_states() {
	new_test_ext().execute_with(|| {
		for state in [State::ArmedBabe, State::ScheduledFlip, State::Babe] {
			EngineState::<Test>::put(state);
			assert_noop!(
				ConsensusEngine::arm_babe(RuntimeOrigin::root()),
				Error::<Test>::InvalidEngineState
			);
			assert_eq!(EngineState::<Test>::get(), state);
			// The arm hook only fires on the real Aura -> ArmedBabe transition, so BABE
			// is never pre-seeded from these states.
			assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(0));
		}
	});
}

#[test]
fn schedule_flip_from_armed() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ArmedBabe);

		assert_ok!(ConsensusEngine::schedule_flip(RuntimeOrigin::root()));

		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);
		// A `ScheduledFlip` still authors with AURA until the flip commits.
		assert_eq!(ConsensusEngine::active_engine(), ActiveEngine::Aura);
	});
}

#[test]
fn schedule_flip_requires_governance_origin() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ArmedBabe);
		assert_noop!(
			ConsensusEngine::schedule_flip(RuntimeOrigin::signed(1)),
			DispatchError::BadOrigin
		);
		assert_noop!(
			ConsensusEngine::schedule_flip(RuntimeOrigin::none()),
			DispatchError::BadOrigin
		);
		assert_eq!(EngineState::<Test>::get(), State::ArmedBabe);
	});
}

#[test]
fn schedule_flip_is_rejected_unless_armed() {
	new_test_ext().execute_with(|| {
		for state in [State::Aura, State::ScheduledFlip, State::Babe] {
			EngineState::<Test>::put(state);
			assert_noop!(
				ConsensusEngine::schedule_flip(RuntimeOrigin::root()),
				Error::<Test>::InvalidEngineState
			);
			assert_eq!(EngineState::<Test>::get(), state);
		}
	});
}

#[test]
#[should_panic(expected = "Issue #1742 adds BABE keys to the runtime")]
fn flip_fires_at_the_last_slot_of_the_epoch() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		seed_babe_authorities();

		// The last slot of the epoch (1499 for a 300-slot epoch) attempts the flip.
		// The flip block must carry a matching BABE pre-digest. Completing the flip
		// panics until real BABE authorities are wired in (Issue #1742).
		start_block_with_babe_pre_digest(1499);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required on flip block")]
fn flip_rejects_epoch_end_block_without_babe_pre_digest() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		seed_babe_authorities();

		// An epoch-end AURA block without a BABE pre-digest must be rejected: committing
		// the flip without it would permanently halt authoring.
		start_block_at_slot(1499);
		on_initialize();
	});
}

#[test]
fn flip_postpones_when_babe_authorities_empty() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		// Default Authorities is empty — postpone even when the digest is present.
		assert!(pallet_babe::Authorities::<Test>::get().is_empty());
		start_block_with_babe_pre_digest(1499);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);

		// Once Authorities is populated, the next epoch-end flip proceeds
		// (and currently panics on Issue #1742 after migrate).
		seed_babe_authorities();
		start_block_with_babe_pre_digest(1799);
		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			on_initialize();
		}));
		assert!(result.is_err());
	});
}

#[test]
fn flip_does_not_run_mid_epoch() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		seed_babe_authorities();
		// A mid-epoch block does not trigger the flip (no panic, state unchanged),
		// even when it carries a BABE pre-digest.
		start_block_with_babe_pre_digest(1400);
		on_initialize();

		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);
	});
}

#[test]
fn flip_does_not_run_on_penultimate_or_first_slot_of_next_epoch() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		seed_babe_authorities();
		// Don't flip at the penultimate slot.
		start_block_with_babe_pre_digest(1498);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);

		// The last slot of the epoch (1499) produced no block; the first block of
		// the next epoch lands at 1500. The flip must NOT execute — we only flip on
		// a block seen exactly at an epoch's last slot.
		start_block_with_babe_pre_digest(1500);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);
	});
}

#[test]
#[should_panic(expected = "Issue #1742 adds BABE keys to the runtime")]
fn flip_fires_at_next_epoch_last_slot_when_the_last_slot_is_skipped() {
	new_test_ext().execute_with(|| {
		EngineState::<Test>::put(State::ScheduledFlip);
		seed_babe_authorities();
		// The epoch's last slot (1499) was skipped; the flip waits and fires at the
		// next epoch's last slot (1799), where completing it panics (Issue #1742).
		start_block_with_babe_pre_digest(1799);
		on_initialize();
	});
}

#[test]
fn on_initialize_is_a_no_op_in_stable_states() {
	new_test_ext().execute_with(|| {
		for state in [State::Aura, State::ArmedBabe, State::Babe] {
			EngineState::<Test>::put(state);
			// Even at an epoch's last slot, non-scheduled states never flip.
			start_block_at_slot(1499);
			on_initialize();
			assert_eq!(EngineState::<Test>::get(), state);
		}
	});
}

#[test]
fn should_emit_babe_preruntime_digest_only_while_armed() {
	new_test_ext().execute_with(|| {
		for (state, expected) in [
			(State::Aura, false),
			(State::ArmedBabe, true),
			(State::ScheduledFlip, true),
			(State::Babe, false),
		] {
			EngineState::<Test>::put(state);
			assert_eq!(ConsensusEngine::should_emit_babe_preruntime_digest(), expected);
		}
	});
}

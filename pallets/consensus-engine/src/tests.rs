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

use crate::{
	Error, State, babe_genesis_slot_sentinel,
	mock::*,
	pallet::{EngineState, LastRotationBabeEpoch},
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{OnFinalize, OnInitialize, OnTimestampSet},
};
use midnight_primitives_consensus_engine::ActiveEngine;
use sp_consensus_slots::Slot;
use sp_runtime::DispatchError;

/// Run `on_initialize` for every pallet in construct_runtime order (Babe before
/// ConsensusEngine), matching production hook ordering.
fn on_initialize() {
	AllPalletsWithSystem::on_initialize(System::block_number());
}

/// Close `pallet-babe`'s block, so the next `on_initialize` is a real one — it
/// short-circuits its `initialize` while `Initialized` is still set from this block.
/// Only Babe is finalized: `pallet-timestamp`'s `on_finalize` asserts the timestamp
/// inherent ran, which these digest-level tests do not simulate.
fn finalize_babe_block() {
	Babe::on_finalize(System::block_number());
}

#[test]
fn default_state_is_baseline_aura() {
	new_test_ext().execute_with(|| {
		assert_eq!(EngineState::<Test>::get(), State::Aura);
		assert_eq!(ConsensusEngine::active_engine(), ActiveEngine::Aura);
	});
}

// --- activation (called by the runtime's upgrade migration) ---

/// Externalities shaped like a network about to be upgraded: pallet-babe never saw a BABE
/// pre-digest, so `GenesisSlot` still holds its `ValueQuery` default.
fn pre_activation_ext() -> sp_io::TestExternalities {
	let mut ext = new_test_ext();
	ext.execute_with(pallet_babe::GenesisSlot::<Test>::kill);
	ext
}

fn activate() {
	ConsensusEngine::activate();
}

#[test]
fn activation_pre_seeds_babe_genesis_slot() {
	pre_activation_ext().execute_with(|| {
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(0));

		activate();

		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), babe_genesis_slot_sentinel());
		// Activation lands in the baseline state; nothing is scheduled yet.
		assert_eq!(EngineState::<Test>::get(), State::Aura);
	});
}

#[test]
fn activation_overwrites_a_self_initialized_babe_genesis_slot() {
	pre_activation_ext().execute_with(|| {
		// A dormant pallet-babe may have adopted a slot from a stray digest before the upgrade.
		pallet_babe::GenesisSlot::<Test>::put(Slot::from(77));

		activate();

		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), babe_genesis_slot_sentinel());
	});
}

#[test]
fn activation_block_does_not_self_initialize_babe_genesis() {
	pre_activation_ext().execute_with(|| {
		// Migrations run first, then the hooks see a block that already carries the digest
		// from a migration-aware author.
		activate();
		start_block_with_babe_pre_digest(100);
		on_initialize();

		// pallet-babe kept the sentinel instead of adopting slot 100 as its genesis slot, and
		// deposited no `NextEpochData` digest.
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), babe_genesis_slot_sentinel());
		assert!(
			System::digest().logs.iter().all(|log| log.as_consensus().is_none()),
			"no consensus digest expected on the activation block",
		);
		assert_eq!(EngineState::<Test>::get(), State::Aura);
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn activation_block_requires_the_babe_pre_digest() {
	pre_activation_ext().execute_with(|| {
		// Strict from the first block: nodes are upgraded before the runtime is.
		activate();
		start_block_at_slot(100);
		on_initialize();
	});
}

/// The BABE `ConsensusLog`s deposited into the current block's header.
fn babe_consensus_logs() -> Vec<sp_consensus_babe::ConsensusLog> {
	use parity_scale_codec::Decode as _;
	System::digest()
		.logs
		.iter()
		.filter_map(|log| log.as_consensus())
		.filter(|(id, _)| *id == sp_consensus_babe::BABE_ENGINE_ID)
		.filter_map(|(_, mut payload)| sp_consensus_babe::ConsensusLog::decode(&mut payload).ok())
		.collect()
}

#[test]
fn first_block_of_a_genesis_chain_lets_babe_self_initialize_harmlessly() {
	pre_activation_ext().execute_with(|| {
		// A chain that has the pallet from genesis runs no upgrade migration, so `activate` is
		// never called: pallet-babe still holds the `ValueQuery` default and adopts the first
		// BABE pre-digest's slot as its genesis slot. The block itself is valid.
		start_block_with_babe_pre_digest(100);
		on_initialize();

		assert_eq!(EngineState::<Test>::get(), State::Aura);
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(100));

		// The cost is real and belongs in the header: self-init deposits a `NextEpochData`
		// consensus digest into an AURA block, the item the activation sentinel exists to
		// suppress on an upgraded chain (see
		// `activation_block_does_not_self_initialize_babe_genesis`, which asserts none). It is
		// inert — the AURA import pipeline does not read BABE consensus logs — but it is there,
		// so assert it rather than let "harmlessly" imply the block is unchanged.
		let logs = babe_consensus_logs();
		assert!(
			matches!(logs.as_slice(), [sp_consensus_babe::ConsensusLog::NextEpochData(_)]),
			"expected exactly one NextEpochData log, got {} BABE consensus log(s)",
			logs.len(),
		);
		finalize_babe_block();

		// Harmless because the flip overwrites every value self-init wrote. Note
		// `put_engine_state` leaves the self-initialized `GenesisSlot` alone (it only seeds the
		// sentinel when unset), so the flip really does run from slot 100 here.
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();
		start_block_with_babe_pre_digest(1499);
		on_initialize();

		// Exactly the epoch-0 state an activated chain reaches; cf.
		// `flip_fires_at_the_last_slot_of_the_epoch`, which asserts the same values having
		// started from the sentinel.
		assert_eq!(EngineState::<Test>::get(), State::Babe);
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(1500));
		assert_eq!(pallet_babe::CurrentSlot::<Test>::get(), Slot::from(1499));
		assert_eq!(pallet_babe::EpochIndex::<Test>::get(), 0);
		assert_eq!(pallet_babe::Randomness::<Test>::get(), [0u8; 32]);
		assert_eq!(pallet_babe::NextRandomness::<Test>::get(), [0u8; 32]);
	});
}

// --- `Aura`: every block must carry AURA then a matching BABE `SecondaryPlain` ---

#[test]
fn aura_accepts_aura_then_matching_babe() {
	new_test_ext().execute_with(|| {
		start_block_with_babe_pre_digest(100);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::Aura);
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_aura_only_blocks() {
	new_test_ext().execute_with(|| {
		// Nodes are upgraded before the runtime that activates the pallet, so a block
		// without the BABE digest comes from a non-compliant author — rejected.
		start_block_at_slot(100);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_babe_before_aura() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![babe_pre_digest(100), aura_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_babe_only() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![babe_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_mismatched_babe_slot() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest(101)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_primary_babe_digest() {
	new_test_ext().execute_with(|| {
		// Matching slot, but the wrong variant: Primary carries unverified VRF material.
		start_block_with_logs(vec![aura_pre_digest(100), babe_primary_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_secondary_vrf_babe_digest() {
	new_test_ext().execute_with(|| {
		// Matching slot, but SecondaryVRF carries the same unverified VRF risk as Primary.
		start_block_with_logs(vec![aura_pre_digest(100), babe_secondary_vrf_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_mismatched_authority_index() {
	new_test_ext().execute_with(|| {
		seed_aura_authorities(3);
		// Slot 100 → AURA author 1; claiming 0 must be rejected.
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest_with_authority(100, 0)]);
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
	// The required shape: AURA followed by a single BABE digest at the same slot.
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
fn primary_babe_digest_is_not_detected() {
	// Only `SecondaryPlain` is a valid transition digest: a `Primary` digest at
	// the matching slot carries VRF material no one verified (AURA import).
	assert!(!aura_before_babe(vec![aura_pre_digest(100), babe_primary_pre_digest(100)]));
}

#[test]
fn secondary_vrf_babe_digest_is_not_detected() {
	// `SecondaryVRF` also carries unverified VRF material under AURA import.
	assert!(!aura_before_babe(vec![aura_pre_digest(100), babe_secondary_vrf_pre_digest(100)]));
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
fn babe_with_mismatched_authority_index_is_not_detected() {
	new_test_ext().execute_with(|| {
		// Three authorities → AURA author for slot 100 is `100 % 3 = 1`.
		seed_aura_authorities(3);
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest_with_authority(100, 0)]);
		assert!(!ConsensusEngine::has_aura_pre_digest_before_babe_pre_digest());
	});
}

#[test]
fn babe_with_matching_authority_index_is_detected() {
	new_test_ext().execute_with(|| {
		seed_aura_authorities(3);
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest_with_authority(100, 1)]);
		assert!(ConsensusEngine::has_aura_pre_digest_before_babe_pre_digest());
	});
}

#[test]
fn babe_with_empty_aura_authorities_is_not_detected() {
	new_test_ext().execute_with(|| {
		pallet_aura::Authorities::<Test>::kill();
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest(100)]);
		assert!(!ConsensusEngine::has_aura_pre_digest_before_babe_pre_digest());
	});
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

// --- `schedule_flip` ---

#[test]
fn schedule_flip_from_aura() {
	new_test_ext().execute_with(|| {
		assert_ok!(ConsensusEngine::schedule_flip(RuntimeOrigin::root()));

		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);
		// A `ScheduledFlip` still authors with AURA until the flip commits.
		assert_eq!(ConsensusEngine::active_engine(), ActiveEngine::Aura);
	});
}

#[test]
fn schedule_flip_requires_governance_origin() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			ConsensusEngine::schedule_flip(RuntimeOrigin::signed(1)),
			DispatchError::BadOrigin
		);
		assert_noop!(
			ConsensusEngine::schedule_flip(RuntimeOrigin::none()),
			DispatchError::BadOrigin
		);
		assert_eq!(EngineState::<Test>::get(), State::Aura);
	});
}

#[test]
fn schedule_flip_is_rejected_unless_aura() {
	new_test_ext().execute_with(|| {
		for state in [State::ScheduledFlip, State::Babe] {
			put_engine_state(state);
			assert_noop!(
				ConsensusEngine::schedule_flip(RuntimeOrigin::root()),
				Error::<Test>::InvalidEngineState
			);
			assert_eq!(EngineState::<Test>::get(), state);
		}
	});
}

// --- `ScheduledFlip`: same digest rule, plus the flip at the epoch's last slot ---

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'ScheduledFlip'")]
fn scheduled_rejects_mismatched_babe_slot() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		// Mid-epoch: flip does not run, but a mismatched BABE digest is still rejected.
		start_block_with_logs(vec![aura_pre_digest(1400), babe_pre_digest(1401)]);
		on_initialize();
	});
}

#[test]
fn flip_fires_at_the_last_slot_of_the_epoch() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();

		// The last slot of the epoch (1499 for a 300-slot epoch) attempts the flip.
		// The flip block must carry a matching BABE pre-digest.
		start_block_with_babe_pre_digest(1499);
		on_initialize();

		assert_eq!(EngineState::<Test>::get(), State::Babe);
		assert_eq!(ConsensusEngine::active_engine(), ActiveEngine::Babe);
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(1500));
		// `CurrentSlot` is the flip block's own slot (the last pre-BABE slot), not the genesis slot,
		// so pallet-babe's `OnTimestampSet` slot check passes for this block.
		assert_eq!(pallet_babe::CurrentSlot::<Test>::get(), Slot::from(1499));
		assert_eq!(pallet_babe::EpochIndex::<Test>::get(), 0);
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'ScheduledFlip'")]
fn flip_rejects_epoch_end_block_without_babe_pre_digest() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();

		// An epoch-end AURA block without a BABE pre-digest must be rejected: committing
		// the flip without it would permanently halt authoring.
		start_block_at_slot(1499);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'ScheduledFlip'")]
fn scheduled_rejects_aura_only_blocks_even_while_authorities_empty() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		assert!(pallet_babe::Authorities::<Test>::get().is_empty());

		// The digest requirement holds on every scheduled block, including while
		// waiting for the session rotation.
		start_block_at_slot(1499);
		on_initialize();
	});
}

#[test]
fn flip_postpones_when_babe_authorities_empty() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		// Default Authorities is empty — postpone even when the digest is present.
		assert!(pallet_babe::Authorities::<Test>::get().is_empty());
		start_block_with_babe_pre_digest(1499);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::ScheduledFlip);

		// Once Authorities is populated, the next epoch-end flip proceeds
		seed_babe_authorities();
		start_block_with_babe_pre_digest(1799);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::Babe);
	});
}

#[test]
fn flip_does_not_run_mid_epoch() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
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
		put_engine_state(State::ScheduledFlip);
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
fn flip_fires_at_next_epoch_last_slot_when_the_last_slot_is_skipped() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();
		// The epoch's last slot (1499) was skipped; the flip waits and fires at the
		// next epoch's last slot (1799).
		start_block_with_babe_pre_digest(1799);
		on_initialize();

		assert_eq!(EngineState::<Test>::get(), State::Babe);
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(1800));
		assert_eq!(pallet_babe::CurrentSlot::<Test>::get(), Slot::from(1799));
	});
}

#[test]
fn migrate_to_babe_writes_epoch_config_when_absent() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();
		// Upgrade-path shape: Babe genesis never ran, so EpochConfig is unset.
		assert!(pallet_babe::EpochConfig::<Test>::get().is_none());

		start_block_with_babe_pre_digest(1499);
		on_initialize();

		assert_eq!(pallet_babe::EpochConfig::<Test>::get(), Some(TestBabeEpochConfig::get()));
	});
}

#[test]
fn migrate_to_babe_preserves_existing_epoch_config() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();

		let existing = sp_consensus_babe::BabeEpochConfiguration {
			c: (2, 5),
			allowed_slots: sp_consensus_babe::AllowedSlots::PrimarySlots,
		};
		pallet_babe::EpochConfig::<Test>::put(existing.clone());

		start_block_with_babe_pre_digest(1499);
		on_initialize();

		assert_eq!(pallet_babe::EpochConfig::<Test>::get(), Some(existing));
	});
}

#[test]
fn on_initialize_is_a_no_op_in_stable_states() {
	new_test_ext().execute_with(|| {
		// Even at an epoch's last slot, non-scheduled states never flip.
		// Slots must strictly increase across these blocks: full-runtime hooks
		// run pallet-aura, which rejects non-increasing slots.
		put_engine_state(State::Aura);
		start_block_with_babe_pre_digest(1499);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::Aura);

		// Post-flip blocks carry a BABE digest only (an AURA one is rejected).
		put_engine_state(State::Babe);
		start_block_with_logs(vec![babe_pre_digest(1500)]);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::Babe);
	});
}

// --- `Babe`: no AURA pre-digest anywhere ---

#[test]
#[should_panic(expected = "AURA pre-runtime digest present in state 'Babe'")]
fn babe_rejects_aura_pre_digest_before_babe() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::Babe);
		// The misattribution shape: `polkadot-js` extractAuthor takes the first
		// decodable pre-runtime digest, so a leading AURA digest would credit
		// `slot % n_authorities` instead of the real BABE author.
		start_block_with_logs(vec![aura_pre_digest(1500), babe_pre_digest(1500)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "AURA pre-runtime digest present in state 'Babe'")]
fn babe_rejects_aura_pre_digest_after_babe() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::Babe);
		// Position-independent: helpers that look AURA up first (e.g. the node's
		// parent-slot resolution) are fooled wherever the digest sits.
		start_block_with_logs(vec![babe_pre_digest(1500), aura_pre_digest(9999)]);
		on_initialize();
	});
}

#[test]
fn babe_accepts_babe_only_block_with_unrelated_digest() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::Babe);
		// What BABE actually authors: its own pre-digest plus partner-chains' own
		// digest item, which must not be mistaken for an AURA one.
		start_block_with_logs(vec![babe_pre_digest(1500), unrelated_pre_digest()]);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::Babe);
	});
}

// --- malformed payloads: presence is keyed on the engine id, not on decoding ---
//
// `as_babe_pre_digest`/`as_aura_pre_digest` decode with `DecodeAll`, so they return
// `None` for an undecodable payload *and* for a valid one with trailing bytes.
// `pallet-babe`/`pallet-aura` read the same items with plain `Decode`, which ignores
// trailing bytes, so a "malformed" item is still consumed on-chain. These guards must
// therefore reject such items rather than see straight through them.

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_undecodable_babe_pre_digest() {
	new_test_ext().execute_with(|| {
		start_block_with_logs(vec![aura_pre_digest(100), undecodable_babe_pre_digest()]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_babe_pre_digest_with_trailing_bytes() {
	new_test_ext().execute_with(|| {
		// pallet-babe decodes this and consumes it as the block's BABE pre-digest, so it
		// must not pass as "well-formed" here either.
		start_block_with_logs(vec![aura_pre_digest(100), babe_pre_digest_with_trailing_bytes(100)]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "AURA pre-runtime digest present in state 'Babe'")]
fn babe_rejects_aura_pre_digest_with_trailing_bytes() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::Babe);
		// pallet-aura's `Slot::decode` accepts this, and lenient off-chain decoders
		// (polkadot-js `extractAuthor`) would credit `slot % n_authorities`.
		start_block_with_logs(vec![
			babe_pre_digest(1500),
			aura_pre_digest_with_trailing_bytes(9999),
		]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_extra_babe_item_with_trailing_bytes() {
	new_test_ext().execute_with(|| {
		// A valid marker plus a second BABE item that `DecodeAll` cannot read: the
		// uniqueness rule must still catch it.
		start_block_with_logs(vec![
			aura_pre_digest(100),
			babe_pre_digest(100),
			babe_pre_digest_with_trailing_bytes(100),
		]);
		on_initialize();
	});
}

#[test]
#[should_panic(expected = "BABE pre-runtime digest required in state 'Aura'")]
fn aura_rejects_malformed_aura_pre_digest() {
	new_test_ext().execute_with(|| {
		// The AURA item is the one pallet-aura reads for the slot; a malformed one
		// must not pass as "no AURA digest" either.
		start_block_with_logs(vec![aura_pre_digest_with_trailing_bytes(100), babe_pre_digest(100)]);
		on_initialize();
	});
}

#[test]
fn malformed_items_are_not_detected_as_the_transition_shape() {
	// Helper level: each malformed arrangement fails the marker check.
	assert!(!aura_before_babe(vec![aura_pre_digest(100), undecodable_babe_pre_digest()]));
	assert!(!aura_before_babe(vec![
		aura_pre_digest(100),
		babe_pre_digest_with_trailing_bytes(100)
	]));
	assert!(!aura_before_babe(vec![
		aura_pre_digest_with_trailing_bytes(100),
		babe_pre_digest(100)
	]));
	// Duplicate AURA items are rejected too.
	assert!(!aura_before_babe(vec![
		aura_pre_digest(100),
		aura_pre_digest(100),
		babe_pre_digest(100)
	]));
}

#[test]
fn flip_leaves_babe_current_slot_matching_the_flip_block_timestamp() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();

		start_block_with_babe_pre_digest(1499);
		on_initialize();

		// `GenesisSlot` aligns BABE's epoch 0 with the next sidechain epoch...
		assert_eq!(pallet_babe::GenesisSlot::<Test>::get(), Slot::from(1500));
		// ...but `CurrentSlot` must stay this block's slot: the timestamp inherent runs
		// after `on_initialize` in the same block, and pallet-babe asserts
		// `CurrentSlot == timestamp_slot`.
		assert_eq!(pallet_babe::CurrentSlot::<Test>::get(), Slot::from(1499));

		// The engine is BABE by the time the timestamp inherent executes, so
		// pallet-babe's hook must accept the flip block rather than reject it.
		assert_eq!(EngineState::<Test>::get(), State::Babe);
		<ConsensusEngine as OnTimestampSet<u64>>::on_timestamp_set(1499 * SLOT_DURATION);
	});
}

#[test]
fn current_slot_reads_aura_storage_pre_flip() {
	new_test_ext().execute_with(|| {
		pallet_aura::CurrentSlot::<Test>::put(Slot::from(7));
		pallet_babe::CurrentSlot::<Test>::put(Slot::from(99));
		assert_eq!(ConsensusEngine::current_slot(), Slot::from(7));
	});
}

#[test]
fn current_slot_reads_babe_storage_post_flip() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::Babe);
		pallet_aura::CurrentSlot::<Test>::put(Slot::from(7));
		pallet_babe::CurrentSlot::<Test>::put(Slot::from(42));
		assert_eq!(ConsensusEngine::current_slot(), Slot::from(42));
	});
}

#[test]
fn current_slot_reads_aura_storage_while_flip_pending() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		pallet_aura::CurrentSlot::<Test>::put(Slot::from(7));
		pallet_babe::CurrentSlot::<Test>::put(Slot::from(99));
		assert_eq!(ConsensusEngine::current_slot(), Slot::from(7));
	});
}

// --- `pallet_session::ShouldEndSession`: one rotation per BABE epoch once on BABE ---

use pallet_session::ShouldEndSession as _;

/// What `pallet-babe::enact_epoch_change` leaves behind when it enacts a rotation in block `n`.
fn babe_enacts_rotation_in_block(n: u64) {
	pallet_babe::EpochStart::<Test>::mutate(|(previous, current)| {
		*previous = *current;
		*current = n;
	});
}

/// Enter `Babe` as `migrate_to_babe` leaves it: genesis slot 1500 (epoch 0 = slots 1500..1799).
fn enter_babe_with_genesis_slot_1500() {
	put_engine_state(State::ScheduledFlip);
	seed_babe_authorities();
	start_block_with_babe_pre_digest(1499);
	on_initialize();
	assert_eq!(EngineState::<Test>::get(), State::Babe);
	ConsensusEngine::on_finalize(System::block_number());
	finalize_babe_block();
}

/// Start a post-flip block at `slot` so `pallet_babe::CurrentSlot` is the one a rotation
/// decision would see.
fn start_babe_block(slot: u64) {
	// Close pallet-babe's previous block first, or its `initialize` short-circuits and leaves
	// `CurrentSlot` stale.
	finalize_babe_block();
	start_block_with_logs(vec![babe_pre_digest(slot)]);
	on_initialize();
}

#[test]
fn should_end_session_forwards_to_the_chain_rule_before_the_flip() {
	new_test_ext().execute_with(|| {
		for state in [State::Aura, State::ScheduledFlip] {
			put_engine_state(state);
			set_rotation_due(false);
			assert!(!ConsensusEngine::should_end_session(5));
			set_rotation_due(true);
			assert!(ConsensusEngine::should_end_session(5), "state {state:?}");
		}
	});
}

#[test]
fn should_end_session_is_false_when_the_chain_rule_says_so_on_babe() {
	new_test_ext().execute_with(|| {
		enter_babe_with_genesis_slot_1500();
		start_babe_block(1500);
		set_rotation_due(false);
		assert!(!ConsensusEngine::should_end_session(System::block_number()));
	});
}

#[test]
fn first_babe_block_may_rotate_even_though_rotations_were_recorded_before_the_flip() {
	new_test_ext().execute_with(|| {
		// A pre-flip rotation leaves a stale record; `migrate_to_babe` must clear it so the first
		// BABE block (which the BABE client requires to announce epoch 1) can rotate.
		LastRotationBabeEpoch::<Test>::put(0);
		enter_babe_with_genesis_slot_1500();
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), None);

		start_babe_block(1500);
		set_rotation_due(true);
		assert!(ConsensusEngine::should_end_session(System::block_number()));
	});
}

#[test]
fn rotation_enacted_in_the_flip_block_is_not_counted_against_babe_epoch_0() {
	new_test_ext().execute_with(|| {
		put_engine_state(State::ScheduledFlip);
		seed_babe_authorities();
		set_rotation_due(true);

		// The flip block is the first block of its epoch, so `pallet-session` (which runs after
		// this pallet) rotates in it; the flip itself is not postponed for that.
		start_block_with_babe_pre_digest(1499);
		on_initialize();
		assert_eq!(EngineState::<Test>::get(), State::Babe);
		let flip = System::block_number();
		babe_enacts_rotation_in_block(flip);
		ConsensusEngine::on_finalize(flip);
		finalize_babe_block();

		// Slot 1499 is below the genesis slot 1500 and would saturate to epoch 0; it must not
		// be recorded, or the first BABE block's rotation would be held back.
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), None);
		start_babe_block(1500);
		assert!(ConsensusEngine::should_end_session(System::block_number()));
	});
}

#[test]
fn second_rotation_in_the_same_babe_epoch_is_held_back_until_the_next_epoch() {
	new_test_ext().execute_with(|| {
		enter_babe_with_genesis_slot_1500();
		set_rotation_due(true);

		// First block of BABE epoch 0: the rotation goes through and pallet-babe enacts it.
		start_babe_block(1500);
		let n = System::block_number();
		assert!(ConsensusEngine::should_end_session(n));
		babe_enacts_rotation_in_block(n);
		ConsensusEngine::on_finalize(n);
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), Some(0));

		// Catch-up rotation two blocks later in the same epoch: held back, nothing recorded.
		start_babe_block(1502);
		let n = System::block_number();
		assert!(!ConsensusEngine::should_end_session(n));
		ConsensusEngine::on_finalize(n);
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), Some(0));

		// Last slot of epoch 0: still the same epoch, still held back.
		start_babe_block(1799);
		assert!(!ConsensusEngine::should_end_session(System::block_number()));

		// First block of epoch 1: allowed again.
		start_babe_block(1800);
		let n = System::block_number();
		assert!(ConsensusEngine::should_end_session(n));
		babe_enacts_rotation_in_block(n);
		ConsensusEngine::on_finalize(n);
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), Some(1));
	});
}

#[test]
fn skipped_babe_epochs_do_not_block_the_catch_up_rotation() {
	new_test_ext().execute_with(|| {
		enter_babe_with_genesis_slot_1500();
		set_rotation_due(true);
		start_babe_block(1500);
		let n = System::block_number();
		babe_enacts_rotation_in_block(n);
		ConsensusEngine::on_finalize(n);

		// Epochs 1 and 2 had no blocks; the first block of epoch 3 is the one the BABE client
		// expects to announce an epoch change, so the rotation must be allowed.
		start_babe_block(2400);
		assert!(ConsensusEngine::should_end_session(System::block_number()));
	});
}

#[test]
fn on_finalize_records_nothing_without_an_enacted_rotation_or_outside_babe() {
	new_test_ext().execute_with(|| {
		// Pre-flip: pallet-babe is a session handler already and stamps `EpochStart`, but that
		// is not a BABE-epoch rotation.
		start_block_with_babe_pre_digest(100);
		on_initialize();
		babe_enacts_rotation_in_block(System::block_number());
		ConsensusEngine::on_finalize(System::block_number());
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), None);

		enter_babe_with_genesis_slot_1500();
		// A BABE block in which pallet-babe enacted nothing leaves the record alone.
		start_babe_block(1500);
		ConsensusEngine::on_finalize(System::block_number());
		assert_eq!(LastRotationBabeEpoch::<Test>::get(), None);
	});
}

#[test]
fn current_babe_epoch_follows_pallet_babe_arithmetic() {
	new_test_ext().execute_with(|| {
		enter_babe_with_genesis_slot_1500();
		for (slot, epoch) in [(1500, 0), (1799, 0), (1800, 1), (2400, 3)] {
			pallet_babe::CurrentSlot::<Test>::put(Slot::from(slot));
			assert_eq!(ConsensusEngine::current_babe_epoch(), epoch, "slot {slot}");
		}
	});
}

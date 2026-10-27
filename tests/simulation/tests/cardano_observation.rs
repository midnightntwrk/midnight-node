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

//! Model-based properties of the Cardano→Midnight observation path under a simulated
//! Cardano with delays, reordering and rollbacks.

use midnight_ledger_unsafe_helpers::{
	CNightGeneratesDustActionType, CNightGeneratesDustEvent, HashOutput, InitialNonce, Timestamp,
	deserialize_untagged,
};
use midnight_simulation::{
	ObservedState, SLOT_DURATION_MS, SimNode,
	cardano::{
		CardanoEvent, CardanoSim, MAX_WALLETS, Observation, RawEvent, Wallet, arb_history,
		arb_release, bind_history, follow,
	},
	model::{Action, ExpectedDustEvent, ObservationModel, Verdict},
};
use pallet_cnight_observation::Error;
use pallet_cnight_observation_mock::mock::Test;
use proptest::prelude::*;
use sp_runtime::DispatchError;

fn wallets() -> Vec<Wallet> {
	(0..MAX_WALLETS).map(Wallet::deterministic).collect()
}

fn expected_event(e: &ExpectedDustEvent) -> CNightGeneratesDustEvent {
	CNightGeneratesDustEvent {
		value: e.value,
		owner: deserialize_untagged(&e.owner.0[..]).expect("dust pk"),
		time: Timestamp::from_secs(e.time_secs),
		action: match e.action {
			Action::Create => CNightGeneratesDustActionType::Create,
			Action::Destroy => CNightGeneratesDustActionType::Destroy,
		},
		nonce: InitialNonce(HashOutput(e.nonce)),
	}
}

fn regression_error() -> DispatchError {
	Error::<Test>::CardanoPositionRegression.into()
}

/// Drives `node` and `model` through `stream`, asserting agreement after every block.
fn run_against_model(node: &mut SimNode, model: &mut ObservationModel, stream: &[Observation]) {
	for obs in stream {
		let before = node.state();
		let verdict = model.observe(&obs.utxos, &obs.next);
		let outcome = node.observe(obs.utxos.clone(), obs.next.clone());
		assert!(outcome.check_inherent_ok, "verifier must accept the proposer's own inherent");
		match verdict {
			Verdict::Applied(expected) => {
				outcome
					.dispatch
					.unwrap_or_else(|e| panic!("model applied, runtime rejected: {e:?}"));
				let expected: Vec<_> = expected.iter().map(expected_event).collect();
				assert_eq!(outcome.dust_events, expected, "ledger dust events");
				let after = node.state();
				assert_eq!(after.mapping, model.mapping, "registration mapping");
				assert_eq!(Some(&after.next_position), model.next_position.as_ref());
				assert_eq!(node.utxo_owner_count(), model.generating.len(), "generating UTXOs");
				assert_eq!(
					after.ledger_state_root != before.ledger_state_root,
					!expected.is_empty(),
					"ledger state changes exactly when a dust system tx is applied"
				);
			},
			Verdict::Regression => {
				let err = outcome.dispatch.expect_err("model rejected, runtime applied");
				assert_eq!(err.error, regression_error());
				assert!(outcome.dust_events.is_empty());
				assert_eq!(node.state(), before, "rejected inherent must not touch state");
			},
		}
		node.advance_block(SLOT_DURATION_MS);
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 24, .. ProptestConfig::default() })]

	/// Rules 1–3 and 5: for any consistent Cardano history and any (possibly stalled)
	/// release schedule, the runtime's mapping, position, generating set and emitted
	/// ledger events equal the model's.
	#[test]
	fn runtime_agrees_with_model(sim in arb_history(8, 5), release in arb_release(10)) {
		let wallets = wallets();
		let stream = follow(&sim, &wallets, &release);
		let mut node = SimNode::new();
		let mut model = ObservationModel::default();
		run_against_model(&mut node, &mut model, &stream);
	}

	/// Rule 4 under a leaked rollback: a follower that switches to a fork without honouring
	/// a stability depth reports a position the runtime must reject whenever the model
	/// says it regresses, leaving state (including the ledger root) untouched; forks the
	/// model accepts are applied identically by both.
	#[test]
	fn leaked_rollback_is_rejected_iff_position_regresses(
		sim in arb_history(6, 4),
		depth in 1usize..=3,
		replacement in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..=6), 1..=3),
	) {
		let wallets = wallets();
		let n = sim.blocks.len();
		let stream = follow(&sim, &wallets, &vec![1u8; n]);
		let mut node = SimNode::new();
		let mut model = ObservationModel::default();
		run_against_model(&mut node, &mut model, &stream);

		let replacement = replacement
			.into_iter()
			.map(|raw| raw.into_iter().map(|b| match b % 3 {
				0 => CardanoEvent::Register { wallet: b % MAX_WALLETS },
				1 => CardanoEvent::Create { asset: b, owner: b % MAX_WALLETS, value: 1 + b as u128 },
				_ => CardanoEvent::Register { wallet: (b / 3) % MAX_WALLETS },
			}).collect())
			.collect();
		let fork = sim.fork(depth.min(n), replacement);
		let keep = n - depth.min(n);
		let leaked: Vec<Observation> = fork.blocks[keep..]
			.iter()
			.map(|b| Observation { utxos: fork.observed(b, &wallets), next: fork.end_position(b.number) })
			.collect();
		run_against_model(&mut node, &mut model, &leaked);
	}

	/// A follower honouring a stability depth `k` larger than any rollback produces the
	/// same observations, hence the same runtime state, with or without the rollback.
	#[test]
	fn stable_follower_hides_shallow_rollbacks(
		sim in arb_history(8, 4),
		depth in 1usize..=2,
		k in 2u8..=4,
	) {
		prop_assume!(sim.blocks.len() > depth);
		let wallets = wallets();
		let replacement = vec![vec![CardanoEvent::Register { wallet: 0 }]; depth];
		let fork = sim.fork(depth, replacement);
		let stable = |s: &CardanoSim| -> Vec<Observation> {
			let visible = s.blocks.len().saturating_sub(k as usize);
			follow(s, &wallets, &vec![1u8; visible])
		};
		let without_rollback = run_trace(&stable(&sim));
		let with_rollback = run_trace(&stable(&fork));
		prop_assert_eq!(without_rollback, with_rollback);
	}

	/// Rule 5: two nodes fed the same stream produce identical traces (events and state
	/// after every block); a verifier given a reordered or re-positioned inherent rejects it
	/// (byte-exact agreement).
	#[test]
	fn determinism_and_byte_exact_inherents(sim in arb_history(6, 5), release in arb_release(8)) {
		let wallets = wallets();
		let stream = follow(&sim, &wallets, &release);
		prop_assert_eq!(run_trace(&stream), run_trace(&stream));

		let mut node = SimNode::new();
		for obs in &stream {
			if obs.utxos.len() >= 2 && obs.utxos[0] != obs.utxos[obs.utxos.len() - 1] {
				let mut reordered = obs.utxos.clone();
				reordered.reverse();
				prop_assert!(!node.check_inherent(reordered, obs.next.clone(), obs.utxos.clone(), obs.next.clone()));
			}
			let mut moved = obs.next.clone();
			moved.tx_index_in_block += 1;
			prop_assert!(!node.check_inherent(obs.utxos.clone(), moved, obs.utxos.clone(), obs.next.clone()));
			node.observe(obs.utxos.clone(), obs.next.clone()).dispatch.unwrap();
			node.advance_block(SLOT_DURATION_MS);
		}
	}
}

/// Per-block (dust events, state) trace of a fresh node fed `stream`. Nodes share one
/// process-global ledger arena, so runs are sequential: the node is dropped on return.
fn run_trace(stream: &[Observation]) -> Vec<(Vec<CNightGeneratesDustEvent>, ObservedState)> {
	let mut node = SimNode::new();
	stream
		.iter()
		.map(|obs| {
			let outcome = node.observe(obs.utxos.clone(), obs.next.clone());
			outcome.dispatch.unwrap();
			let state = node.state();
			node.advance_block(SLOT_DURATION_MS);
			(outcome.dust_events, state)
		})
		.collect()
}

#[test]
fn second_inherent_in_a_block_is_rejected() {
	let sim = bind_history(vec![vec![]]);
	let mut node = SimNode::new();
	node.observe(vec![], sim.end_position(1)).dispatch.unwrap();
	let err = node.observe(vec![], sim.end_position(1)).dispatch.unwrap_err();
	assert_eq!(err.error, Error::<Test>::InherentAlreadyExecuted.into());
}

#[test]
fn over_capacity_inherent_is_rejected() {
	let sim = bind_history(vec![vec![RawEvent::Register(0)]]);
	let utxo = sim.observed(&sim.blocks[0], &wallets()).remove(0);
	let mut node = SimNode::new();
	let too_many = vec![utxo; pallet_cnight_observation::MAX_UTXO_COUNT as usize + 1];
	let err = node.observe(too_many, sim.end_position(1)).dispatch.unwrap_err();
	assert_eq!(err.error, Error::<Test>::TooManyUtxos.into());
}

/// Documents a gap the model makes explicit: position monotonicity is checked on
/// (block number, tx index) only, so a same-height fork block with at least as many
/// transactions is accepted even though its hash differs. Safety therefore rests on the
/// follower's stability depth, not on the runtime.
#[test]
fn same_height_fork_is_not_detected_by_position_check() {
	let wallets = wallets();
	let sim = bind_history(vec![vec![RawEvent::Register(0)]]);
	let mut node = SimNode::new();
	node.observe(sim.observed(&sim.blocks[0], &wallets), sim.end_position(1))
		.dispatch
		.unwrap();
	node.advance_block(SLOT_DURATION_MS);
	let fork = sim.fork(1, vec![vec![CardanoEvent::Register { wallet: 1 }]]);
	assert_ne!(fork.block_hash(1), sim.block_hash(1));
	let outcome = node.observe(fork.observed(&fork.blocks[0], &wallets), fork.end_position(1));
	assert!(outcome.dispatch.is_ok(), "same-height fork block accepted by position check alone");
	assert_eq!(node.state().mapping.len(), 2, "effects of the rolled-back block persist");
}

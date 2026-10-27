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

//! Committee rotation over many sidechain epochs with the mainchain selection data arriving
//! late at some epoch boundaries, as it does on a node whenever the sidechain epoch starts
//! before the mainchain epoch it depends on is stable.
//!
//! Spec (also stated in the pallet's own `check_inherent`): when no new committee can be
//! computed, the block keeps the committee it has. The model here is that rule plus the pallet's
//! two-stage pipeline: a committee set during epoch `e` is queued at the rotation into `e + 1`
//! and serves from `e + 2`; empty selection data re-sets the queued committee; and a boundary
//! slot whose data is not there yet produces a block without changing anything.

use midnight_simulation::committee::{Authoring, AuthorityId, SimCommittee, Validator};
use proptest::prelude::*;

const CANDIDATES: [Validator; 5] = [(21, 11), (22, 12), (23, 13), (24, 14), (25, 15)];
const GENESIS: [Validator; 2] = [(21, 11), (22, 12)];

/// One epoch as the node lives it: the candidates its selection data names, and whether that
/// data is already stable when the epoch's first block is authored.
#[derive(Clone, Debug)]
struct Epoch {
	candidates: Vec<Validator>,
	stable_at_boundary: bool,
}

fn epoch_strategy() -> impl Strategy<Value = Epoch> {
	(prop::collection::vec(0..CANDIDATES.len(), 0..=5), any::<bool>()).prop_map(
		|(picks, stable_at_boundary)| {
			let mut candidates: Vec<Validator> = Vec::new();
			for i in picks {
				if !candidates.contains(&CANDIDATES[i]) {
					candidates.push(CANDIDATES[i]);
				}
			}
			Epoch { candidates, stable_at_boundary }
		},
	)
}

fn schedule(max_epochs: usize) -> impl Strategy<Value = Vec<Epoch>> {
	prop::collection::vec(epoch_strategy(), 1..=max_epochs)
}

fn ids(validators: &[Validator]) -> Vec<AuthorityId> {
	validators.iter().map(|(id, _)| *id).collect()
}

/// The committee that should serve each epoch `0..=schedule.len() + 1`: the genesis committee
/// serves epochs 0 and 1, and the data of epoch `e` decides epoch `e + 2`.
fn model(schedule: &[Epoch]) -> Vec<Vec<AuthorityId>> {
	let mut committees = vec![ids(&GENESIS), ids(&GENESIS)];
	for (e, epoch) in schedule.iter().enumerate() {
		let queued = committees[e + 1].clone();
		committees.push(if epoch.candidates.is_empty() { queued } else { ids(&epoch.candidates) });
	}
	committees
}

/// Drives one epoch: the boundary block with or without data, then a second block where the
/// data has become stable. Returns the boundary block's authoring outcome.
fn run_epoch(sim: &mut SimCommittee, epoch: &Epoch) -> Authoring {
	let boundary =
		sim.author_block(epoch.stable_at_boundary.then_some(epoch.candidates.as_slice()));
	sim.advance_block();
	let late = sim.author_block(Some(&epoch.candidates));
	assert!(
		!matches!(late, Authoring::Panicked(_)),
		"authoring with stable data panicked: {late:?}"
	);
	boundary
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

	/// With the data stable at every boundary the committee follows the selection data
	/// exactly, one `set` per epoch, and nothing else is ever authored.
	#[test]
	fn committee_follows_the_selection_data(schedule in schedule(300)) {
		let mut sim = SimCommittee::new(&GENESIS);
		let expected = model(&schedule);
		for (e, epoch) in schedule.iter().enumerate() {
			prop_assert_eq!(sim.current_committee(), expected[e].clone(), "committee serving epoch {}", e);
			let stable = Epoch { stable_at_boundary: true, ..epoch.clone() };
			prop_assert_eq!(run_epoch(&mut sim, &stable), Authoring::Set(expected[e + 2].clone()));
			prop_assert!(sim.next_committee_stored());
			sim.advance_epoch();
		}
		prop_assert_eq!(sim.current_committee(), expected[schedule.len()].clone());
	}

	/// Spec: a boundary slot whose selection data is not yet stable keeps the committee and
	/// still produces a block. Ignored while the pallet panics instead (see the test below).
	#[test]
	#[ignore = "pallet-session-validator-management::create_inherent panics when the Ariadne inherent data is absent; every epoch boundary on local-env loses its first slot to this"]
	fn boundary_without_stable_data_keeps_the_committee(schedule in schedule(300)) {
		let mut sim = SimCommittee::new(&GENESIS);
		let expected = model(&schedule);
		for (e, epoch) in schedule.iter().enumerate() {
			let boundary = run_epoch(&mut sim, epoch);
			prop_assert!(!matches!(boundary, Authoring::Panicked(_)), "epoch {}: {:?}", e, boundary);
			if !epoch.stable_at_boundary {
				prop_assert!(
					matches!(&boundary, Authoring::NoInherent) || boundary == Authoring::Set(expected[e + 1].clone()),
					"epoch {}: boundary block without data changed the committee: {:?}", e, boundary
				);
			}
			sim.advance_epoch();
			prop_assert_eq!(sim.current_committee(), expected[e + 1].clone(), "committee serving epoch {}", e + 1);
		}
	}

	/// What happens today, kept as the fast-tier reproduction of the lost slot seen live: the
	/// first slot after every rotation whose data is late panics in `create_inherent`, and the
	/// next slot recovers. Delete this test when the one above is un-ignored.
	#[test]
	fn boundary_without_stable_data_is_lost_today(schedule in schedule(300)) {
		let mut sim = SimCommittee::new(&GENESIS);
		let expected = model(&schedule);
		for (e, epoch) in schedule.iter().enumerate() {
			let boundary = run_epoch(&mut sim, epoch);
			if epoch.stable_at_boundary {
				prop_assert_eq!(boundary, Authoring::Set(expected[e + 2].clone()));
			} else {
				prop_assert!(
					matches!(&boundary, Authoring::Panicked(msg) if msg.contains("Validator inherent data must be provided")),
					"epoch {}: expected the known panic, got {:?}", e, boundary
				);
			}
			sim.advance_epoch();
			prop_assert_eq!(sim.current_committee(), expected[e + 1].clone(), "committee serving epoch {}", e + 1);
		}
	}

	/// Two chains fed the same schedule serve identical committees in every epoch.
	#[test]
	fn rotation_is_deterministic(schedule in schedule(100)) {
		let mut a = SimCommittee::new(&GENESIS);
		let mut b = SimCommittee::new(&GENESIS);
		for epoch in &schedule {
			let (oa, ob) = (run_epoch(&mut a, epoch), run_epoch(&mut b, epoch));
			prop_assert_eq!(oa, ob);
			a.advance_epoch();
			b.advance_epoch();
			prop_assert_eq!(a.current_committee(), b.current_committee());
		}
	}
}

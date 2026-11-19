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

use midnight_primitives_candidate_registrations::{
	CardanoTxPosition, CrchHash, MAX_PREVIOUS_REGISTRATIONS, RegistrationEntry,
	RegistrationHistory, RegistrationRecord, SNAPSHOT_EPOCH_OFFSET,
};
use sidechain_domain::{McEpochNumber, UtxoId};

/// A state told apart by `tag`, left by an event of Cardano epoch `epoch`.
fn entry(epoch: u32, tag: u32) -> RegistrationEntry<()> {
	RegistrationEntry {
		mc_epoch: McEpochNumber(epoch),
		record: RegistrationRecord::Inactive {
			crch_hash: CrchHash([0; 32]),
			observed_at: CardanoTxPosition::new(tag, 0),
			utxo: UtxoId::default(),
		},
	}
}

fn tag(record: Option<&RegistrationRecord<()>>) -> Option<u32> {
	record.map(|record| record.observed_at().block_no)
}

fn epochs(history: &RegistrationHistory<()>) -> Vec<u32> {
	core::iter::once(&history.current)
		.chain(history.previous.iter())
		.map(|entry| entry.mc_epoch.0)
		.collect()
}

#[test]
fn a_later_state_of_the_same_epoch_replaces_the_current_one() {
	let mut history = RegistrationHistory::new(entry(10, 1));
	history.record(entry(10, 2));

	assert_eq!(tag(Some(history.current())), Some(2));
	assert!(history.previous.is_empty());
}

#[test]
fn the_state_at_the_start_of_an_epoch_is_the_last_one_of_an_earlier_epoch() {
	let mut history = RegistrationHistory::new(entry(10, 1));
	history.record(entry(11, 2));
	history.record(entry(11, 3));
	history.record(entry(13, 4));

	assert_eq!(tag(history.at_start_of(McEpochNumber(10))), None);
	assert_eq!(tag(history.at_start_of(McEpochNumber(11))), Some(1));
	assert_eq!(tag(history.at_start_of(McEpochNumber(12))), Some(3));
	assert_eq!(tag(history.at_start_of(McEpochNumber(13))), Some(3));
	assert_eq!(tag(history.at_start_of(McEpochNumber(14))), Some(4));
}

#[test]
fn states_no_snapshot_can_read_are_dropped() {
	let mut history = RegistrationHistory::new(entry(10, 1));
	history.record(entry(11, 2));
	history.record(entry(20, 3));

	// The oldest snapshot still read is the start of epoch 17: epoch 11's state is the
	// one standing then, and epoch 10's is no longer needed.
	assert_eq!(epochs(&history), vec![20, 11]);
	assert_eq!(tag(history.at_start_of(McEpochNumber(20 - SNAPSHOT_EPOCH_OFFSET))), Some(2));
}

#[test]
fn a_pool_changing_every_epoch_keeps_every_snapshot_still_read() {
	let mut history = RegistrationHistory::new(entry(1, 1));
	for epoch in 2..=12 {
		history.record(entry(epoch, epoch));
	}

	assert_eq!(history.previous.len() as u32, MAX_PREVIOUS_REGISTRATIONS);
	assert_eq!(epochs(&history), vec![12, 11, 10, 9, 8]);
	for snapshot in (12 - SNAPSHOT_EPOCH_OFFSET)..=13 {
		assert_eq!(tag(history.at_start_of(McEpochNumber(snapshot))), Some(snapshot - 1));
	}
}

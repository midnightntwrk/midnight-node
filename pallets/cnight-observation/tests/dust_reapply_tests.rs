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

//! v1 -> v2 dust generation replay tests, against a **real** ledger.
//!
//! The mock wires the real `pallet-midnight`/`pallet-midnight-system` over a
//! parity-db arena, so the replay's system transactions genuinely apply to a
//! ledger-9 state, and the pre-fork values genuinely come out of a ledger-8 one
//! seeded in the same arena (v8 and v9 share the storage backend).

use frame_support::{migrations::SteppedMigration, pallet_prelude::*, weights::WeightMeter};
use midnight_node_ledger_helpers::{
	CNightGeneratesDustActionType, DustPublicKey, SystemTransaction, deserialize,
	serialize_untagged,
};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives_cnight_observation::DustPublicKeyBytes;
use pallet_cnight_observation::{
	DustReapplyCtime, DustReapplyProgress, Event, Pallet, PreForkStateKey, UtxoOwners,
	migrations::v2::{MAX_REAPPLY_BATCH, MigrateV1ToV2},
};
use pallet_cnight_observation_mock::mock::{
	self, CNightObservation, RuntimeEvent, System, Test, new_test_ext,
};
use sp_core::H256;
use test_log::test;

/// v8-side ledger types, reached through the helpers crate's per-generation
/// re-exports (the same crates the node's ledger-8 module is built from).
mod v8 {
	pub use midnight_node_ledger_helpers::ledger_8::{
		base_crypto::{hash::HashOutput, time::Timestamp},
		ledger_storage::{db::ParityDb, storage::default_storage},
		midnight_serialize::tagged_serialize,
		mn_ledger::{
			dust::{DustPublicKey, InitialNonce},
			structure::{
				CNightGeneratesDustActionType, CNightGeneratesDustEvent, LedgerState,
				SystemTransaction,
			},
		},
		transient_crypto::curve::Fr,
	};
}

/// The fork block's time.
const FORK_TIME_SECS: u64 = 1_800_000_000;

/// The `ctime` every replayed entry must carry: the fork block backdated by the
/// dust `time_to_cap`, so each restored entry is at its DUST cap on arrival.
/// Derived from the *active* (v9) parameters — the 8 -> 9 translation recasts
/// `parameters.dust` unchanged, so this is an independent check of the value the
/// migration reads out of the v8 state.
fn expected_ctime_secs() -> u64 {
	FORK_TIME_SECS
		- midnight_node_ledger_helpers::INITIAL_PARAMETERS.dust.time_to_cap().as_seconds() as u64
}

fn init_ledger_state() {
	let path_buf = tempfile::tempdir().unwrap().keep();
	let state_key = midnight_node_ledger::latest::storage::init_storage_paritydb_separate(
		&path_buf,
		UndeployedNetwork.genesis_state(),
		1024 * 1024,
	);

	mock::Midnight::initialize_state(UndeployedNetwork.id(), &state_key);
	mock::System::set_block_number(1);
	mock::Timestamp::set_timestamp(FORK_TIME_SECS * 1000);
	StorageVersion::new(1).put::<CNightObservation>();
}

fn nonce(byte: u8) -> H256 {
	H256([byte; 32])
}

fn owner() -> v8::DustPublicKey {
	v8::DustPublicKey(v8::Fr::from(7u64))
}

fn owner_bytes() -> DustPublicKeyBytes {
	DustPublicKeyBytes(serialize_untagged(&owner()).unwrap().try_into().unwrap())
}

/// Build a ledger-8 state whose dust generating set holds `entries`, persist it
/// into the (shared) arena and return its root — exactly what
/// `RecordPreForkState` would have saved during the hardfork upgrade block.
fn seed_pre_fork_state(entries: &[(H256, u128)]) -> Vec<u8> {
	let events = entries
		.iter()
		.map(|(nonce, value)| v8::CNightGeneratesDustEvent {
			value: *value,
			owner: owner(),
			time: v8::Timestamp::from_secs(FORK_TIME_SECS - 1_000),
			action: v8::CNightGeneratesDustActionType::Create,
			nonce: v8::InitialNonce(v8::HashOutput(nonce.0)),
		})
		.collect();

	let (state, _) = v8::LedgerState::<v8::ParityDb>::new(UndeployedNetwork.id())
		.apply_system_tx(
			&v8::SystemTransaction::CNightGeneratesDustUpdate { events },
			v8::Timestamp::from_secs(FORK_TIME_SECS - 1_000),
		)
		.expect("seed v8 dust generation entries");

	let mut sp = v8::default_storage::<v8::ParityDb>()
		.arena
		.alloc(midnight_node_ledger::ledger_8::api::ledger::Ledger::new(state));
	sp.persist();
	let mut root = Vec::new();
	v8::tagged_serialize(&sp.as_typed_key(), &mut root).expect("serialize v8 root");
	root
}

/// Drive the replay to completion, returning the number of steps taken.
fn run_to_completion() -> u32 {
	let mut cursor = None;
	let mut steps = 0;
	loop {
		let mut meter = WeightMeter::new();
		cursor = MigrateV1ToV2::<Test>::step(cursor, &mut meter).expect("step must not fail");
		steps += 1;
		if cursor.is_none() {
			return steps;
		}
	}
}

/// The `CNightGeneratesDustEvent`s of every system transaction applied so far.
fn applied_dust_events() -> Vec<midnight_node_ledger_helpers::CNightGeneratesDustEvent> {
	System::events()
		.iter()
		.filter_map(|record| match &record.event {
			RuntimeEvent::MidnightSystem(
				pallet_midnight_system::Event::SystemTransactionApplied(applied),
			) => Some(applied.serialized_system_transaction.clone()),
			_ => None,
		})
		.flat_map(|tx| {
			let SystemTransaction::CNightGeneratesDustUpdate { events } =
				deserialize(&tx[..]).expect("deserialize replay system tx")
			else {
				panic!("replay must apply a CNightGeneratesDustUpdate");
			};
			events
		})
		.collect()
}

fn cnight_events() -> Vec<Event<Test>> {
	System::events()
		.iter()
		.filter_map(|record| match &record.event {
			RuntimeEvent::CNightObservation(e) => Some(e.clone()),
			_ => None,
		})
		.collect()
}

/// The happy path: every live `UtxoOwners` nonce is restored with the night value
/// and dust owner the pre-fork ledger held for it, stamped with a `ctime` that
/// puts it at its DUST cap. A nonce the pre-fork state doesn't know is tallied as
/// skipped.
#[test]
fn replays_live_entries_from_pre_fork_state() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		let entries = [(nonce(1), 100u128), (nonce(2), 250u128), (nonce(3), 7u128)];
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}
		// Live in the pallet but absent from the pre-fork ledger state.
		UtxoOwners::<Test>::insert(nonce(9), owner_bytes());

		assert_eq!(run_to_completion(), 2, "one batch, then the completing step");

		assert_eq!(cnight_events(), vec![Event::DustReapplyCompleted { applied: 3, skipped: 1 }],);
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
		assert!(PreForkStateKey::<Test>::get().is_none());
		assert!(DustReapplyCtime::<Test>::get().is_none());
		assert_eq!(DustReapplyProgress::<Test>::get(), (0, 0));
		assert_eq!(
			UtxoOwners::<Test>::iter().count(),
			4,
			"UtxoOwners is the live set, not a queue — it must survive the replay",
		);

		// The applied events must be field-for-field the wiped ones, bar `ctime`.
		let expected_owner: DustPublicKey =
			midnight_node_ledger_helpers::deserialize_untagged(&mut &owner_bytes().0[..]).unwrap();
		let mut applied: Vec<(u128, [u8; 32])> = applied_dust_events()
			.iter()
			.map(|event| {
				assert_eq!(event.action, CNightGeneratesDustActionType::Create);
				assert_eq!(event.owner, expected_owner, "restored owner must be the ledger's");
				assert_eq!(
					event.time.to_secs(),
					expected_ctime_secs(),
					"replayed ctime must be the fork block backdated by time_to_cap",
				);
				(event.value, event.nonce.0.0)
			})
			.collect();
		applied.sort();
		let mut expected: Vec<(u128, [u8; 32])> =
			entries.iter().map(|(nonce, value)| (*value, nonce.0)).collect();
		expected.sort();
		assert_eq!(applied, expected);
	});
}

/// The inert-today path, for real: re-applying entries that are still present
/// fails with `GenerationInfoAlreadyPresent` on the first batch, which is how the
/// replay detects that the hardfork did not wipe dust after all. Driven by
/// replaying twice — the second run's ledger state already holds the entries.
#[test]
fn first_batch_failure_self_cancels() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		let entries = [(nonce(1), 100u128), (nonce(2), 250u128)];
		let pre_fork_key = seed_pre_fork_state(&entries);
		PreForkStateKey::<Test>::put(pre_fork_key.clone());
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}
		run_to_completion();

		// Now the current (v9) state holds them, as it would if the hardfork had
		// carried dust across instead of wiping it.
		frame_system::Pallet::<Test>::reset_events();
		StorageVersion::new(1).put::<CNightObservation>();
		PreForkStateKey::<Test>::put(pre_fork_key);

		assert_eq!(run_to_completion(), 1, "the failing first batch must end the replay");

		assert_eq!(cnight_events(), vec![Event::DustReapplySkipped]);
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
		assert!(PreForkStateKey::<Test>::get().is_none());
		assert!(applied_dust_events().is_empty(), "nothing must have been applied");
	});
}

/// More rows than one batch: the cursor hands off between steps and every row is
/// visited exactly once (the tallies sum to the row count).
#[test]
fn pages_across_steps_visiting_every_row_once() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		// Only a couple of rows resolve against the pre-fork state; the rest are
		// tallied as skipped. Keeps the seeded v8 state small while still
		// spanning three pages of `UtxoOwners`.
		let rows = MAX_REAPPLY_BATCH * 2 + 5;
		let entries = [(nonce(1), 100u128), (nonce(2), 250u128)];
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for i in 0..rows {
			UtxoOwners::<Test>::insert(H256::from_low_u64_be(i as u64 + 1), owner_bytes());
		}
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}
		let total = UtxoOwners::<Test>::iter().count() as u32;

		assert_eq!(run_to_completion(), 4, "three pages plus the completing step");

		let Some(Event::DustReapplyCompleted { applied, skipped }) = cnight_events().pop() else {
			panic!("replay must complete, got {:?}", cnight_events());
		};
		assert_eq!(applied, 2);
		assert_eq!(applied + skipped, total, "every row must be visited exactly once");
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
	});
}

/// A batch that fails *after* something has already been restored is a genuine
/// batch failure, not the "dust survived the hardfork" signal: report its nonces
/// and carry on to the next page.
#[test]
fn later_batch_failure_is_reported_and_the_replay_completes() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		let pre_fork_key = seed_pre_fork_state(&[(nonce(1), 100u128)]);
		PreForkStateKey::<Test>::put(pre_fork_key.clone());
		UtxoOwners::<Test>::insert(nonce(1), owner_bytes());
		run_to_completion();

		// Replay the same nonce again — it is now present in the ledger, so its
		// batch fails — but against progress that says an earlier page landed.
		frame_system::Pallet::<Test>::reset_events();
		StorageVersion::new(1).put::<CNightObservation>();
		PreForkStateKey::<Test>::put(pre_fork_key);
		DustReapplyProgress::<Test>::put((5, 0));

		assert_eq!(run_to_completion(), 2, "the replay must carry on past a failed batch");

		assert_eq!(
			cnight_events(),
			vec![
				Event::DustReapplyBatchFailed { nonces: vec![nonce(1)] },
				Event::DustReapplyCompleted { applied: 5, skipped: 1 },
			],
		);
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
	});
}

/// A `PreForkStateKey` that isn't a ledger-8 root (here: the current v9 root)
/// must abandon the replay rather than silently restore nothing.
#[test]
fn unreadable_pre_fork_key_cancels() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		PreForkStateKey::<Test>::put(mock::Midnight::state_key());
		UtxoOwners::<Test>::insert(nonce(1), owner_bytes());

		assert_eq!(run_to_completion(), 1);

		assert_eq!(cnight_events(), vec![Event::DustReapplySkipped]);
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
		assert!(applied_dust_events().is_empty());
	});
}

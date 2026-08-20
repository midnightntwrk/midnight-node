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

use frame_support::{
	migrations::SteppedMigration, pallet_prelude::*, traits::Hooks, weights::WeightMeter,
};
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

/// The mock's block weight limit, which the ledger's own cost model is normalized
/// against (`get_transaction_cost`'s `max_weight`).
fn max_block() -> u64 {
	let block_weights: frame_system::limits::BlockWeights =
		<Test as frame_system::Config>::BlockWeights::get();
	block_weights.max_block.ref_time()
}

/// A budget with room for exactly one batch — see [`run_to_completion`].
fn one_batch_budget() -> Weight {
	Weight::from_parts(max_block() / 100 * 15, u64::MAX)
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

/// Drive the replay to completion, roughly a page per step, returning the number of
/// steps taken.
///
/// A meter with room for one full batch is what paces the steps: a 25-nonce batch
/// prices at ~11% of the mock's block, so 15% takes one and turns the next away, while
/// leaving the batch itself well inside the 90%-of-limit ceiling above which the replay
/// gives up entirely. A *short* page costs proportionally less and can still share a
/// step. `WeightMeter::new()` would drain the whole replay in one step and make the
/// step counts below meaningless.
///
/// `Midnight::on_finalize` runs between steps because it is the only place the
/// ledger's `block_fullness` resets; each step is a block.
fn run_to_completion() -> u32 {
	run_from(None)
}

/// [`run_to_completion`], resuming from an in-flight cursor.
fn run_from(mut cursor: Option<<MigrateV1ToV2<Test> as SteppedMigration>::Cursor>) -> u32 {
	let mut steps = 0;
	loop {
		let mut meter = WeightMeter::with_limit(one_batch_budget());
		cursor = MigrateV1ToV2::<Test>::step(cursor, &mut meter).expect("step must not fail");
		steps += 1;
		if cursor.is_none() {
			return steps;
		}
		<mock::Midnight as Hooks<u64>>::on_finalize(1);
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

		// One page, and the empty page after it that completes the replay: seeing the
		// end of `UtxoOwners` costs nothing but the read, so it happens in the same step.
		assert_eq!(run_to_completion(), 1);

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

/// More rows than one batch: the cursor hands off between steps and every row is
/// visited exactly once (the tallies sum to the row count).
#[test]
fn pages_across_steps_visiting_every_row_once() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		// Every generated row resolves, so each page prices off real ledger work and
		// [`one_batch_budget`] takes exactly one of them per step. Two rows live only in
		// the pallet and are tallied as skipped.
		let rows = MAX_REAPPLY_BATCH * 2 + 5;
		let entries: Vec<(H256, u128)> = (0..rows)
			.map(|i| (H256::from_low_u64_be(i as u64 + 1), 100u128 + i as u128))
			.collect();
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}
		UtxoOwners::<Test>::insert(nonce(1), owner_bytes());
		UtxoOwners::<Test>::insert(nonce(2), owner_bytes());
		let total = UtxoOwners::<Test>::iter().count() as u32;

		// Two steps for three pages: a full page all but fills [`one_batch_budget`], but
		// the short last page and the read that completes the replay both fit in what is
		// left after the second one.
		assert_eq!(run_to_completion(), 2, "the cursor must hand off between steps");

		let Some(Event::DustReapplyCompleted { applied, skipped }) = cnight_events().pop() else {
			panic!("replay must complete, got {:?}", cnight_events());
		};
		assert_eq!(applied, rows);
		assert_eq!(applied + skipped, total, "every row must be visited exactly once");
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
	});
}

/// A batch the ledger rejects on its merits — here a surviving `Create` colliding
/// with `GenerationInfoAlreadyPresent`, which is also what the whole replay would hit
/// if a translation ever stopped wiping dust — reports its nonces and carries on to
/// the next page. Driven by replaying twice: the second run's ledger state already
/// holds the entry.
#[test]
fn rejected_batch_is_reported_and_the_replay_completes() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		let pre_fork_key = seed_pre_fork_state(&[(nonce(1), 100u128)]);
		PreForkStateKey::<Test>::put(pre_fork_key.clone());
		UtxoOwners::<Test>::insert(nonce(1), owner_bytes());
		run_to_completion();

		frame_system::Pallet::<Test>::reset_events();
		StorageVersion::new(1).put::<CNightObservation>();
		PreForkStateKey::<Test>::put(pre_fork_key);

		assert_eq!(run_to_completion(), 2, "the replay must carry on past a rejected batch");

		assert_eq!(
			cnight_events(),
			vec![
				Event::DustReapplyBatchFailed { nonces: vec![nonce(1)] },
				Event::DustReapplyCompleted { applied: 0, skipped: 1 },
			],
		);
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
	});
}

/// A block that fills up mid-replay must cost the replay latency, not a page.
#[test]
fn a_full_block_defers_the_page_rather_than_losing_it() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		// More `Create`s than one block's ledger budget affords (~220 on the current
		// parameters), so the step runs into the block limit before the last page.
		let rows = 250u32;
		let entries: Vec<(H256, u128)> = (0..rows)
			.map(|i| (H256::from_low_u64_be(i as u64 + 1), 100u128 + i as u128))
			.collect();
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}

		let mut meter = WeightMeter::new();
		let cursor = MigrateV1ToV2::<Test>::step(None, &mut meter).expect("step must not fail");

		let applied_first_block = applied_dust_events().len() as u32;
		assert!(cursor.is_some(), "the block filled up, so the replay cannot be finished");
		assert!(
			0 < applied_first_block && applied_first_block < rows,
			"the block must have filled part-way through, got {applied_first_block} of {rows}",
		);
		assert_eq!(
			DustReapplyProgress::<Test>::get(),
			(applied_first_block, 0),
			"a deferred page must not be tallied as skipped",
		);
		assert!(cnight_events().is_empty(), "a deferred page must not be evented as failed");

		// Fresh block, fresh ledger fullness: the deferred page comes back, and by the
		// end every single row has been restored.
		<mock::Midnight as Hooks<u64>>::on_finalize(1);
		run_from(cursor);

		assert_eq!(applied_dust_events().len() as u32, rows, "no row may be lost to a full block");
		let Some(Event::DustReapplyCompleted { applied, skipped }) = cnight_events().pop() else {
			panic!("replay must complete, got {:?}", cnight_events());
		};
		assert_eq!((applied, skipped), (rows, 0));
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

		assert_eq!(cnight_events(), vec![Event::DustReapplySkipped { applied: 0, skipped: 0 }]);
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
		assert!(applied_dust_events().is_empty());
	});
}

/// The loop inside a single `step`: `pallet_migrations` runs exactly one step per
/// block, so spending the MBM weight budget means applying several batches in that
/// one step. Given the runtime's real budget (80% of `max_block`), more than one
/// batch must land — and the step must stay inside its meter.
#[test]
fn one_step_packs_several_batches_into_its_budget() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		// Ten pages, every row resolvable, so every batch is priced off real ledger
		// work rather than the unmeasurable-page fallback. More pages than the budget
		// affords (~7 at the mock's 2e12 `max_block`), so the step has to hand a cursor
		// back rather than finish.
		let entries: Vec<(H256, u128)> = (0..MAX_REAPPLY_BATCH * 10)
			.map(|i| (H256::from_low_u64_be(i as u64 + 1), 100u128 + i as u128))
			.collect();
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}

		// `runtime::MbmServiceWeight`.
		let mut meter =
			WeightMeter::with_limit(Weight::from_parts(max_block() / 100 * 80, u64::MAX));

		let cursor = MigrateV1ToV2::<Test>::step(None, &mut meter).expect("step must not fail");

		assert!(cursor.is_some(), "ten pages must not all fit in one step");
		let (applied, skipped) = DustReapplyProgress::<Test>::get();
		assert_eq!(skipped, 0, "every seeded row resolves");
		assert!(
			applied > MAX_REAPPLY_BATCH,
			"more than one batch must land in a single step, got {applied}",
		);
		assert!(
			meter.consumed().all_lte(meter.limit()),
			"the step must not overrun its budget: consumed {:?} of {:?}",
			meter.consumed(),
			meter.limit(),
		);
	});
}

/// A batch that prices above what this migration may spend in a *whole* block can
/// never be applied — pages are 25 nonces, so no retry will ever make it fit. The
/// replay gives up instead of overrunning the budget or spinning forever.
#[test]
fn a_batch_that_outprices_a_whole_block_cancels() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		let entries = [(nonce(1), 100u128), (nonce(2), 250u128)];
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}

		let mut meter = WeightMeter::with_limit(Weight::from_parts(1, u64::MAX));
		let cursor = MigrateV1ToV2::<Test>::step(None, &mut meter).expect("step must not fail");

		assert!(cursor.is_none(), "an unaffordable batch must end the replay, not retry it");
		assert_eq!(cnight_events(), vec![Event::DustReapplySkipped { applied: 0, skipped: 0 }]);
		assert!(applied_dust_events().is_empty(), "nothing must have been applied");
		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);
	});
}

/// A batch that fits a whole block but not what is *left* of this one is not applied
/// at all: the step hands its cursor straight back and the next block, on a fresh
/// budget, applies that same page exactly once.
#[test]
fn a_batch_that_doesnt_fit_the_remaining_budget_retries_next_block() {
	new_test_ext().execute_with(|| {
		init_ledger_state();

		// A full page: a batch's price is per event, so a short page would fit the sliver
		// of budget left below and land instead of bouncing.
		let entries: Vec<(H256, u128)> = (0..MAX_REAPPLY_BATCH)
			.map(|i| (H256::from_low_u64_be(i as u64 + 1), 100u128 + i as u128))
			.collect();
		PreForkStateKey::<Test>::put(seed_pre_fork_state(&entries));
		for (nonce, _) in entries.iter() {
			UtxoOwners::<Test>::insert(nonce, owner_bytes());
		}

		// A whole `MbmServiceWeight` budget, but 5% of the block left in it: less than
		// the ~11% a batch prices at, while the *limit* stays a full budget so the batch
		// is not hopeless — just too big for the rest of this block.
		let mut meter =
			WeightMeter::with_limit(Weight::from_parts(max_block() / 100 * 80, u64::MAX));
		meter.consume(Weight::from_parts(max_block() / 100 * 75, 0));

		let cursor = MigrateV1ToV2::<Test>::step(None, &mut meter).expect("step must not fail");

		assert_eq!(cursor, Some(None), "the step must hand back the cursor it was given");
		assert!(applied_dust_events().is_empty(), "nothing must have been applied");
		assert_eq!(DustReapplyProgress::<Test>::get(), (0, 0));
		assert!(cnight_events().is_empty(), "the replay must not be wound up");

		// Next block, fresh budget: the same page lands, and only once.
		<mock::Midnight as Hooks<u64>>::on_finalize(1);
		let mut meter = WeightMeter::with_limit(one_batch_budget());
		MigrateV1ToV2::<Test>::step(cursor, &mut meter).expect("step must not fail");

		assert_eq!(applied_dust_events().len(), entries.len(), "the page must land exactly once");
		assert_eq!(
			cnight_events(),
			vec![Event::DustReapplyCompleted { applied: MAX_REAPPLY_BATCH, skipped: 0 }],
		);
	});
}

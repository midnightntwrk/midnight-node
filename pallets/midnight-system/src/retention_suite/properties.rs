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

//! The properties of keeping intra-block ledger states alive, and every test of them.
//!
//! During block execution only the post-block tip is persisted. Intra-block states are held in
//! `LedgerStatesExt` until the post-block update releases them, before the flush. Off-chain
//! states are held in `LedgerEphemeralExt` until the execution ends.
//!
//! Kinds follow Hughes, *How to Specify It!*: an invariant holds in every reachable world, a
//! postcondition checks the world right after one call, a model-based property compares against
//! a simpler model, and a test-code check or generator requirement checks the test itself.
//!
//! - P1, valid worlds (invariant): [`live_keys_resolve`], [`roots_are_tips`].
//! - P2, the test only produces histories the node can run (test-code check):
//!   [`history_is_well_formed`], over [`well_formed`].
//! - P3, the histories the test must produce (generator requirement):
//!   [`generator_reaches_every_class`].
//! - P4, no leaks (postcondition of flush): [`released_states_gone`],
//!   [`disk_reachable_from_roots`].
//! - P5, same results as the never-forget store (model-based): [`same_results`].
//!
//! [`retention_holds_over_random_histories`] checks P1 after every call, P4 after every
//! `finalize_block` and P5 per call, on histories that never finalize a block while another
//! holds states. [`flush_while_another_execution_holds`] lifts that restriction, and fails P4: a
//! block's flush writes the states another block holds to disk, with no root.
//!
//! Calls go through the runtime: `frame_executive::Executive` over this pallet's mock, with the
//! extensions `node/src/extensions.rs` registers. The harness picks the storage of each world,
//! the order calls interleave in, and which runtime calls run; the runtime does the rest.
//!
//! Not covered:
//! - Ledger 8, which the runtime no longer drives.
//! - Host functions only older runtimes call, as a node syncing from genesis does:
//!   `migrate_state_v8_to_v9`, `post_block_update`, and older host function versions.
//! - P6, held states are not re-read: a speed property, which needs an arena deserialization
//!   counter in midnight-ledger.
//! - Agreement with nodes that run older code, and memory and speed.
//! - Restarts (`drop_all_default_storage`), separate against unified storage, `has_ledger_state`
//!   on an unknown key, real threads, and reclaiming by time-to-idle.
//! - The validation cache's `VerifiedTransaction`s, which a flush also writes to disk. The
//!   harness empties the cache before each call, so P4 sees only the states, and P5's model
//!   never reads an answer the real world cached.
//!
//! Interleaving is calls on different executions alternating in one thread, which is
//! deterministic and shrinks, and reaches the process-wide storage as threads do. One execution
//! is one `TestExternalities`, as one runtime-API instance keeps its extensions across the
//! calls of a block.
//!
//! # Definitions
//!
//! ```text
//! world                   the state every call runs against:
//!   arena                   default_storage<D>: in-memory nodes, write cache, root counts
//!   disk                    the ledger DB under the arena
//!   executions              the live runtime-API instances; each one has:
//!     onchain                 LedgerOnchainExt is registered (block authoring or import)
//!     held                    LedgerStatesExt: its intra-block states
//!     held_offchain           LedgerEphemeralExt and Storage<EphemeralDb<D>>: its off-chain states
//!     state_key               pallet StateKey in its overlay (FRAME storage, transactional)
//!     layers                  its open with_storage_layer frames, each with the state_key it reverts to
//!
//! call                    one of, inside one execution:
//!   apply(key, tx)          apply_transaction | apply_system_transaction[_checked], ledger 8 or 9
//!   post_block(key)         post_block_update | apply_post_block_update
//!   migrate(key)            migrate_state_v8_to_v9 (persists its output and flushes, on-chain)
//!   flush                   flush_storage
//!   read(key)               a call that only loads through get_ledger:
//!                           get_ledger_state_root, get_transaction_cost, validate_*, pre_dispatch
//!   open_layer | commit | rollback
//!                           rollback: the innermost layer fails, state_key := its saved value
//!   end_execution           the runtime-API instance and its extensions are dropped
//!
//! history                 a sequence of calls, from a world holding only the persisted genesis
//! well_formed(history)    a history the node can produce:
//!                           on-chain apply and post_block use the execution's state_key
//!                           migrate runs at most once, on-chain, at the start of a block
//!                           every commit or rollback closes an open layer, and none is open at post_block
//!                           an on-chain execution ends: post_block, flush, end_execution,
//!                             or ends early (an abandoned proposal or a failed import)
//!                           off-chain calls use any key in live_keys
//!                           executions interleave
//! world_after(history)    the world once the history has run
//! test_cases()            the histories the test's test-case generator produces at random
//! shrink(history)         shrinking: when a check fails, the test retries it on smaller
//!                         versions of the failing history (fewer calls, smaller txs) and
//!                         reports the smallest one that still fails
//!
//! tips(world)             genesis, the post_block key of every block, and the output of every
//!                         on-chain migrate, each counted once per time it was produced
//! live_keys(world)        every tip, and for each live execution: its state_key and every key
//!                         saved by its open layers
//! produced(history)       every key an apply, post_block or migrate in the history returned
//! resolves(world, key)    get_ledger(key) is Ok and every node under key can be read
//! root_count(world, key)  the arena's root count for key, 0 when absent
//! results(history)        each call's whole return value: the state_root bytes, every
//!                         AppliedStateRoot / SystemTransactionAppliedStateRoot field, or the exact
//!                         LedgerApiError
//! model                   a reference store: allocs every state into its own arena, persists it,
//!                         and never releases or unpersists anything (the never-forget store)
//! deserializations(call)  the arena deserializations of a ledger root during the call
//!
//! key equality            byte equality; keys are content hashes, so equal keys are equal ledgers
//! ```
//!
//! How the definitions are reached:
//! - apply: an unsigned `send_mn_transaction` through `Executive::apply_extrinsic`, which runs
//!   `pre_dispatch` (a read) first; or a cNIGHT update through this pallet's
//!   `MidnightSystemTransactionCNightExecutor`, in a storage layer as the cNIGHT inherent applies
//!   it. Several per block stand in for a block of transactions, which are costly to generate.
//! - post_block and flush: `Executive::finalize_block`, through `on_finalize`.
//! - read: `validate_transaction`, `get_transaction_cost` and `get_ledger_state_root`, off-chain.
//! - rollback: an inherent whose later transaction fails rolls back the earlier ones.
//! - off-chain executions run reads and dry-run applies, at a tip: a runtime API call runs at an
//!   imported block, so it never sees another execution's intra-block state.
//! - migrate is not reached: the runtime no longer calls it.

use super::{
	Call, Class, Classes, Entry, History, Outcome, Step, check_history, history, world::Hash,
};
use proptest::{prelude::*, strategy::ValueTree, test_runner::TestRunner};
use std::collections::{BTreeMap, HashMap};

// --- Properties ---

/// P1, live keys resolve (invariant). Every key the node may still load, a block's tip, an
/// execution's current state or a rollback target, loads in full.
///
/// A released state the node still loads panics on first deref, which wedges block production.
pub fn live_keys_resolve(step: Step, live: &[(Vec<u8>, bool)]) -> Result<(), TestCaseError> {
	let unresolved: Vec<_> = live.iter().filter(|(_, resolves)| !resolves).collect();
	prop_assert!(unresolved.is_empty(), "P1 live keys resolve: {:?} at {:?}", unresolved, step);
	Ok(())
}

/// P1, only tips are roots, once per time each was produced (invariant).
///
/// The count is strict because nothing garbage-collects abandoned forks today. A double persist
/// leaves a count of 2 where 1 is right, and a later fork GC that releases one count would leave
/// the state alive.
pub fn roots_are_tips(
	step: Step,
	roots: &HashMap<Hash, u32>,
	tips: &HashMap<Hash, u32>,
) -> Result<(), TestCaseError> {
	prop_assert_eq!(roots, tips, "P1 roots are tips: {:?}", step);
	Ok(())
}

/// The rules a history the node can run follows (see `well_formed` in the module doc). Checks the
/// shape of the calls: which tip an `Index` picks is settled when the history runs.
pub fn well_formed(history: &History) -> Result<(), String> {
	let mut live = BTreeMap::new();
	let mut ended = std::collections::BTreeSet::new();
	for (i, call) in history.iter().enumerate() {
		let id = call.exec();
		let fail = |why: &str| Err(format!("call {i}, {call:?}: {why}"));
		if let Call::Start { onchain, .. } = *call {
			if live.contains_key(&id) || ended.contains(&id) {
				return fail("the execution already started");
			}
			live.insert(id, (onchain, false));
			continue;
		}
		let Some((onchain, finalized)) = live.get_mut(&id) else {
			return fail("the execution is not running");
		};
		match call {
			Call::Start { .. } => {},
			_ if *finalized && !matches!(call, Call::End { .. }) => {
				return fail("after finalize_block");
			},
			Call::Inherent { .. } | Call::Finalize { .. } if !*onchain => {
				return fail("an inherent or finalize_block off-chain");
			},
			Call::Read { .. } if *onchain => return fail("a runtime API read on-chain"),
			Call::Finalize { .. } => *finalized = true,
			Call::End { .. } => {
				live.remove(&id);
				ended.insert(id);
			},
			Call::Inherent { .. } | Call::Extrinsic { .. } | Call::Read { .. } => {},
		}
	}
	match live.keys().next() {
		Some(id) => Err(format!("execution {id} never ends")),
		None => Ok(()),
	}
}
/// P2, the test only produces histories the node can run (test-code check). Checked on every
/// generated history, and on every shrink of one.
///
/// A history the node cannot produce can fail a property for a reason production never meets.
pub fn history_is_well_formed(history: &History) -> Result<(), TestCaseError> {
	let verdict = well_formed(history);
	prop_assert!(verdict.is_ok(), "P2: {}: {:?}", verdict.unwrap_err(), history);
	Ok(())
}

/// P4, released states are gone (postcondition of flush). After a flush, no produced state that
/// is not a tip, and that no live execution holds, loads.
///
/// Checked on nodes: `leftover` is each node of such a state that still loads and that no live
/// key reaches.
pub fn released_states_gone(step: Step, leftover: &[Hash]) -> Result<(), TestCaseError> {
	prop_assert!(
		leftover.is_empty(),
		"P4 released states gone: {} nodes still load at {:?}",
		leftover.len(),
		step
	);
	Ok(())
}

/// P4, the disk holds only what the roots reach (postcondition of flush).
///
/// Nothing garbage-collects the ledger DB, so a node no root reaches stays on disk for good.
/// This is the only property that sees a flush writing another block's held states: they have
/// no root and nothing loads them, so no root count or result changes.
pub fn disk_reachable_from_roots(step: Step, unreachable: &[Hash]) -> Result<(), TestCaseError> {
	prop_assert!(
		unreachable.is_empty(),
		"P4 disk reachable from roots: {} unreachable nodes at {:?}",
		unreachable.len(),
		step
	);
	Ok(())
}

/// P5, same results as the never-forget store (model-based). Every call returns, byte for byte,
/// what it returns against a store that keeps every state forever.
///
/// The model shares only the pure ledger functions with the code under test, so a released
/// state shows up as an error, and a key serialized in another form as different bytes.
pub fn same_results(step: Step, real: &Outcome, model: &Outcome) -> Result<(), TestCaseError> {
	prop_assert_eq!(real, model, "P5 same results: {:?}", step);
	Ok(())
}

// --- Tests ---

/// No regressions file: the shrunk history is in the failure message.
fn config() -> ProptestConfig {
	ProptestConfig { failure_persistence: None, ..ProptestConfig::default() }
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..config() })]

	/// P1 after every call, P4 after every flush and P5 per call, over random histories in which
	/// no block flushes while another holds states.
	#[test]
	fn retention_holds_over_random_histories(history in history(6, false)) {
		check_history(&history, &mut Classes::new())?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..config() })]

	/// As [`retention_holds_over_random_histories`], where a block's flush can run while another
	/// block holds states, as in production when one block's `on_finalize` runs during another's
	/// proposal or import. Fails P4's disk clause: the flush writes the other block's held states
	/// to disk with no root.
	#[test]
	#[ignore = "known leak: a flush writes another execution's held states to disk"]
	fn flush_while_another_execution_holds(history in history(6, true)) {
		check_history(&history, &mut Classes::new())?;
	}
}

proptest! {
	#![proptest_config(config())]

	/// As [`retention_holds_over_random_histories`], with longer histories. Scale with
	/// `PROPTEST_CASES`:
	///
	/// `PROPTEST_CASES=1000 cargo test --release -p pallet-midnight-system --lib retention_suite \
	///     -- --ignored retention_holds_over_random_histories_nightly \
	///     --skip flush_while_another_execution_holds`
	#[test]
	#[ignore = "slow: run nightly, scaled with PROPTEST_CASES"]
	fn retention_holds_over_random_histories_nightly(history in history(16, false)) {
		check_history(&history, &mut Classes::new())?;
	}
}

/// P3, the generators reach every case the properties need: forks, re-imports, rollbacks after
/// an apply and the apply replayed, abandoned blocks, off-chain applies and executions at the
/// same keys, reads, and errors at every entry point. Without them P1, P4 and P5 would hold
/// vacuously.
///
/// The concurrent flush is reached only by [`flush_while_another_execution_holds`]'s generator.
/// Its histories fail P4, so here only the cases they reach count.
#[test]
fn generator_reaches_every_class() {
	let mut runner = TestRunner::deterministic();
	let mut classes = Classes::new();
	for _ in 0..32 {
		let history = history(6, false).new_tree(&mut runner).unwrap().current();
		check_history(&history, &mut classes).unwrap();
	}
	let mut concurrent = Classes::new();
	for _ in 0..8 {
		let history = history(6, true).new_tree(&mut runner).unwrap().current();
		let _ = check_history(&history, &mut concurrent);
	}

	let every = [
		Class::Fork,
		Class::Reimport,
		Class::RollbackAfterApply,
		Class::HeldTwice,
		Class::Abandoned,
		Class::SameKeyOnAndOffChain,
		Class::OffchainApply,
		Class::Read,
		Class::Error(Entry::Inherent),
		Class::Error(Entry::Extrinsic),
		Class::Error(Entry::Read),
	];
	let missing: Vec<_> = every.iter().filter(|class| !classes.contains(class)).collect();
	assert!(missing.is_empty(), "never reached {missing:?}; reached {classes:?}");
	assert!(
		!classes.contains(&Class::FlushWhileHeld),
		"the per-PR generator flushed while another block held states"
	);
	assert!(concurrent.contains(&Class::FlushWhileHeld), "reached {concurrent:?}");
}

/// P2 on generated histories and on their shrinks, from both generators.
#[test]
fn shrinks_stay_well_formed() {
	let mut runner = TestRunner::deterministic();
	for concurrent_flush in [false, true] {
		for _ in 0..64 {
			let mut tree = history(16, concurrent_flush).new_tree(&mut runner).unwrap();
			history_is_well_formed(&tree.current()).unwrap();
			for _ in 0..256 {
				if !tree.simplify() {
					break;
				}
				history_is_well_formed(&tree.current()).unwrap();
			}
		}
	}
}

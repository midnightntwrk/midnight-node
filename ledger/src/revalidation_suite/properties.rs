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

//! The properties of [`TxValidationCache`], and every test of them.
//!
//! The cache maps `(runtime_version, tx_hash)` to a receipt `(state_key, tblock)`: the state and
//! effective tblock at which the tx last passed `well_formed`. On a hit, `well_formed` runs
//! against `RevalidationReference { prev: receipt's state, new: call's state }`, which skips
//! proof verification and repeats only the checks whose inputs changed. The reference for every
//! comparison is the cold call: the same entry point against an empty cache, which always misses.
//!
//! Each property's statement and reason are on its check function below. Kinds follow Hughes,
//! *How to Specify It!*: a metamorphic property compares two related calls, a postcondition checks
//! one call against its inputs, a model-based one compares against a simpler model, and an
//! invariant holds for every reachable cache.
//!
//! - P1, same result (metamorphic): [`same_result`].
//! - P3, history independence: P1, with receipts from the call's own state, an ancestor, a
//!   descendant, a sibling fork, and from a later tblock.
//! - P4, eviction transparency: P1, with histories that evict receipts and point them at a state
//!   no storage holds.
//! - P5, rejections are inert: P1, with histories holding calls rejected at `well_formed`, the
//!   dry-run and `apply`.
//! - P6, partitioning: receipts never serve another runtime version, which is P8a's no-receipt
//!   row; receipts are shared between on-chain and off-chain storage, and P1 holds across them.
//! - P8a, the hit table (model-based): [`outcome_follows_table`], [`metric_records_outcome`],
//!   [`receipt_follows_table`].
//! - P8b, an off-chain receipt serves on-chain (postcondition): [`offchain_receipt_serves_onchain`].
//! - P8c, a rejection keeps the receipt, and P8d, success writes a receipt (postconditions): rows
//!   of [`receipt_follows_table`].
//! - P8e, a hit verifies only the proofs it has to (postcondition): [`proofs_verified`].
//! - P9a, disk parity (metamorphic): [`flushes_match`].
//! - P10, receipts are true (invariant): [`receipt_is_true`].
//!
//! P3, P4, P5 and P6 constrain the histories rather than add a check, so
//! [`generator_reaches_every_class`] checks the histories reach each case. Not tested here: P2,
//! merged into P1, which compares the whole return value; P7, revalidation equals a full check,
//! which is the ledger's to test; and P9b, no ledger allocations held, which holds by the type of
//! `ProofReceipt`, with P9a as its observable consequence.

use super::history::{Class, Classes, Relation, Stage, check_history, history};
use super::ledger::{
	ProofReceipt, TX_VALIDATION_CACHE_CAPACITY, Timestamp, TxValidationCache,
	TxValidationCacheOutcome, api,
	ledger_storage_local::{
		DefaultDB,
		arena::{ArenaKey, TypedArenaKey},
		db::DB,
	},
	mn_ledger_local::structure::ProofMarker,
	types::{LedgerApiError, TransactionError},
};
use super::{
	Action, At, Call, Exts, Lost, Step, Storage, cache_key, capacity, disk, lookup, missing_state,
	model::{self, ModelCall, OutcomeRow, Receipt, ReceiptRow},
	one_hot, peek, predict,
	proofs::{self, Counts},
	run, shim, smoke_tree, smoke_trees, target, threads,
	tree::{RUNTIME_VERSION, Tree, TreeBridge, TxKind, build_tree},
	tree_spec,
};
use proptest::{prelude::*, sample::Index, strategy::ValueTree};
use sp_externalities::Externalities;
use std::collections::{BTreeMap, HashSet};

type Verdict = Result<Vec<u8>, LedgerApiError>;

// --- Properties ---

/// P1, same result. For any history and any call, a warm call returns what the same call returns
/// against an empty cache: the same error, or the same success value, state root included.
///
/// The cache must never change a decision, or nodes with different histories disagree on a block.
/// Comparing the whole value catches a stale fee, which changes the state root of a call that
/// still succeeds.
pub fn same_result(step: &Step, warm: &Verdict, cold: &Verdict) -> Result<(), TestCaseError> {
	prop_assert_eq!(warm, cold, "P1: {:?}", step);
	Ok(())
}

/// P8a, the outcome a call that reaches the cache gets, from the receipt it finds for its key:
///
/// | receipt                                         | outcome          |
/// |-------------------------------------------------|------------------|
/// | none                                            | miss             |
/// | the call's state, at its effective tblock       | strict hit       |
/// | the call's state, at another tblock             | revalidation hit |
/// | another state, which still loads                | revalidation hit |
/// | a state that no longer loads                    | miss, and evicted |
///
/// Every other property holds for a cache that never hits; this one makes it hit when it can.
/// `actual` is `lookup_receipt`'s answer. Returns the row, for coverage.
pub fn outcome_follows_table(
	step: &Step,
	call: &ModelCall,
	before: Option<&Receipt>,
	loadable: bool,
	actual: &TxValidationCacheOutcome,
) -> Result<OutcomeRow, TestCaseError> {
	let (row, expected) = model::outcome(before, loadable, call);
	prop_assert_eq!(actual, &expected, "P8a outcome, row {:?}: {:?}", row, step);
	Ok(row)
}

/// P8a, as operators see it: a call that succeeds moves the metric of the outcome its last
/// attempt got. A call that fails moves none, as the entry points record outcomes on success
/// only.
pub fn metric_records_outcome(
	step: &Step,
	call: &ModelCall,
	before: Option<&Receipt>,
	loadable: bool,
	succeeded: bool,
	moved: [u64; 3],
) -> Result<(), TestCaseError> {
	let expected = match model::attempts(before, loadable, call).last() {
		Some(outcome) if succeeded && call.reaches_cache => one_hot(outcome),
		_ => [0; 3],
	};
	prop_assert_eq!(moved, expected, "P8a outcome metric: {:?}", step);
	Ok(())
}

/// P8a, the receipt a call leaves for its key, which also states P8c and P8d:
///
/// | call                                          | receipt after                         |
/// |-----------------------------------------------|---------------------------------------|
/// | passed `well_formed`                          | its state and effective tblock (P8d)  |
/// | failed, and the receipt's state loads         | unchanged (P8c)                       |
/// | failed, and no receipt or its state is gone   | none                                  |
/// | never reached the cache                       | unchanged                             |
///
/// P8c: a rejected tx can become valid after a reorg, and should then be revalidated, not
/// verified again. P8d, including straight after an eviction: a lost receipt costs one full
/// verification, not one per call. Returns the row, for coverage.
pub fn receipt_follows_table(
	step: &Step,
	call: &ModelCall,
	before: Option<&Receipt>,
	loadable: bool,
	after: Option<&Receipt>,
) -> Result<ReceiptRow, TestCaseError> {
	let (row, expected) = model::receipt_after(before, loadable, call);
	prop_assert_eq!(after, expected.as_ref(), "P8a receipt, row {:?}: {:?}", row, step);
	Ok(row)
}

/// P8e, a hit verifies only the proofs it has to. P8a only sees the outcome label the node
/// assigns; the skipping happens in the ledger's `RevalidationReference`, so this counts the
/// proofs actually verified:
/// - a warm call verifies no more of each kind than the same call against an empty cache;
/// - a call that stops before the cache verifies none;
/// - a miss that passes on its first attempt verifies each of the tx's proofs once;
/// - a strict hit verifies none.
///
/// Which proofs a revalidation hit verifies again is left to the ledger's tests: predicting it
/// here would re-implement `RevalidationReference`.
#[allow(clippy::too_many_arguments)]
pub fn proofs_verified(
	tree: &Tree,
	tx: usize,
	call: &ModelCall,
	before: Option<&Receipt>,
	loadable: bool,
	warm: Counts,
	cold: Counts,
) -> Result<(), TestCaseError> {
	prop_assert!(
		warm.contract <= cold.contract && warm.dust <= cold.dust && warm.zswap <= cold.zswap,
		"P8e: the warm call verified {:?}, more than the cold call's {:?}",
		warm,
		cold
	);
	if !call.reaches_cache {
		prop_assert_eq!(warm, Counts::default(), "P8e: verified before reaching the cache");
		return Ok(());
	}
	// A retried cold call verifies its failed first attempt's proofs too.
	if call.attempts.len() == 1 && call.passes_well_formed {
		prop_assert_eq!(cold, model::proofs_in(tree, tx), "P8e: a miss verifies each proof once");
	}
	let attempts = model::attempts(before, loadable, call);
	if attempts.iter().all(|a| *a == TxValidationCacheOutcome::StrictCacheHit) {
		prop_assert_eq!(warm, Counts::default(), "P8e: a strict hit verifies nothing");
	}
	Ok(())
}

/// P9a, disk parity. A warm node's block flush writes exactly the nodes and root counts a cold
/// node's does, or the cache grows the database.
pub fn flushes_match(
	step: &Step,
	warm: &disk::Writes,
	cold: &disk::Writes,
) -> Result<(), TestCaseError> {
	prop_assert!(
		warm == cold,
		"P9a after {:?}\nwarm only: {:?}\ncold only: {:?}",
		step,
		warm.difference(cold).collect::<Vec<_>>(),
		cold.difference(warm).collect::<Vec<_>>(),
	);
	Ok(())
}

/// P10, receipts are true. Every receipt records a check that really passed: `well_formed` in
/// full, at the receipt's state and tblock, passes.
///
/// The other properties are stated over one sequence of calls, but production shares one cache
/// between pool validation and block import, on several threads. This one holds whatever the
/// order. Receipts in `orphans` are false by construction, planted by the harness.
pub fn receipt_is_true(
	tree: &Tree,
	tx: usize,
	receipt: Option<Receipt>,
	orphans: &HashSet<Vec<u8>>,
) -> Result<(), TestCaseError> {
	let Some((state_key, tblock)) = receipt.filter(|(state, _)| !orphans.contains(state)) else {
		return Ok(());
	};
	prop_assert_ne!(tree.txs[tx].kind, TxKind::WrongKey, "P10: a receipt for the wrong-key call");
	let ledger = TreeBridge::load_ledger(&api::new(), &state_key).expect("tree states stay stored");
	let verdict = model::well_formed(tree, tx, &ledger.state, tblock);
	prop_assert!(
		verdict.is_ok(),
		"P10: false receipt for tx {} at node {:?}, {:?}: {:?}",
		tx,
		tree.nodes.iter().position(|n| n.state_key == state_key),
		tblock,
		verdict.err()
	);
	Ok(())
}

/// A `store` call proved for a key no contract holds is rejected wherever it is sent. Not a cache
/// property: it shows the dummy proofs still bind a call to its verifier key.
pub fn wrong_key_is_rejected(step: &Step, warm: &Verdict) -> Result<(), TestCaseError> {
	prop_assert!(warm.is_err(), "the wrong-key call passed: {:?}", step);
	Ok(())
}

// --- Tests ---

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

	/// P1, P3 to P6, P8a, P8c to P8e and P10, over random histories on the fixed smoke trees.
	///
	/// At capacity 2, moka evicts and refuses receipts on its own, which tests P4 against real
	/// eviction but makes the P8 tables unpredictable, so they are checked at production capacity
	/// only. A failure at capacity 2 may not replay exactly: moka's admission isn't driven by the
	/// seed. `Evict` is the reproducible form.
	fn warm_cache_gives_the_cold_verdict(
		tree in smoke_tree(),
		capacity in capacity(),
		actions in history(40),
	) {
		let trees = smoke_trees();
		check_history(&trees[tree], capacity, &actions, &mut Classes::new())?;
	}
}

proptest! {
	/// As [`warm_cache_gives_the_cold_verdict`], with a fresh tree per case, so a failure shrinks
	/// the tree as well as the history.
	///
	/// Every tree's states stay in the process-wide storage, about 0.6 MB per case. Scale by
	/// running repeatedly rather than raising `PROPTEST_CASES` without limit: each run draws a new
	/// seed.
	///
	/// `PROPTEST_CASES=1000 cargo test --release -p midnight-node-ledger --lib -- --ignored \
	///     warm_cache_gives_the_cold_verdict_on_generated_trees`
	#[test]
	#[ignore = "slow: run nightly, scaled with PROPTEST_CASES"]
	fn warm_cache_gives_the_cold_verdict_on_generated_trees(
		spec in tree_spec(),
		capacity in capacity(),
		actions in history(60),
	) {
		check_history(&build_tree(&spec), capacity, &actions, &mut Classes::new())?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

	/// P8b, an off-chain receipt serves on-chain. After off-chain mempool validation accepts a
	/// tx, `validate_guaranteed_execution` and `apply_transaction` at the same state, on-chain,
	/// hit its receipt.
	///
	/// Proofs verified at admission must not be verified again when the node builds or imports
	/// the block. Random histories rarely send one tx through the mempool and then on-chain at
	/// the same state.
	fn offchain_receipt_serves_onchain(
		tree in any::<Index>(),
		[kind, tx] in any::<[Index; 2]>(),
		node in prop_oneof![Just(At::Parent), any::<Index>().prop_map(At::Any)],
		dt in -60i64..=60,
		runtime_version in 1u32..=2,
		[check_utxos, skew_tblock] in any::<[bool; 2]>(),
	) {
		let trees = smoke_trees();
		let tree = &trees[tree.index(trees.len())];
		let step =
			|call| Step { kind, tx, node: node.clone(), dt, parent_gap: 0, runtime_version, call };
		let offchain = step(Call::Validate {
			pre_dispatch: false,
			skew_tblock: false,
			check_utxos,
			ephemeral: true,
			get_tx_details: false,
		});
		let onchain = [
			Call::Validate {
				pre_dispatch: true,
				skew_tblock,
				check_utxos,
				ephemeral: false,
				get_tx_details: false,
			},
			Call::Apply { skew_tblock, filter_only_failed_segments: true, net_intra_tx_utxos: true },
		];
		for call in onchain.map(step) {
			let mut exts = Exts::new();
			let cache = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
			if run::<DefaultDB>(tree, &offchain, &mut exts, &cache).is_err() {
				return Ok(());
			}
			let model = ModelCall::new(tree, &call);
			let key = cache_key(tree, target(tree, &call).0, runtime_version);
			let receipt = peek(&cache, &key);
			let outcome = predict(
				&mut exts,
				Storage::OnChain,
				&model.state_key,
				model.tblock,
				&key,
				receipt.as_ref(),
			);
			prop_assert_ne!(&outcome, &TxValidationCacheOutcome::CacheMiss, "P8b: {:?}", call);
			let passing = model::attempts(receipt.as_ref(), true, &model).pop().unwrap();

			let (result, moved) = exts.outcome_moved(|exts| run::<DefaultDB>(tree, &call, exts, &cache));
			if result.is_ok() {
				prop_assert_eq!(moved, one_hot(&passing), "P8b metric: {:?}", call);
			}
		}
	}
}

/// P8a's not-loadable row, for a receipt whose ledger root is small enough to be stored inline in
/// its key, and one of whose children is gone: a miss, and the receipt is evicted.
///
/// Random histories only orphan receipts by referenced roots. A root is inlined when it
/// serializes to at most `SMALL_OBJECT_LIMIT` bytes, children inlined in it included. Genesis
/// after a block end still fits, and `Bridge` keys a state before persisting it, which would turn
/// the key into a reference.
fn receipt_on_a_gone_inline_root_misses() {
	let tree = &smoke_trees()[0];
	let genesis = &tree.nodes[0];
	let root = TreeBridge::post_block_update(
		&mut Exts::new().onchain.ext(),
		&genesis.state_key,
		genesis.block_context.clone(),
	)
	.unwrap();
	let key: TypedArenaKey<api::Ledger<DefaultDB>, <DefaultDB as DB>::Hasher> =
		api::new().tagged_deserialize(&root).unwrap();
	assert!(matches!(key.key, ArenaKey::Direct(_)), "genesis after a block end is inlined");
	let node = &tree.nodes[tree.txs[0].parent];
	let key = cache_key(tree, 0, RUNTIME_VERSION);
	let tblock = Timestamp::from_secs(node.block_context.tblock);
	let cache = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
	cache
		.0
		.insert(key.clone(), ProofReceipt { state_key: missing_state(&root), tblock });

	let outcome = lookup(&mut Exts::new(), Storage::OnChain, &node.state_key, tblock, &key, &cache);
	assert_eq!(outcome, TxValidationCacheOutcome::CacheMiss, "P8a");
	assert_eq!(peek(&cache, &key), None, "P8a: the receipt is evicted");
}

/// P1 and P8a for a call ledger 8 retries: a skewed call at a block start, an hour after its
/// parent block. Its corrected tblock is an hour before the block's, where every tree tx fails
/// `well_formed`, so ledger 8 retries at the block's own.
///
/// Random steps never pass a retry: every tree state is minutes from every build, so the
/// corrected tblock fails no check the block's passes. Run with and without a mempool receipt at
/// the block's tblock, which the retry then strictly hits.
fn retry_at_the_block_tblock() {
	let strategy = (smoke_tree(), any::<[Index; 2]>(), any::<[bool; 2]>(), 1u32..=2);
	let mut runner = proptest::test_runner::TestRunner::deterministic();
	let trees = smoke_trees();
	let mut classes = Classes::new();
	for _ in 0..64 {
		let (tree, [kind, tx], [apply, receipt], runtime_version) =
			strategy.new_tree(&mut runner).unwrap().current();
		let step = |call, parent_gap| {
			Action::Call(Step {
				kind,
				tx,
				node: At::Parent,
				dt: 0,
				parent_gap,
				runtime_version,
				call,
			})
		};
		let mempool = Call::Validate {
			pre_dispatch: false,
			skew_tblock: false,
			check_utxos: true,
			ephemeral: false,
			get_tx_details: false,
		};
		let skewed = if apply {
			Call::Apply {
				skew_tblock: true,
				filter_only_failed_segments: true,
				net_intra_tx_utxos: true,
			}
		} else {
			Call::Validate {
				pre_dispatch: true,
				skew_tblock: true,
				check_utxos: true,
				ephemeral: false,
				get_tx_details: false,
			}
		};
		let actions: Vec<_> = receipt
			.then(|| step(mempool, 0))
			.into_iter()
			.chain([step(skewed, 3600)])
			.collect();
		check_history(&trees[tree], TX_VALIDATION_CACHE_CAPACITY, &actions, &mut classes).unwrap();
	}
	println!("{classes:#?}");
	if shim::RETRY_AT_BLOCK_TBLOCK {
		assert!(classes.contains_key(&Class::Retried(true)), "no retry passed");
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

	/// P9a and P1 over random calls on the fixed smoke trees, with block flushes between them.
	fn warm_node_flushes_what_a_cold_one_does(tree in any::<Index>(), steps in disk::steps()) {
		let _worlds = disk::worlds();
		let trees = smoke_trees();
		disk::check_disk_parity(&trees[tree.index(trees.len())], &steps, None)?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

	/// P10 and P1 with threads sharing one cache, as pool validation and block import do.
	///
	/// Interleavings don't replay from the seed: rerun a failing case several times.
	fn receipts_stay_true_across_threads(case in threads::case()) {
		let trees = smoke_trees();
		let (tree, histories) = threads::restrict(&trees[case.tree.index(trees.len())], &case);
		threads::check_threads(&tree, case.capacity, &histories)?;
	}
}

// --- Coverage: the properties above test something ---

/// The dummy proofs are the only thing letting tree transactions through, and their bytes
/// deserialize under both proof kinds. Otherwise the suite would test a ledger that skips checks
/// production runs.
fn dummy_proofs_pass_only_unverified() {
	let tree = &smoke_trees()[0];
	let tx = &tree.txs[0];
	let node = &tree.nodes[tx.parent];
	let mut ext = sp_io::TestExternalities::default();
	let mut ext = ext.ext();
	let validate = |ext: &mut dyn Externalities, real: bool| {
		let cache = TxValidationCache::new(1);
		let bc = node.block_context.clone();
		if real {
			shim::validate_transaction::<DefaultDB, ProofMarker>(
				ext,
				&node.state_key,
				&tx.bytes,
				bc,
				RUNTIME_VERSION,
				u64::MAX,
				false,
				true,
				&cache,
			)
		} else {
			shim::validate_transaction::<DefaultDB, proofs::UnverifiedProofs>(
				ext,
				&node.state_key,
				&tx.bytes,
				bc,
				RUNTIME_VERSION,
				u64::MAX,
				false,
				true,
				&cache,
			)
		}
	};

	validate(&mut ext, false).expect("UnverifiedProofs accepts the dummy proofs");
	let real = validate(&mut ext, true).unwrap_err();
	assert!(
		matches!(real, LedgerApiError::Transaction(TransactionError::Malformed(_))),
		"ProofMarker must reject the dummy proofs as malformed, got {real:?}"
	);
}

/// The histories reach every case P3 to P6 name, every row of the P8a tables, and every P8e
/// case. Without them those properties would hold vacuously.
///
/// At production capacity only: at capacity 2 moka evicts on its own schedule, which would make
/// the counts vary between runs.
fn generator_reaches_every_class() {
	let mut runner = proptest::test_runner::TestRunner::deterministic();
	let strategy = (smoke_tree(), history(40));
	let trees = smoke_trees();
	let mut classes = Classes::new();
	for _ in 0..64 {
		let (tree, actions) = strategy.new_tree(&mut runner).unwrap().current();
		let tree = &trees[tree];
		check_history(tree, TX_VALIDATION_CACHE_CAPACITY, &actions, &mut classes).unwrap();
	}
	println!("{classes:#?}");

	use super::history::ProofCase;
	use Relation::*;
	use Storage::*;
	let every = [Absent, Own, Ancestor, Descendant, Sibling, Gone]
		.map(Class::Receipt)
		.into_iter()
		.chain([true, false].map(Class::EarlierTblock))
		.chain(
			[Stage::Accepted, Stage::WellFormed, Stage::DryRun, Stage::Apply]
				.into_iter()
				.flat_map(|stage| {
					[Class::Result(stage), Class::ShieldedResult(stage), Class::CallResult(stage)]
				}),
		)
		.chain(
			[(OnChain, OnChain), (OnChain, OffChain), (OffChain, OnChain), (OffChain, OffChain)]
				.map(|(call, receipt)| Class::Storage(call, receipt)),
		)
		.chain(
			[
				OutcomeRow::NoReceipt,
				OutcomeRow::SameStateSameTblock,
				OutcomeRow::SameStateOtherTblock,
				OutcomeRow::OtherStateLoadable,
				OutcomeRow::NotLoadable,
			]
			.map(Class::Outcome),
		)
		.chain(
			[
				ReceiptRow::Passed,
				ReceiptRow::FailedLoadable,
				ReceiptRow::FailedNotLoadable,
				ReceiptRow::FailedNoReceipt,
				ReceiptRow::NeverReached,
			]
			.map(Class::ReceiptAfter),
		)
		.chain([Class::SpentNullifier, Class::UnknownMerkleRoot, Class::WrongKeyRejected])
		.chain([1, 2].map(Class::OtherVersionOnly))
		.chain([Lost::Evicted, Lost::Orphaned].map(Class::PassedAfter))
		.chain(
			[
				ProofCase::StrictHit,
				ProofCase::Miss,
				ProofCase::Skipped,
				ProofCase::ContractReverified,
				ProofCase::DustReverified,
			]
			.map(Class::ProofCount),
		)
		// `retry_at_the_block_tblock` reaches the passing retries.
		.chain(shim::RETRY_AT_BLOCK_TBLOCK.then_some(Class::Retried(false)));
	let missing: Vec<_> = every.filter(|class| !classes.contains_key(class)).collect();
	assert!(missing.is_empty(), "never reached {missing:?}; reached {classes:#?}");
}

/// P9a only says something if the warm world loads states the cold one doesn't, and is rejected
/// or applies on a hit.
fn disk_parity_reaches_hits() {
	use disk::Seen;
	let mut runner = proptest::test_runner::TestRunner::deterministic();
	let strategy = (any::<Index>(), disk::steps());
	let trees = smoke_trees();
	let mut seen = BTreeMap::new();
	for _ in 0..16 {
		let _worlds = disk::worlds();
		let (tree, steps) = strategy.new_tree(&mut runner).unwrap().current();
		disk::check_disk_parity(&trees[tree.index(trees.len())], &steps, Some(&mut seen)).unwrap();
	}
	println!("{seen:#?}");
	let missing: Vec<_> =
		[Seen::OnChainHitFromOtherState, Seen::OnChainRejectedOnHit, Seen::AppliedOnHit]
			.into_iter()
			.filter(|s| !seen.contains_key(s))
			.collect();
	assert!(missing.is_empty(), "never reached {missing:?}");
}

/// The threads in [`receipts_stay_true_across_threads`] do race: calls find receipts other
/// threads wrote, and run alongside calls for the same key. Interleavings vary between runs; the
/// inputs don't.
fn threads_collide() {
	let mut runner = proptest::test_runner::TestRunner::deterministic();
	let trees = smoke_trees();
	let mut total = threads::Collisions::default();
	for _ in 0..8 {
		let case = threads::case().new_tree(&mut runner).unwrap().current();
		let (tree, histories) = threads::restrict(&trees[case.tree.index(trees.len())], &case);
		let seen = threads::check_threads(&tree, case.capacity, &histories).unwrap();
		println!("{seen:?}");
		total.calls += seen.calls;
		total.foreign += seen.foreign;
		total.overlap += seen.overlap;
		total.passes += seen.passes;
	}
	println!("total {total:?}");
	assert!(total.foreign > 0, "no call found another thread's receipt");
	assert!(total.overlap > 0, "no two calls for one key ran at once");
}

suite_tests!(
	warm_cache_gives_the_cold_verdict,
	offchain_receipt_serves_onchain,
	receipt_on_a_gone_inline_root_misses,
	retry_at_the_block_tblock,
	warm_node_flushes_what_a_cold_one_does,
	receipts_stay_true_across_threads,
	dummy_proofs_pass_only_unverified,
	generator_reaches_every_class,
	disk_parity_reaches_hits,
	threads_collide,
);

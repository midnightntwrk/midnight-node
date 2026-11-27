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

//! Several histories on their own threads against one cache, as pool validation and block import
//! share one, for P10 and P1.

use super::ledger::ledger_storage_local::DefaultDB;
use super::{
	Action, Exts, ProofReceipt, TX_VALIDATION_CACHE_CAPACITY, TxValidationCache, action, cache_key,
	capacity, missing_state, model, peek, properties, run, target, tree::Tree,
};
use proptest::{prelude::*, sample::Index};
use std::{
	collections::{HashMap, HashSet},
	sync::{
		Barrier,
		atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
	},
	thread,
};

const THREADS: usize = 4;
const STEPS: usize = 16;

/// How often the threads met on a key.
#[derive(Debug, Default, Clone, Copy)]
pub struct Collisions {
	pub calls: usize,
	/// Calls that found a receipt only another thread could have written.
	pub foreign: usize,
	/// Calls that started while another thread was inside a call for the same key.
	pub overlap: usize,
	/// Full passes the checker thread made over the cache.
	pub passes: usize,
}

/// A case: a few of the tree's txs, every action on them, and at most two runtime versions.
#[derive(Debug, Clone)]
pub struct Case {
	pub tree: Index,
	pub capacity: u64,
	txs: Vec<Index>,
	/// `None` for both versions.
	version: Option<u32>,
	histories: Vec<Vec<Action>>,
}

/// Few keys, so threads race on the same receipts.
pub fn case() -> impl Strategy<Value = Case> {
	(
		any::<Index>(),
		capacity(),
		prop::collection::vec(any::<Index>(), 1..=3),
		prop::option::of(1u32..=2),
		prop::collection::vec(prop::collection::vec(action(), STEPS), THREADS),
	)
		.prop_map(|(tree, capacity, txs, version, histories)| Case {
			tree,
			capacity,
			txs,
			version,
			histories,
		})
}

/// The tree with only the case's txs, and its histories held to the case's version. Its nodes'
/// edges still index the full tree's txs, so it can't be replayed.
pub fn restrict(tree: &Tree, case: &Case) -> (Tree, Vec<Vec<Action>>) {
	let mut picked: Vec<_> = case.txs.iter().map(|i| i.index(tree.txs.len())).collect();
	picked.sort();
	picked.dedup();
	let tree = Tree {
		nodes: tree.nodes.clone(),
		txs: picked.into_iter().map(|i| tree.txs[i].clone()).collect(),
	};
	let mut histories = case.histories.clone();
	if let Some(v) = case.version {
		for action in histories.iter_mut().flatten() {
			match action {
				Action::Call(step) => step.runtime_version = v,
				Action::Evict { runtime_version, .. } | Action::Orphan { runtime_version, .. } => {
					*runtime_version = v
				},
				Action::Again { .. } => {},
			}
		}
	}
	(tree, histories)
}

/// Runs each history on its own thread against one shared cache, and checks:
/// - every call: [`properties::same_result`];
/// - every action: [`properties::receipt_is_true`] for the receipt it touched, and the same for
///   every receipt in the cache, on a checker thread that keeps passing over it until the last
///   history ends, then once more.
///
/// Receipts `Orphan` plants are false by construction. Their states are `missing_state` of a
/// tree node, so they are told apart without the threads sharing anything.
pub fn check_threads(
	tree: &Tree,
	capacity: u64,
	histories: &[Vec<Action>],
) -> Result<Collisions, TestCaseError> {
	let warm = TxValidationCache::new(capacity);
	let synthetic: HashSet<_> = tree.nodes.iter().map(|n| missing_state(&n.state_key)).collect();
	let tx_of: HashMap<_, _> = tree.txs.iter().enumerate().map(|(i, tx)| (tx.hash, i)).collect();
	let in_flight: HashMap<_, _> = (0..tree.txs.len())
		.flat_map(|tx| [1, 2].map(|version| cache_key(tree, tx, version)))
		.map(|key| (key, AtomicUsize::new(0)))
		.collect();
	let barrier = Barrier::new(histories.len() + 1);
	let done = AtomicBool::new(false);

	let worker = |actions: &[Action]| -> Result<Collisions, TestCaseError> {
		let mut exts = Exts::new();
		let mut seen = Collisions::default();
		let mut mine = HashSet::new();
		barrier.wait();
		for action in actions {
			let (tx, key) = match action {
				Action::Call(step) => {
					let (tx, node, block_context) = target(tree, step);
					let key = cache_key(tree, tx, step.runtime_version);
					let node = &tree.nodes[node];
					let tblocks = model::tblocks(&step.call, node.block_start, &block_context);
					if peek(&warm, &key).is_some_and(|r| {
						!synthetic.contains(&r.0) && !mine.contains(&(key.clone(), r))
					}) {
						seen.foreign += 1;
					}
					let slot = &in_flight[&key];
					if slot.fetch_add(1, SeqCst) > 0 {
						seen.overlap += 1;
					}
					let warm_result = run::<DefaultDB>(tree, step, &mut exts, &warm);
					slot.fetch_sub(1, SeqCst);
					seen.calls += 1;
					for tblock in tblocks {
						mine.insert((key.clone(), (node.state_key.clone(), tblock)));
					}

					let cold = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
					let cold_result = run::<DefaultDB>(tree, step, &mut exts, &cold);
					prop_assert_eq!(&warm_result, &cold_result, "{:?}", step);
					(tx, key)
				},
				Action::Evict { tx, runtime_version } => {
					let tx = tx.index(tree.txs.len());
					let key = cache_key(tree, tx, *runtime_version);
					warm.0.invalidate(&key);
					(tx, key)
				},
				Action::Orphan { tx, runtime_version } => {
					let tx = tx.index(tree.txs.len());
					let key = cache_key(tree, tx, *runtime_version);
					if let Some((state_key, tblock)) =
						peek(&warm, &key).filter(|(state, _)| !synthetic.contains(state))
					{
						let state_key = missing_state(&state_key);
						warm.0.insert(key.clone(), ProofReceipt { state_key, tblock });
					}
					(tx, key)
				},
				Action::Again { .. } => unreachable!("only `history` repeats calls"),
			};
			properties::receipt_is_true(tree, tx, peek(&warm, &key), &synthetic)?;
		}
		Ok(seen)
	};

	let checker = || -> Result<usize, TestCaseError> {
		barrier.wait();
		let mut passes = 0;
		loop {
			let last = done.load(SeqCst);
			for (key, receipt) in warm.0.iter() {
				let receipt = Some((receipt.state_key, receipt.tblock));
				properties::receipt_is_true(tree, tx_of[&key.tx_hash], receipt, &synthetic)?;
			}
			passes += 1;
			if last {
				return Ok(passes);
			}
		}
	};

	thread::scope(|s| {
		let checker = s.spawn(checker);
		let workers: Vec<_> = histories.iter().map(|h| s.spawn(|| worker(h))).collect();
		let results: Vec<_> = workers.into_iter().map(|w| w.join()).collect();
		done.store(true, SeqCst);
		let passes = checker.join().unwrap_or_else(|p| std::panic::resume_unwind(p))?;
		let mut total = Collisions { passes, ..Collisions::default() };
		for result in results {
			let seen = result.unwrap_or_else(|p| std::panic::resume_unwind(p))?;
			total.calls += seen.calls;
			total.foreign += seen.foreign;
			total.overlap += seen.overlap;
		}
		Ok(total)
	})
}

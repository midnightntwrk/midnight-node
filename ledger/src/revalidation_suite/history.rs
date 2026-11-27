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

//! Runs a history of calls against one warm cache, checks the per-call properties after each, and
//! counts what the calls met, for coverage.

use super::ledger::{
	ProofReceipt, TX_VALIDATION_CACHE_CAPACITY, TxValidationCache, TxValidationCacheOutcome,
	ledger_storage_local::DefaultDB,
	types::{InvalidError, LedgerApiError, TransactionError, ZswapInvalidErrorCode},
};
use super::{
	Action, At, Call, Exts, Lost, Step, Storage, action, cache_key, call, is_ancestor,
	missing_state,
	model::{self, ModelCall, Receipt},
	peek, predict,
	proofs::{self, Counts},
	properties, run, storage, target,
	tree::{Tree, TxKind},
};
use proptest::{prelude::*, sample::Index};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Where a receipt's state sits relative to the call's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
	Absent,
	Own,
	Ancestor,
	Descendant,
	Sibling,
	/// Orphaned: its state is in no storage.
	Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
	Accepted,
	WellFormed,
	DryRun,
	Apply,
}

/// What a warm call that reached the cache did to the proofs, for P8e.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProofCase {
	StrictHit,
	/// A call that passed on its first attempt, checked against the tx's proofs.
	Miss,
	/// A hit that verified fewer proofs than the cold call.
	Skipped,
	/// A hit that verified a contract proof again.
	ContractReverified,
	/// A hit that verified a dust spend again.
	DustReverified,
}

/// What a warm call met, judged by the receipt it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
	Receipt(Relation),
	/// Whether the call's effective tblock is earlier than the receipt's.
	EarlierTblock(bool),
	Result(Stage),
	/// The result of a call sending a shielded tx.
	ShieldedResult(Stage),
	/// A shielded tx rejected because its coin was spent on the way to the call's state.
	SpentNullifier,
	/// A shielded tx rejected at a state that never recorded its Merkle root.
	UnknownMerkleRoot,
	/// The result of a call sending a `store` call.
	CallResult(Stage),
	/// The wrong-key call sent, and rejected.
	WrongKeyRejected,
	/// The call's storage, then that of the call that wrote the receipt.
	Storage(Storage, Storage),
	Outcome(model::OutcomeRow),
	ReceiptAfter(model::ReceiptRow),
	/// The call's runtime version has no receipt, the other one has. Holds the call's.
	OtherVersionOnly(u32),
	/// A call that passed `well_formed` straight after its receipt was lost.
	PassedAfter(Lost),
	ProofCount(ProofCase),
	/// A call retried at the block's tblock, and whether the retry passed `well_formed`.
	Retried(bool),
}

pub type Classes = BTreeMap<Class, usize>;

/// An action with its targets resolved against the tree and the calls made so far.
enum Op {
	Call(Step),
	/// The tx and runtime version of the receipt lost.
	Lose(Lost, usize, u32),
}

fn relation(tree: &Tree, node: usize, receipt_state: Option<&[u8]>) -> Relation {
	let Some(state) = receipt_state else { return Relation::Absent };
	if tree.nodes[node].state_key == state {
		return Relation::Own;
	}
	match tree.nodes.iter().position(|n| n.state_key == state) {
		None => Relation::Gone,
		Some(r) if is_ancestor(tree, r, node) => Relation::Ancestor,
		Some(r) if is_ancestor(tree, node, r) => Relation::Descendant,
		Some(_) => Relation::Sibling,
	}
}

fn stage(call: &Call, result: &Result<Vec<u8>, LedgerApiError>) -> Stage {
	match (result, call) {
		(Ok(_), _) => Stage::Accepted,
		(Err(LedgerApiError::Transaction(TransactionError::Malformed(_))), _) => Stage::WellFormed,
		(Err(_), Call::Validate { .. }) => Stage::DryRun,
		(Err(_), Call::Apply { .. }) => Stage::Apply,
	}
}

/// The P8e cases a call reached.
fn proof_cases(
	call: &ModelCall,
	before: Option<&Receipt>,
	loadable: bool,
	warm: Counts,
	cold: Counts,
) -> Vec<ProofCase> {
	use TxValidationCacheOutcome::*;
	if !call.reaches_cache {
		return vec![];
	}
	let mut cases = vec![];
	if call.attempts.len() == 1 && call.passes_well_formed {
		cases.push(ProofCase::Miss);
	}
	let attempts = model::attempts(before, loadable, call);
	if attempts.iter().all(|a| *a == StrictCacheHit) {
		cases.push(ProofCase::StrictHit);
	} else if attempts.iter().all(|a| *a != CacheMiss) {
		if warm != cold {
			cases.push(ProofCase::Skipped);
		}
		if warm.contract > 0 {
			cases.push(ProofCase::ContractReverified);
		}
		if warm.dust > 0 {
			cases.push(ProofCase::DustReverified);
		}
	}
	cases
}

/// Runs `actions` against one warm cache of `capacity`, and after each checks:
/// - every call: [`properties::same_result`], [`properties::wrong_key_is_rejected`];
/// - every action: [`properties::receipt_is_true`] for the receipt it touched;
/// - every call, at production capacity: the P8a tables and [`properties::proofs_verified`].
///   Below it, moka may evict or refuse a receipt on its own, so the tables can't predict it.
pub fn check_history(
	tree: &Tree,
	capacity: u64,
	actions: &[Action],
	classes: &mut Classes,
) -> Result<(), TestCaseError> {
	let tables = capacity == TX_VALIDATION_CACHE_CAPACITY;
	let mut exts = Exts::new();
	let warm = TxValidationCache::new(capacity);
	let mut writers = HashMap::new();
	let mut orphans = HashSet::new();
	let mut lost = HashMap::new();
	let mut receipted: BTreeMap<TxKind, Vec<Step>> = BTreeMap::new();
	for action in actions {
		let ops = match action {
			Action::Call(step) => vec![Op::Call(step.clone())],
			Action::Evict { tx, runtime_version } => {
				vec![Op::Lose(Lost::Evicted, tx.index(tree.txs.len()), *runtime_version)]
			},
			Action::Orphan { tx, runtime_version } => {
				vec![Op::Lose(Lost::Orphaned, tx.index(tree.txs.len()), *runtime_version)]
			},
			Action::Again { .. } if receipted.is_empty() => vec![],
			Action::Again { kind, earlier, node, call, other_version, lose } => {
				let steps = receipted.values().nth(kind.index(receipted.len())).unwrap();
				let mut step = steps[earlier.index(steps.len())].clone();
				step.call = call.clone();
				if *other_version {
					step.runtime_version = 3 - step.runtime_version;
				}
				step.node = node.clone().unwrap_or(step.node);
				let tx = target(tree, &step).0;
				let lose = lose.map(|lost| Op::Lose(lost, tx, step.runtime_version));
				lose.into_iter().chain([Op::Call(step)]).collect()
			},
		};
		for op in ops {
			let (tx, key) = match op {
				Op::Call(step) => {
					let step = &step;
					let (tx, node, block_context) = target(tree, step);
					let key = cache_key(tree, tx, step.runtime_version);
					let before = peek(&warm, &key);
					let rel = relation(tree, node, before.as_ref().map(|(s, _)| &s[..]));
					let loadable = rel != Relation::Gone;
					let mut count = |class| *classes.entry(class).or_default() += 1;

					let model = tables.then(|| ModelCall::new(tree, step));
					if let Some(call) = model.as_ref().filter(|call| call.reaches_cache) {
						let actual = predict(
							&mut exts,
							storage(&step.call),
							&call.state_key,
							call.tblock,
							&key,
							before.as_ref(),
						);
						let row = properties::outcome_follows_table(
							step,
							call,
							before.as_ref(),
							loadable,
							&actual,
						)?;
						count(Class::Outcome(row));
						let other = cache_key(tree, tx, 3 - step.runtime_version);
						if before.is_none() && peek(&warm, &other).is_some() {
							count(Class::OtherVersionOnly(step.runtime_version));
						}
					}

					let ((warm_result, moved), warm_proofs) = proofs::counting(|| {
						exts.outcome_moved(|exts| run::<DefaultDB>(tree, step, exts, &warm))
					});
					let cold = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
					let (cold_result, cold_proofs) =
						proofs::counting(|| run::<DefaultDB>(tree, step, &mut exts, &cold));
					properties::same_result(step, &warm_result, &cold_result)?;

					let after = peek(&warm, &key);
					if let Some(call) = &model {
						let (before, after) = (before.as_ref(), after.as_ref());
						properties::metric_records_outcome(
							step,
							call,
							before,
							loadable,
							warm_result.is_ok(),
							moved,
						)?;
						let row =
							properties::receipt_follows_table(step, call, before, loadable, after)?;
						count(Class::ReceiptAfter(row));
						properties::proofs_verified(
							tree,
							tx,
							call,
							before,
							loadable,
							warm_proofs,
							cold_proofs,
						)?;
						for case in proof_cases(call, before, loadable, warm_proofs, cold_proofs) {
							count(Class::ProofCount(case));
						}
						if call.reaches_cache && call.attempts.len() > 1 {
							count(Class::Retried(call.passes_well_formed));
						}
						if call.reaches_cache
							&& let Some(lost) = lost.remove(&key)
							&& row == model::ReceiptRow::Passed
						{
							count(Class::PassedAfter(lost));
						}
					}

					count(Class::Receipt(rel));
					let stage = stage(&step.call, &warm_result);
					count(Class::Result(stage));
					match tree.txs[tx].kind {
						TxKind::Shielded => count(Class::ShieldedResult(stage)),
						TxKind::Store => count(Class::CallResult(stage)),
						TxKind::WrongKey => {
							properties::wrong_key_is_rejected(step, &warm_result)?;
							count(Class::WrongKeyRejected);
						},
						_ => {},
					}
					if let Err(LedgerApiError::Transaction(TransactionError::Invalid(
						InvalidError::Zswap(code),
					))) = &warm_result
					{
						match code {
							ZswapInvalidErrorCode::NullifierAlreadyPresent => {
								count(Class::SpentNullifier)
							},
							ZswapInvalidErrorCode::UnknownMerkleRoot => {
								count(Class::UnknownMerkleRoot)
							},
							_ => {},
						}
					}
					if let Some((_, tblock)) = &before {
						let block_start = tree.nodes[node].block_start;
						let effective =
							model::effective_tblock(&step.call, block_start, &block_context);
						count(Class::EarlierTblock(effective < *tblock));
						if let Some(&writer) = writers.get(&key) {
							count(Class::Storage(storage(&step.call), writer));
						}
					}
					if after.is_some() && after != before {
						writers.insert(key.clone(), storage(&step.call));
						receipted.entry(tree.txs[tx].kind).or_default().push(step.clone());
					}
					(tx, key)
				},
				Op::Lose(Lost::Evicted, tx, runtime_version) => {
					let key = cache_key(tree, tx, runtime_version);
					if peek(&warm, &key).is_some() {
						lost.insert(key.clone(), Lost::Evicted);
					}
					warm.0.invalidate(&key);
					(tx, key)
				},
				Op::Lose(Lost::Orphaned, tx, runtime_version) => {
					let key = cache_key(tree, tx, runtime_version);
					if let Some((state_key, tblock)) =
						peek(&warm, &key).filter(|(state, _)| !orphans.contains(state))
					{
						let state_key = missing_state(&state_key);
						orphans.insert(state_key.clone());
						warm.0.insert(key.clone(), ProofReceipt { state_key, tblock });
						lost.insert(key.clone(), Lost::Orphaned);
					}
					(tx, key)
				},
			};
			properties::receipt_is_true(tree, tx, peek(&warm, &key), &orphans)?;
		}
	}
	Ok(())
}

fn again() -> impl Strategy<Value = Action> {
	let lose = prop::option::weighted(0.25, prop_oneof![Just(Lost::Evicted), Just(Lost::Orphaned)]);
	let node = prop_oneof![
		1 => Just(None),
		1 => any::<Index>().prop_map(|n| Some(At::Below(n))),
		2 => any::<Index>().prop_map(|n| Some(At::Across(n))),
	];
	(any::<[Index; 2]>(), node, call(), prop::bool::weighted(0.2), lose).prop_map(
		|([kind, earlier], node, call, other_version, lose)| Action::Again {
			kind,
			earlier,
			node,
			call,
			other_version,
			lose,
		},
	)
}

pub fn history(max_len: usize) -> impl Strategy<Value = Vec<Action>> {
	prop::collection::vec(prop_oneof![1 => action(), 2 => again()], 1..max_len)
}

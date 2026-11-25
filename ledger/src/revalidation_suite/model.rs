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

//! P8a's two tables as plain functions of the receipt and the call, with their inputs computed
//! from the tree and the ledger, never from the cache.

use super::ledger::{
	BlockContext, TBLOCK_CORRECTION_OFFSET_SECS, Timestamp, TxValidationCacheOutcome, api,
	ensure_within_tx_byte_limit, ledger_storage_local::DefaultDB, types::LedgerApiError,
};
use super::{Call, Step, shim, target, tree::Tree, tree::TreeBridge};

/// A receipt as `peek` reads it: the state key and tblock.
pub type Receipt = (Vec<u8>, Timestamp);

/// What the tables need to know about a call.
pub struct ModelCall {
	pub state_key: Vec<u8>,
	/// The tblock of the first attempt, which `lookup_receipt` is asked with first.
	pub tblock: Timestamp,
	pub reaches_cache: bool,
	/// At some attempt's tblock.
	pub passes_well_formed: bool,
	/// The tblocks the call validates at, up to the first `well_formed` passes at.
	pub attempts: Vec<Timestamp>,
}

impl ModelCall {
	pub fn new(tree: &Tree, step: &Step) -> Self {
		let (tx, node, block_context) = target(tree, step);
		let node = &tree.nodes[node];
		let ledger =
			TreeBridge::load_ledger(&api::new(), &node.state_key).expect("tree states stay stored");
		// Every tree state loads and every tree tx deserializes, so only the validate paths'
		// byte-limit pre-check can stop a call short of the cache. `apply_transaction` has none.
		let reaches_cache = match step.call {
			Call::Apply { .. } => true,
			Call::Validate { .. } => ensure_within_tx_byte_limit(
				ledger.state.parameters.limits.transaction_byte_limit,
				&tree.txs[tx].bytes,
			)
			.is_ok(),
		};
		let mut attempts = vec![];
		let mut passes_well_formed = false;
		for tblock in tblocks(&step.call, node.block_start, &block_context) {
			attempts.push(tblock);
			passes_well_formed = well_formed(tree, tx, &ledger.state, tblock).is_ok();
			if passes_well_formed {
				break;
			}
		}
		Self {
			state_key: node.state_key.clone(),
			tblock: attempts[0],
			reaches_cache,
			passes_well_formed,
			attempts,
		}
	}
}

/// Every tblock a call may validate at, in order: ledger 8 retries a call `well_formed` rejects
/// at the corrected tblock at the block's own.
pub fn tblocks(call: &Call, block_start: bool, block_context: &BlockContext) -> Vec<Timestamp> {
	let effective = effective_tblock(call, block_start, block_context);
	let block = Timestamp::from_secs(block_context.tblock);
	let retry = shim::RETRY_AT_BLOCK_TBLOCK && effective != block;
	std::iter::once(effective).chain(retry.then_some(block)).collect()
}

/// The timestamp `well_formed` sees. The mempool never skews.
pub fn effective_tblock(call: &Call, block_start: bool, block_context: &BlockContext) -> Timestamp {
	let skew = match *call {
		Call::Validate { pre_dispatch, skew_tblock, .. } => pre_dispatch && skew_tblock,
		Call::Apply { skew_tblock, .. } => skew_tblock,
	};
	match block_context.parent_block_time() {
		Some(parent) if skew && block_start => {
			Timestamp::from_secs(parent + TBLOCK_CORRECTION_OFFSET_SECS as u64)
		},
		_ => Timestamp::from_secs(block_context.tblock),
	}
}

/// `well_formed` in full, against the state production passes on a miss.
pub fn well_formed(
	tree: &Tree,
	tx: usize,
	state: &super::ledger::LedgerState<DefaultDB>,
	tblock: Timestamp,
) -> Result<(), LedgerApiError> {
	let parsed = api::new().tagged_deserialize(&tree.txs[tx].bytes).unwrap();
	TreeBridge::is_well_formed(&parsed, state, tblock).map(drop)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutcomeRow {
	NoReceipt,
	SameStateSameTblock,
	SameStateOtherTblock,
	OtherStateLoadable,
	NotLoadable,
}

/// P8a's outcome table, for a call that reaches the cache.
pub fn outcome(
	receipt: Option<&Receipt>,
	loadable: bool,
	call: &ModelCall,
) -> (OutcomeRow, TxValidationCacheOutcome) {
	outcome_at(receipt, loadable, &call.state_key, call.tblock)
}

fn outcome_at(
	receipt: Option<&Receipt>,
	loadable: bool,
	state_key: &[u8],
	call_tblock: Timestamp,
) -> (OutcomeRow, TxValidationCacheOutcome) {
	use OutcomeRow::*;
	use TxValidationCacheOutcome::*;
	match receipt {
		None => (NoReceipt, CacheMiss),
		Some((state, tblock)) if *state == state_key && *tblock == call_tblock => {
			(SameStateSameTblock, StrictCacheHit)
		},
		Some((state, _)) if *state == state_key => (SameStateOtherTblock, RevalidationHit),
		Some(_) if loadable => (OtherStateLoadable, RevalidationHit),
		Some(_) => (NotLoadable, CacheMiss),
	}
}

/// The outcome table applied to each attempt in turn. An attempt only follows one that failed
/// `well_formed`, which left the receipt as it was, or evicted it if its state was gone.
pub fn attempts(
	receipt: Option<&Receipt>,
	loadable: bool,
	call: &ModelCall,
) -> Vec<TxValidationCacheOutcome> {
	let mut receipt = receipt.cloned();
	let mut outcomes = vec![];
	for tblock in &call.attempts {
		outcomes.push(outcome_at(receipt.as_ref(), loadable, &call.state_key, *tblock).1);
		receipt = receipt.filter(|_| loadable);
	}
	outcomes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReceiptRow {
	Passed,
	FailedLoadable,
	FailedNotLoadable,
	FailedNoReceipt,
	NeverReached,
}

/// P8a's receipts-after table: the receipt for the call's key once the call has run.
pub fn receipt_after(
	before: Option<&Receipt>,
	loadable: bool,
	call: &ModelCall,
) -> (ReceiptRow, Option<Receipt>) {
	use ReceiptRow::*;
	match before {
		_ if !call.reaches_cache => (NeverReached, before.cloned()),
		_ if call.passes_well_formed => {
			(Passed, Some((call.state_key.clone(), *call.attempts.last().unwrap())))
		},
		Some(receipt) if loadable => (FailedLoadable, Some(receipt.clone())),
		Some(_) => (FailedNotLoadable, None),
		None => (FailedNoReceipt, None),
	}
}

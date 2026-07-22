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

//! Per-ledger-version replay-check driver: replays blocks through a
//! [`LedgerContext`] and runs a set of [`Predicate`]s against every applied
//! block, its transactions, and the pre/post ledger states.
//!
//! Compiled once per ledger generation via the `fork` `#[path = "common"]`
//! mechanism, so all types here bind to `ledger_helpers_local`
//! (= `ledger_7` / `ledger_8` / `ledger_9`).
//!
//! # Adding a predicate
//!
//! **Version-agnostic** (expressible in every ledger generation): implement
//! [`Predicate`] in this file, next to the examples at the bottom. Because
//! this module is compiled once per generation, the implementation must
//! build against *all* of `ledger_7`/`ledger_8`/`ledger_9` — stick to APIs
//! that exist in every generation (the pattern used by the sibling `common`
//! modules like `night_pools`). Then register it in the `predicates()`
//! registry of every generation that should run it
//! (`commands/fork/ledger_{7,8,9}.rs`).
//!
//! **Version-specific** (uses APIs that only exist in one generation — new
//! `LedgerState` fields, changed `Transaction` variants, etc.): do *not*
//! define it in this file, since it would fail to compile under the other
//! generations' bindings. Instead define it in that generation's wrapper
//! file, e.g. `commands/fork/ledger_8.rs`, *outside* the `inner` block.
//! There you can import `midnight_node_ledger_helpers::ledger_8` directly;
//! the types line up because `inner::ledger_helpers_local` *is*
//! `midnight_node_ledger_helpers::ledger_8`, so `replay_check::Predicate`
//! in that file is exactly this trait as instantiated for ledger 8:
//!
//! ```ignore
//! // In commands/fork/ledger_8.rs, below `pub use inner::*;`:
//! use crate::commands::replay_check::Violation;
//! use midnight_node_ledger_helpers::ledger_8 as lh8;
//!
//! /// Only meaningful/compilable on ledger 8.
//! struct ZeroValueRewardsClaim;
//!
//! impl replay_check::Predicate for ZeroValueRewardsClaim {
//! 	fn name(&self) -> &'static str {
//! 		"zero-value-rewards-claim"
//! 	}
//!
//! 	fn observe_block(
//! 		&self,
//! 		obs: &replay_check::BlockObservation<'_>,
//! 		out: &mut Vec<Violation>,
//! 	) {
//! 		// `obs.txs` are ledger-8 `SerdeTransaction`s, `obs.pre_state` /
//! 		// `obs.post_state` are ledger-8 `LedgerState`s: the full ledger-8
//! 		// API is available here, including anything absent from v7/v9.
//! 		for (tx_index, tx) in obs.txs.iter().enumerate() {
//! 			if let lh8::SerdeTransaction::Midnight(lh8::Transaction::ClaimRewards(claim)) = tx
//! 				&& claim.value == 0
//! 			{
//! 				out.push(Violation::tx_level(
//! 					self.name(),
//! 					obs.block,
//! 					tx_index,
//! 					tx.transaction_hash().0.0,
//! 					"zero-value rewards claim",
//! 				));
//! 			}
//! 		}
//! 	}
//! }
//! ```
//!
//! Finally add it to that file's `predicates()` registry — `--predicate`
//! filtering and `--list-predicates` pick it up from there automatically.
//! Blocks of other generations are simply never shown to it: the
//! `replay-check` driver dispatches each block run to the registry of the
//! generation that produced it.

use midnight_node_ledger_helpers::fork::raw_block_data::{RawBlockData, RawTransaction};

use super::ledger_helpers_local::{
	self, DefaultDB, Event, HashOutput, LedgerState, ProofMarker, PureGeneratorPedersen, Signature,
	SystemTransaction, Timestamp, context::LedgerContext, midnight_serialize::tagged_deserialize,
};
use crate::commands::replay_check::Violation;
use crate::progress::Progress;

type MnTx =
	ledger_helpers_local::Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;
pub type SerdeTx = ledger_helpers_local::SerdeTransaction<Signature, ProofMarker, DefaultDB>;

/// Everything a predicate gets to look at for one applied block.
pub struct BlockObservation<'a> {
	pub block: &'a RawBlockData,
	pub block_context: &'a ledger_helpers_local::BlockContext,
	/// The block's transactions, decoded once and shared by all predicates.
	pub txs: &'a [SerdeTx],
	/// Ledger state before the block was applied.
	pub pre_state: &'a LedgerState<DefaultDB>,
	/// Ledger state after the block was applied (incl. `post_block_update`).
	pub post_state: &'a LedgerState<DefaultDB>,
	/// Events emitted while applying the block.
	pub events: &'a [Event<DefaultDB>],
}

/// A scannable invariant over blocks, their transactions, and ledger state.
///
/// Implementations are registered per ledger generation in
/// `commands/fork/ledger_{7,8,9}.rs::predicates()`.
pub trait Predicate: Send + Sync {
	/// Stable name, used for `--predicate` filtering and violation reports.
	fn name(&self) -> &'static str;
	/// One-line human description for `--list-predicates`.
	fn description(&self) -> &'static str {
		""
	}
	/// Called once per applied block; iterate `obs.txs` to test transactions.
	fn observe_block(&self, obs: &BlockObservation<'_>, out: &mut Vec<Violation>);
}

/// Bookkeeping returned by [`observe_blocks`].
pub struct ObserveOutcome {
	/// Blocks applied to the ledger state (replayed).
	pub blocks_applied: u64,
	/// Blocks the predicates actually observed (within `--from-block` bounds).
	pub blocks_observed: u64,
	/// True when `fail_fast` stopped the run at the first violating block.
	pub aborted: bool,
}

/// Decode a block's raw transactions into this ledger generation's types.
/// Mirrors `apply_block_N` in `ledger/helpers/src/fork/fork_aware_context.rs`,
/// but surfaces decode failures as errors instead of panicking.
pub fn decode_block_txs(
	block: &RawBlockData,
) -> Result<Vec<SerdeTx>, Box<dyn std::error::Error + Send + Sync>> {
	let mut transactions: Vec<SerdeTx> = Vec::with_capacity(block.transactions.len());
	for (i, raw_tx) in block.transactions.iter().enumerate() {
		match raw_tx {
			RawTransaction::Midnight(bytes) => {
				let tx: MnTx = tagged_deserialize(&mut bytes.as_slice()).map_err(|e| {
					format!(
						"block {} tx {i}: failed to deserialize midnight transaction: {e}",
						block.number
					)
				})?;
				transactions.push(SerdeTx::Midnight(tx));
			},
			RawTransaction::System(bytes) => {
				let tx: SystemTransaction =
					tagged_deserialize(&mut bytes.as_slice()).map_err(|e| {
						format!(
							"block {} tx {i}: failed to deserialize system transaction: {e}",
							block.number
						)
					})?;
				transactions.push(SerdeTx::System(tx));
			},
		}
	}
	Ok(transactions)
}

fn block_context_from_raw(block: &RawBlockData) -> ledger_helpers_local::BlockContext {
	ledger_helpers_local::make_block_context(
		Timestamp::from_secs(block.tblock_secs),
		HashOutput(block.parent_block_hash),
		Timestamp::from_secs(block.last_block_time_secs),
	)
}

fn snapshot_state(ctx: &LedgerContext<DefaultDB>) -> LedgerState<DefaultDB> {
	(**ctx.ledger_state.lock().expect("ledger_state mutex poisoned")).clone()
}

/// Replay `blocks` through `ctx` and run every predicate on each applied block.
///
/// Every block is applied faithfully via `LedgerContext::update_from_block`
/// (which also verifies state roots); predicates only observe blocks with
/// `number >= from_block`. Violations are appended to `violations`; with
/// `fail_fast` the run stops after the first block that produced any.
pub fn observe_blocks(
	ctx: &LedgerContext<DefaultDB>,
	blocks: &[RawBlockData],
	predicates: &[Box<dyn Predicate>],
	from_block: Option<u64>,
	fail_fast: bool,
	progress: Option<&Progress>,
	violations: &mut Vec<Violation>,
) -> Result<ObserveOutcome, Box<dyn std::error::Error + Send + Sync>> {
	let mut outcome = ObserveOutcome { blocks_applied: 0, blocks_observed: 0, aborted: false };

	for block in blocks {
		let txs = decode_block_txs(block)?;
		let block_context = block_context_from_raw(block);

		let observe = !predicates.is_empty() && from_block.is_none_or(|from| block.number >= from);
		// Cheap: `LedgerState` is a persistent structure, cloning is O(1)-ish.
		let pre_state = observe.then(|| snapshot_state(ctx));

		let events = ctx
			.update_from_block(
				&txs,
				&block_context,
				block.state_root.as_ref(),
				block.state.as_ref(),
			)
			.map_err(|e| format!("failed to apply block {}: {e}", block.number))?;
		outcome.blocks_applied += 1;
		if let Some(progress) = progress {
			progress.inc(1);
		}

		if let Some(pre_state) = pre_state {
			let post_state = snapshot_state(ctx);
			let obs = BlockObservation {
				block,
				block_context: &block_context,
				txs: &txs,
				pre_state: &pre_state,
				post_state: &post_state,
				events: &events,
			};
			let before = violations.len();
			for predicate in predicates {
				predicate.observe_block(&obs, violations);
			}
			outcome.blocks_observed += 1;
			if fail_fast && violations.len() > before {
				outcome.aborted = true;
				return Ok(outcome);
			}
		}
	}

	Ok(outcome)
}

// --- Example predicates ---------------------------------------------------
//
// These demonstrate the two observation capabilities (block-level state
// deltas and per-transaction structure). They are genuine invariants that
// should never fire on healthy chain history; real vulnerability predicates
// land alongside them later.

/// EXAMPLE block-level predicate: the zswap coin-commitment tree only ever
/// grows, so its `first_free` index must never decrease across a block.
pub struct ZswapFirstFreeMonotonic;

impl Predicate for ZswapFirstFreeMonotonic {
	fn name(&self) -> &'static str {
		"zswap-first-free-monotonic"
	}

	fn description(&self) -> &'static str {
		"zswap coin-commitment tree first_free index never decreases across a block (example block-level predicate)"
	}

	fn observe_block(&self, obs: &BlockObservation<'_>, out: &mut Vec<Violation>) {
		let pre = obs.pre_state.zswap.first_free;
		let post = obs.post_state.zswap.first_free;
		if post < pre {
			out.push(Violation::block_level(
				self.name(),
				obs.block,
				format!("zswap first_free decreased: {pre} -> {post}"),
			));
		}
	}
}

/// EXAMPLE transaction-level predicate: every standard transaction must
/// declare the chain's own network id.
pub struct TxNetworkIdMatches;

impl Predicate for TxNetworkIdMatches {
	fn name(&self) -> &'static str {
		"tx-network-id-matches"
	}

	fn description(&self) -> &'static str {
		"standard transactions declare the chain's network id (example transaction-level predicate)"
	}

	fn observe_block(&self, obs: &BlockObservation<'_>, out: &mut Vec<Violation>) {
		let expected = &obs.post_state.network_id;
		for (tx_index, tx) in obs.txs.iter().enumerate() {
			if let Some(network_id) = tx.network_id() {
				if network_id != expected {
					out.push(Violation::tx_level(
						self.name(),
						obs.block,
						tx_index,
						tx.transaction_hash().0.0,
						format!("network id {network_id:?} != chain network id {expected:?}"),
					));
				}
			}
		}
	}
}

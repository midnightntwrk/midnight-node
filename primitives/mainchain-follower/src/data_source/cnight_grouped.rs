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

//! cNIGHT observation events grouped by Cardano transaction.
//!
//! The observation queries return raw UTXO-level events, but the consensus
//! rules operate on whole Cardano transactions: an inherent must never admit
//! part of a transaction's events (resuming past a split tx would skip the
//! rest of its UTXOs forever). [`CNightGroupedUtxos`] makes that invariant
//! structural — events enter and leave only in whole-transaction units — so
//! callers cannot accidentally drop a single UTXO out of a transaction.
//!
//! One caveat the type cannot enforce on its own: it guarantees we never
//! split what was *fetched*, not that the fetch captured every event. A
//! row-limited query truncates its category at a position frontier (and can
//! slice the frontier transaction mid-tx); [`merge_gap_free`] therefore cuts
//! the merged set at the earliest such frontier
//! ([`CNightGroupedUtxos::truncate_at_position`]) so the set it hands out is
//! gap-free over the range it claims to cover.

use crate::ObservedUtxo;
use midnight_primitives_cnight_observation::{CardanoPosition, ObservedUtxos};

/// One observed Cardano transaction: every fetched cNIGHT event it produced,
/// in canonical [`ObservedUtxo`] order.
#[derive(Debug, Clone)]
pub struct ObservedTx {
	/// Position shared by all of the transaction's events.
	pub position: CardanoPosition,
	/// The transaction's events, sorted by header (creates before spends,
	/// then by `utxo_tx_hash`/`utxo_index`).
	pub utxos: Vec<ObservedUtxo>,
}

/// Observation events grouped by Cardano transaction, sorted by position.
///
/// This is the node-side working representation between the raw db queries
/// and the wire-format `ObservedUtxos` (whose flat `Vec<ObservedUtxo>` is
/// consensus-visible bytes and must stay as-is). Flattening a group back with
/// [`Self::into_utxos`] yields exactly the fully-sorted event sequence the
/// raw path produced, so the conversion is byte-transparent.
///
/// "Same transaction" is defined once, here: two events belong to the same
/// transaction iff their positions share `(block_number, tx_index_in_block)`
/// — the same relation `CardanoPosition`'s ordering uses.
#[derive(Debug, Clone, Default)]
pub struct CNightGroupedUtxos {
	/// One entry per distinct transaction position, sorted ascending.
	txs: Vec<ObservedTx>,
	/// Total events across all transactions, kept in sync by every mutator.
	num_utxos: usize,
}

/// The single definition of "same transaction": positions agree on
/// `(block_number, tx_index_in_block)`, the fields `CardanoPosition`'s
/// ordering compares. (Derived `PartialEq` also compares `block_hash` and
/// `block_timestamp`, which are placeholders in range-bound positions —
/// using it here would make grouping sensitive to them.)
fn same_tx(a: &CardanoPosition, b: &CardanoPosition) -> bool {
	a.block_number == b.block_number && a.tx_index_in_block == b.tx_index_in_block
}

impl CNightGroupedUtxos {
	/// Sort raw query results and group them by transaction. The only entry
	/// point from raw events, so sortedness is this type's guarantee rather
	/// than a call-site obligation.
	pub fn from_unsorted(mut utxos: Vec<ObservedUtxo>) -> Self {
		utxos.sort();
		let num_utxos = utxos.len();
		let mut txs: Vec<ObservedTx> = Vec::new();
		for utxo in utxos {
			match txs.last_mut() {
				Some(tx) if same_tx(&tx.position, &utxo.header.tx_position) => tx.utxos.push(utxo),
				_ => txs.push(ObservedTx {
					position: utxo.header.tx_position.clone(),
					utxos: vec![utxo],
				}),
			}
		}
		Self { txs, num_utxos }
	}

	/// Number of distinct Cardano transactions.
	pub fn num_transactions(&self) -> usize {
		self.txs.len()
	}

	/// Total number of events across all transactions.
	pub fn num_utxos(&self) -> usize {
		self.num_utxos
	}

	pub fn is_empty(&self) -> bool {
		self.txs.is_empty()
	}

	/// The grouped transactions, sorted ascending by position.
	pub fn txs(&self) -> &[ObservedTx] {
		&self.txs
	}

	/// Position of the last (highest) transaction, if any. Cursor rules build
	/// on this: "just past the last admitted tx" is `last_position().increment()`.
	pub fn last_position(&self) -> Option<&CardanoPosition> {
		self.txs.last().map(|tx| &tx.position)
	}

	/// Admit whole transactions in position order while they fit the
	/// acceptance envelope; return the admitted prefix and whether a cap fired.
	///
	/// - At most `tx_capacity` transactions are admitted (an empty result when
	///   `tx_capacity == 0` — misconfiguration stalls loudly at the caller
	///   rather than silently dropping events).
	/// - The next transaction is refused when it would push the event total
	///   past `max_utxos` — **except** a lone transaction bigger than the whole
	///   envelope, which is admitted alone so the runtime's bound rejects it
	///   loudly instead of the node stalling forever. (Physically unreachable
	///   given Cardano's block-size limit, but defensive.)
	pub fn take_envelope_prefix(self, tx_capacity: usize, max_utxos: usize) -> (Self, bool) {
		let mut admitted = Self::default();
		for tx in self.txs {
			let exceeds_tx_cap = admitted.txs.len() + 1 > tx_capacity;
			let exceeds_max_utxos =
				!admitted.txs.is_empty() && admitted.num_utxos + tx.utxos.len() > max_utxos;
			if exceeds_tx_cap || exceeds_max_utxos {
				return (admitted, true);
			}
			admitted.push_tx(tx);
		}
		(admitted, false)
	}

	/// Drop every transaction at or after `cut`, in place. Used to trim the
	/// merged set back to a row-limit frontier: the transaction *at* the
	/// frontier may have been sliced mid-tx by the limit, so it goes too,
	/// leaving a set that is provably gap-free over `[.., cut)`.
	pub fn truncate_at_position(&mut self, cut: &CardanoPosition) {
		let keep = self.txs.partition_point(|tx| tx.position < *cut);
		for tx in self.txs.drain(keep..) {
			self.num_utxos -= tx.utxos.len();
		}
	}

	/// Flatten back to the wire representation: the fully-sorted flat event
	/// sequence, byte-identical to sorting the raw query results directly.
	pub fn into_utxos(self) -> Vec<ObservedUtxo> {
		let mut out = Vec::with_capacity(self.num_utxos);
		for tx in self.txs {
			out.extend(tx.utxos);
		}
		out
	}

	fn push_tx(&mut self, tx: ObservedTx) {
		self.num_utxos += tx.utxos.len();
		self.txs.push(tx);
	}
}

/// Merge the per-category query results into one gap-free grouped set.
///
/// Each element of `categories` is one category's events plus its **frontier**:
/// the position of the last row the raw query returned, and `Some` only when
/// that query hit its row limit. The queries are
/// `ORDER BY block_no, block_index ... LIMIT limit`, so a category that hit the
/// limit is proven complete only *below* its frontier — and the frontier
/// transaction itself may have been sliced mid-tx by the limit.
///
/// The merged set is therefore cut at the earliest frontier across all
/// row-limited categories, dropping the frontier tx too. Every event below the
/// cut is provably fetched for **all** categories, so the result carries no
/// hidden gaps: a category's truncation can never be masked by another
/// category's events beyond that frontier. Returns the cut position (`None`
/// when no category was row-limited); the caller's cursor must resume there.
///
/// Saturation is decided upstream, from the raw row count, not from the
/// filtered event vec: these queries drop rows (undecodable datums, non-base
/// holder addresses), so a saturated query can arrive here with far fewer
/// events than the limit and would otherwise look complete.
pub fn merge_gap_free(
	categories: Vec<(Vec<ObservedUtxo>, Option<CardanoPosition>)>,
) -> (CNightGroupedUtxos, Option<CardanoPosition>) {
	let mut cut: Option<CardanoPosition> = None;
	// One sort over the concatenated categories: `from_unsorted` merges events
	// sharing a transaction position whichever category they came from, so
	// per-category accumulation (and its repeated re-sorts) buys nothing.
	let mut all: Vec<ObservedUtxo> = Vec::new();
	for (rows, frontier) in categories {
		if let Some(frontier) = frontier {
			cut = Some(match cut {
				Some(c) if c < frontier => c,
				_ => frontier,
			});
		}
		all.extend(rows);
	}
	let mut merged = CNightGroupedUtxos::from_unsorted(all);
	if let Some(cut) = &cut {
		merged.truncate_at_position(cut);
	}
	(merged, cut)
}

/// Build the observation inherent from a **gap-free** grouped event set
/// covering `[start_position, covered_end)` (what [`merge_gap_free`] returns —
/// a row-limited pull is already cut back to its proven-complete prefix, with
/// `covered_end` the cut). Caps at `tx_capacity` whole transactions and
/// `max_utxos` UTXOs (the runtime `process_tokens` envelope).
///
/// Consensus invariants:
/// - Transactions are admitted whole, so `end` lands on a tx boundary; resuming
///   there cannot skip a counted tx's UTXOs.
/// - The cursor reaches `covered_end` ONLY when every event was admitted;
///   otherwise it stops just past the last admitted tx — safe because the set
///   is gap-free, so everything not admitted is at or ahead of that boundary.
/// - The cursor never advances past an event we did not admit, and never at all
///   when nothing was admitted.
///
/// Every node feeds this the same gap-free range and the same caps, so there is
/// no fetch-size input left to disagree on — inherents are byte-identical.
pub fn truncate_to_tx_capacity(
	events: CNightGroupedUtxos,
	tx_capacity: usize,
	max_utxos: usize,
	start_position: &CardanoPosition,
	covered_end: CardanoPosition,
) -> ObservedUtxos {
	// Whole transactions only, up to both caps; the lone-oversized-tx admission
	// lives in `take_envelope_prefix`.
	let (admitted, capped) = events.take_envelope_prefix(tx_capacity, max_utxos);

	// Capped: resume just past the last admitted tx. Nothing admitted (needs a
	// `tx_capacity` of 0) holds the cursor at `start` — incrementing there would
	// step over the transaction sitting at the cursor for good, and a stalled
	// cursor is visible and recoverable where dropped mint/burn events are not.
	let end = if capped {
		admitted
			.last_position()
			.map_or_else(|| start_position.clone(), |last| last.clone().increment())
	} else {
		covered_end
	};

	ObservedUtxos { start: start_position.clone(), end, utxos: admitted.into_utxos() }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ObservedUtxoData, ObservedUtxoHeader, RegistrationData, UtxoIndexInTx};
	use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;
	use sidechain_domain::{McBlockHash, McTxHash};

	fn pos(block: u32, tx_index: u32) -> CardanoPosition {
		CardanoPosition {
			block_hash: McBlockHash([0u8; 32]),
			block_number: block,
			block_timestamp: Default::default(),
			tx_index_in_block: tx_index,
		}
	}

	/// One event at `(block, tx_index)` with `utxo_index` disambiguating
	/// events within the same transaction.
	fn utxo(block: u32, tx_index: u32, utxo_index: u16) -> ObservedUtxo {
		ObservedUtxo {
			header: ObservedUtxoHeader {
				tx_position: pos(block, tx_index),
				tx_hash: McTxHash([0u8; 32]),
				utxo_tx_hash: McTxHash([0u8; 32]),
				utxo_index: UtxoIndexInTx(utxo_index),
			},
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_reward_address: CardanoRewardAddressBytes([0u8; 29]),
				dust_public_key: vec![0u8; 33].try_into().unwrap(),
			}),
		}
	}

	/// One single-event tx per block in `range`.
	fn blocks(range: core::ops::Range<u32>) -> CNightGroupedUtxos {
		CNightGroupedUtxos::from_unsorted(range.map(|n| utxo(n, 0, 0)).collect())
	}

	fn positions(g: &CNightGroupedUtxos) -> Vec<(u32, u32)> {
		g.txs()
			.iter()
			.map(|tx| (tx.position.block_number, tx.position.tx_index_in_block))
			.collect()
	}

	#[test]
	fn from_unsorted_sorts_and_groups_by_tx() {
		// Out of order, with tx (5,1) split across non-adjacent entries.
		let events =
			vec![utxo(5, 1, 1), utxo(4, 0, 0), utxo(5, 1, 0), utxo(5, 0, 0), utxo(6, 0, 0)];
		let g = CNightGroupedUtxos::from_unsorted(events);
		assert_eq!(g.num_transactions(), 4);
		assert_eq!(g.num_utxos(), 5);
		assert_eq!(positions(&g), vec![(4, 0), (5, 0), (5, 1), (6, 0)]);
		// Tx (5,1) holds both of its events, in utxo_index order.
		assert_eq!(g.txs()[2].utxos.len(), 2);
		assert_eq!(g.txs()[2].utxos[0].header.utxo_index.0, 0);
	}

	#[test]
	fn flatten_matches_plain_sort() {
		let events =
			vec![utxo(5, 1, 1), utxo(4, 0, 0), utxo(5, 1, 0), utxo(5, 0, 0), utxo(6, 0, 0)];
		let mut sorted = events.clone();
		sorted.sort();
		let flat = CNightGroupedUtxos::from_unsorted(events).into_utxos();
		assert_eq!(flat, sorted, "group->flatten must be byte-identical to a plain sort");
	}

	#[test]
	fn envelope_prefix_cuts_on_whole_tx_at_max_utxos() {
		// 5 txs x 2 events; max_utxos 5 -> 2 whole txs (4 events), capped.
		let events: Vec<_> =
			(0..5u32).flat_map(|t| (0..2u16).map(move |u| utxo(0, t, u))).collect();
		let (admitted, capped) =
			CNightGroupedUtxos::from_unsorted(events).take_envelope_prefix(usize::MAX, 5);
		assert!(capped);
		assert_eq!(admitted.num_transactions(), 2);
		assert_eq!(admitted.num_utxos(), 4, "must not split the third tx");
	}

	#[test]
	fn envelope_prefix_respects_tx_capacity() {
		let events: Vec<_> = (0..5u32).map(|t| utxo(0, t, 0)).collect();
		let (admitted, capped) =
			CNightGroupedUtxos::from_unsorted(events).take_envelope_prefix(3, usize::MAX);
		assert!(capped);
		assert_eq!(admitted.num_transactions(), 3);
	}

	#[test]
	fn envelope_prefix_admits_lone_oversized_tx() {
		// One tx with 5 events, envelope 3: admitted whole (runtime rejects
		// loudly downstream instead of the node stalling).
		let events: Vec<_> = (0..5u16).map(|u| utxo(0, 0, u)).collect();
		let (admitted, capped) =
			CNightGroupedUtxos::from_unsorted(events).take_envelope_prefix(usize::MAX, 3);
		assert!(!capped);
		assert_eq!(admitted.num_utxos(), 5);
	}

	#[test]
	fn envelope_prefix_zero_tx_capacity_admits_nothing() {
		let events: Vec<_> = (0..3u32).map(|t| utxo(0, t, 0)).collect();
		let (admitted, capped) =
			CNightGroupedUtxos::from_unsorted(events).take_envelope_prefix(0, usize::MAX);
		assert!(capped);
		assert!(admitted.is_empty());
	}

	#[test]
	fn truncate_at_position_drops_the_frontier_tx_too() {
		// Txs at blocks 5, 6 (two events), 7; cut at block 6's position: the
		// frontier tx may be mid-sliced by a row limit, so both of its events
		// and everything after must go.
		let events = vec![utxo(5, 0, 0), utxo(6, 0, 0), utxo(6, 0, 1), utxo(7, 0, 0)];
		let mut g = CNightGroupedUtxos::from_unsorted(events);
		g.truncate_at_position(&pos(6, 0));
		assert_eq!(g.num_transactions(), 1);
		assert_eq!(g.num_utxos(), 1, "the frontier tx's events must go whole");
		assert_eq!(g.last_position().unwrap().block_number, 5);
	}

	#[test]
	fn truncate_at_position_above_all_is_noop() {
		let mut g = blocks(10..15);
		g.truncate_at_position(&pos(100, 0));
		assert_eq!(g.num_utxos(), 5);
	}

	#[test]
	fn truncate_at_position_at_or_below_first_empties() {
		let mut g = blocks(10..15);
		g.truncate_at_position(&pos(10, 0));
		assert!(g.is_empty());
		assert_eq!(g.num_utxos(), 0);
	}

	#[test]
	fn last_position_none_when_empty() {
		assert!(CNightGroupedUtxos::default().last_position().is_none());
	}

	/// No category hit its row limit: nothing is cut and the whole merge is
	/// covered (`cut == None`).
	#[test]
	fn merge_gap_free_without_row_limit_covers_everything() {
		let a: Vec<_> = (0..3u32).map(|b| utxo(b, 0, 0)).collect();
		let b = vec![utxo(1, 1, 0)];
		let (merged, cut) = merge_gap_free(vec![(a, None), (b, None)]);
		assert!(cut.is_none());
		assert_eq!(merged.num_utxos(), 4);
		assert_eq!(merged.num_transactions(), 4);
	}

	/// One category (A) is row-limited while another (B) has events beyond A's
	/// frontier. B's later events must not mask A's truncation — the merge cuts
	/// everything at A's frontier, so the gap in A can never be silently
	/// skipped over.
	#[test]
	fn category_truncation_cannot_be_masked_by_later_events() {
		// A: 10 single-UTXO txs at blocks 0..10, its raw query saturated, so
		// its frontier is the position of its last returned row (block 9).
		let a: Vec<_> = (0..10u32).map(|b| utxo(b, 0, 0)).collect();
		// B: one event below the frontier, one far beyond it.
		let b = vec![utxo(3, 1, 0), utxo(50, 0, 0)];
		let (merged, cut) = merge_gap_free(vec![(a, Some(pos(9, 0))), (b, None)]);
		let cut = cut.expect("A hit the row limit");
		assert_eq!(cut, pos(9, 0), "cut at A's frontier");
		// Kept: A's blocks 0..9 (9 events) + B's (3,1). Dropped: A's frontier
		// tx (possibly mid-sliced) and B's block-50 event beyond the frontier.
		assert_eq!(merged.num_utxos(), 10);
		assert!(merged.last_position().unwrap() < &cut);
		// The cursor resumes exactly at the cut — A's unfetched events are
		// ahead of it and get re-pulled by the next inherent.
		let obs = truncate_to_tx_capacity(merged, 1000, 100_000, &pos(0, 0), cut.clone());
		assert_eq!(obs.end, cut);
	}

	/// The cNIGHT observation skip bug, fixed structurally: a row-limited fetch
	/// is cut back to its proven-complete prefix and the cursor resumes at the
	/// cut — NEVER at the tip. The rows between the cut and the tip would
	/// otherwise be skipped forever, and a node that DID fetch them would build
	/// a different inherent (check_inherent split).
	#[test]
	fn row_limited_fetch_must_not_advance_to_tip() {
		// The range holds 50 txs (5 UTXOs each) but the fetch was row-limited
		// to the first 200 rows = 40 txs, the last of which may be mid-sliced.
		let fetched: Vec<ObservedUtxo> =
			(0..40u32).flat_map(|tx| (0..5u16).map(move |u| utxo(tx, 0, u))).collect();
		let tip = pos(100, 0);

		let (merged, cut) = merge_gap_free(vec![(fetched, Some(pos(39, 0)))]);
		let cut = cut.expect("the fetch hit the row limit");
		assert_eq!(merged.num_utxos(), 39 * 5, "the frontier tx is dropped whole");

		let obs = truncate_to_tx_capacity(merged, 1000, 100_000, &pos(0, 0), cut.clone());
		assert_ne!(obs.end, tip, "advanced to tip on a row-limited fetch -> skips unfetched txs");
		assert_eq!(obs.end, cut, "resume exactly at the frontier");
		assert!(
			obs.utxos.iter().all(|u| u.header.tx_position < cut),
			"retained a tx at/after the truncation frontier",
		);
	}

	/// A row limit sliced the sole fetched tx mid-transaction: the merge cut
	/// drops the partial tx entirely and the cursor holds at its position,
	/// never stepping over it.
	#[test]
	fn stalled_cut_at_start_holds_cursor() {
		// One tx (5 UTXOs sharing a position) sitting at the cursor.
		let events: Vec<_> = (0..5u16).map(|u| utxo(7, 0, u)).collect();
		let start = pos(7, 0);
		let (merged, cut) = merge_gap_free(vec![(events, Some(pos(7, 0)))]);
		assert!(merged.is_empty(), "a possibly-partial tx must not be admitted");
		let cut = cut.expect("the fetch hit the row limit");
		let obs = truncate_to_tx_capacity(merged, 1000, 100_000, &start, cut);
		assert!(obs.utxos.is_empty());
		assert_eq!(obs.end, start, "cursor stepped over an unobserved transaction");
	}

	/// The shipped `get_utxos_up_to_capacity` tail, verbatim, as the reference
	/// implementation for the equivalence test below.
	fn shipped_truncate(
		mut utxos: Vec<ObservedUtxo>,
		tx_capacity: usize,
		start_position: &CardanoPosition,
		end: CardanoPosition,
	) -> ObservedUtxos {
		utxos.sort();

		let mut truncated_utxos = Vec::new();
		let mut num_txs = 0;
		let mut cur_tx: Option<CardanoPosition> = None;
		for utxo in utxos {
			if cur_tx.as_ref().is_none_or(|tx| tx < &utxo.header.tx_position) {
				num_txs += 1;
				cur_tx = Some(utxo.header.tx_position.clone());
			}
			if num_txs == tx_capacity {
				break;
			}
			truncated_utxos.push(utxo);
		}

		if num_txs < tx_capacity {
			ObservedUtxos { start: start_position.clone(), end, utxos: truncated_utxos }
		} else {
			ObservedUtxos {
				start: start_position.clone(),
				end: truncated_utxos
					.last()
					.map_or(start_position.clone(), |u| u.header.tx_position.clone())
					.increment(),
				utxos: truncated_utxos,
			}
		}
	}

	/// The byte-identity claim, run: over every shape of input the node can
	/// hand it, the new pipeline (with the `tx_capacity - 1` off-by-one kept)
	/// produces exactly the events and cursor the shipped loop produced. Nodes
	/// re-derive this inherent locally and reject a mismatch, so a divergence
	/// here would be consensus-visible.
	#[test]
	fn matches_shipped_truncation_when_no_category_saturates() {
		let start = pos(0, 0);
		let tip = pos(1000, 0);
		for num_txs in 1..=8u32 {
			for events_per_tx in 1..=3u16 {
				// Spread the txs over a few blocks, several per block.
				let events: Vec<ObservedUtxo> = (0..num_txs)
					.flat_map(|t| {
						(0..events_per_tx).map(move |u| utxo(t / 3, t % 3, u)).collect::<Vec<_>>()
					})
					.collect();
				for tx_capacity in 2..=10usize {
					let want = shipped_truncate(events.clone(), tx_capacity, &start, tip.clone());
					let (merged, cut) = merge_gap_free(vec![(events.clone(), None)]);
					assert!(cut.is_none());
					let got = truncate_to_tx_capacity(
						merged,
						tx_capacity - 1,
						usize::MAX,
						&start,
						tip.clone(),
					);
					assert_eq!(
						got.utxos, want.utxos,
						"events differ at {num_txs} txs x {events_per_tx} events, cap {tx_capacity}",
					);
					assert_eq!(
						got.end, want.end,
						"cursor differs at {num_txs} txs x {events_per_tx} events, cap {tx_capacity}",
					);
				}
			}
		}
	}
}

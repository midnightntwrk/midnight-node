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
//! One caveat the type cannot enforce: it guarantees we never split what was
//! *fetched*, not that the fetch captured every event of the final
//! transaction. A row-limited query can truncate mid-tx; that is what the
//! `complete` flag on `bulk_pull` covers, and why callers drop the last
//! transaction of an incomplete fetch ([`CNightGroupedUtxos::drop_last_tx`]).

use crate::ObservedUtxo;
use midnight_primitives_cnight_observation::CardanoPosition;

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

	/// Append more raw events, restoring the sort and merging into existing
	/// transactions where positions coincide (the category queries return the
	/// same transaction's events across separate calls).
	pub fn add(&mut self, utxos: Vec<ObservedUtxo>) {
		if utxos.is_empty() {
			return;
		}
		let mut flat = core::mem::take(self).into_utxos();
		flat.extend(utxos);
		*self = Self::from_unsorted(flat);
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

	/// Keep only the transactions of the first (lowest) non-empty Cardano
	/// block — the v2 one-block-per-inherent selection. Empty stays empty.
	pub fn take_first_block(self) -> Self {
		let Some(first_block) = self.txs.first().map(|tx| tx.position.block_number) else {
			return self;
		};
		let mut out = Self::default();
		for tx in self.txs.into_iter().take_while(|tx| tx.position.block_number == first_block) {
			out.push_tx(tx);
		}
		out
	}

	/// Drop the last (highest) transaction whole. Used when a row-limited
	/// fetch may have truncated the final transaction's events mid-tx: the
	/// prefix before it is proven complete, the last tx is not.
	pub fn drop_last_tx(&mut self) {
		if let Some(tx) = self.txs.pop() {
			self.num_utxos -= tx.utxos.len();
		}
	}

	/// Clone out the transactions whose position falls in `[start, end)` —
	/// the sliding-window read path. `partition_point` finds the bounds in
	/// O(log n); the cost is the copy of the returned range only.
	pub fn slice_range(&self, start: &CardanoPosition, end: &CardanoPosition) -> Self {
		let a = self.txs.partition_point(|tx| tx.position < *start);
		let b = self.txs.partition_point(|tx| tx.position < *end);
		let txs: Vec<ObservedTx> = self.txs[a..b].to_vec();
		let num_utxos = txs.iter().map(|tx| tx.utxos.len()).sum();
		Self { txs, num_utxos }
	}

	/// Drop every transaction in a Cardano block before `block_number` — the
	/// sliding window's front trim. In place, O(log n) to find the cut.
	pub fn trim_before_block(&mut self, block_number: u32) {
		let trim_at = self.txs.partition_point(|tx| tx.position.block_number < block_number);
		for tx in self.txs.drain(..trim_at) {
			self.num_utxos -= tx.utxos.len();
		}
	}

	/// Append `extension`, whose transactions must all sort strictly after
	/// the existing ones (the sliding window only ever extends forward).
	/// Debug-asserted, so "no global re-sort is needed" is a checked claim
	/// rather than a comment.
	pub fn append(&mut self, extension: Self) {
		debug_assert!(
			match (self.last_position(), extension.txs.first()) {
				(Some(last), Some(first)) => *last < first.position,
				_ => true,
			},
			"extension must sort strictly after the existing window"
		);
		self.num_utxos += extension.num_utxos;
		self.txs.extend(extension.txs);
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
	fn add_merges_and_resorts() {
		// Category queries return the same tx's events in separate batches,
		// out of global order.
		let mut g = CNightGroupedUtxos::from_unsorted(vec![utxo(5, 0, 0), utxo(7, 0, 0)]);
		g.add(vec![utxo(4, 0, 0), utxo(5, 0, 1)]);
		assert_eq!(g.num_transactions(), 3);
		assert_eq!(g.num_utxos(), 4);
		assert_eq!(positions(&g), vec![(4, 0), (5, 0), (7, 0)]);
		assert_eq!(g.txs()[1].utxos.len(), 2, "same-position events merge into one tx");
	}

	#[test]
	fn add_empty_is_noop() {
		let mut g = CNightGroupedUtxos::from_unsorted(vec![utxo(1, 0, 0)]);
		g.add(Vec::new());
		assert_eq!(g.num_utxos(), 1);
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
	fn take_first_block_keeps_only_the_lowest_block() {
		let events = vec![utxo(5, 0, 0), utxo(5, 1, 0), utxo(6, 0, 0), utxo(7, 0, 0)];
		let g = CNightGroupedUtxos::from_unsorted(events).take_first_block();
		assert_eq!(positions(&g), vec![(5, 0), (5, 1)]);
		assert_eq!(g.num_utxos(), 2);
	}

	#[test]
	fn take_first_block_on_empty_stays_empty() {
		let g = CNightGroupedUtxos::default().take_first_block();
		assert!(g.is_empty());
	}

	#[test]
	fn drop_last_tx_removes_the_whole_tx() {
		let events = vec![utxo(5, 0, 0), utxo(6, 0, 0), utxo(6, 0, 1)];
		let mut g = CNightGroupedUtxos::from_unsorted(events);
		g.drop_last_tx();
		assert_eq!(g.num_transactions(), 1);
		assert_eq!(g.num_utxos(), 1, "both of the last tx's events must go");
		assert_eq!(g.last_position().unwrap().block_number, 5);
	}

	#[test]
	fn last_position_none_when_empty() {
		assert!(CNightGroupedUtxos::default().last_position().is_none());
	}

	/// One single-event tx per block in `range`.
	fn blocks(range: core::ops::Range<u32>) -> CNightGroupedUtxos {
		CNightGroupedUtxos::from_unsorted(range.map(|n| utxo(n, 0, 0)).collect())
	}

	fn block_numbers(g: &CNightGroupedUtxos) -> Vec<u32> {
		g.txs().iter().map(|tx| tx.position.block_number).collect()
	}

	#[test]
	fn slice_range_returns_half_open_subrange() {
		let got = blocks(0..10).slice_range(&pos(2, 0), &pos(7, 0));
		// Half-open: block 7 excluded.
		assert_eq!(block_numbers(&got), vec![2, 3, 4, 5, 6]);
		assert_eq!(got.num_utxos(), 5);
	}

	#[test]
	fn slice_range_empty_when_start_eq_end() {
		assert!(blocks(0..10).slice_range(&pos(5, 0), &pos(5, 0)).is_empty());
	}

	#[test]
	fn slice_range_empty_when_above_data() {
		assert!(blocks(0..10).slice_range(&pos(20, 0), &pos(30, 0)).is_empty());
	}

	#[test]
	fn trim_and_append_slide_the_window() {
		// Existing window covers blocks [10..30); slide to new_start=15 while
		// appending blocks [30..35).
		let mut window = blocks(10..30);
		window.trim_before_block(15);
		window.append(blocks(30..35));
		assert_eq!(block_numbers(&window), (15..35).collect::<Vec<_>>());
		assert_eq!(window.num_utxos(), 20, "trim and append must keep the count in sync");
	}

	#[test]
	fn trim_before_block_noop_below_existing_start() {
		let mut window = blocks(10..15);
		window.trim_before_block(5);
		assert_eq!(block_numbers(&window), (10..15).collect::<Vec<_>>());
	}

	#[test]
	fn append_after_full_trim() {
		// Window restart (the plan_refresh jump case): everything existing is
		// trimmed, only the extension survives.
		let mut window = blocks(10..15);
		window.trim_before_block(100);
		window.append(blocks(20..25));
		assert_eq!(block_numbers(&window), (20..25).collect::<Vec<_>>());
		assert_eq!(window.num_utxos(), 5);
	}

	#[test]
	fn append_empty_extension_is_noop() {
		let mut window = blocks(10..15);
		window.append(CNightGroupedUtxos::default());
		assert_eq!(window.num_utxos(), 5);
	}
}

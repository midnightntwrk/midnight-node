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

//! v2 cNIGHT observation derivation: **at most one Cardano block per inherent**.
//!
//! Selected by the IDP once the parent runtime `spec_version` reaches the v2
//! activation (see `idp::cnight_observation::CNIGHT_OBSERVATION_V2_SPEC_VERSION`).
//! Until then the frozen v1 path runs and this is dormant.
//!
//! The consensus rule is deliberately tiny: an inherent ingests whole cNIGHT
//! transactions from a **single Cardano block** — the first non-empty block at
//! or after the cursor — and advances the cursor past what it took. Usually
//! that is the whole block; an oversized block yields only a whole-tx prefix
//! and drains across several inherents (see below). Because "the events in
//! block N" is unambiguous and transactions are never split, there is no
//! cross-block truncation, no row-limit completeness flag, and no synthesised
//! cursor: the cursor is always a real position.
//!
//! Cases:
//! - **empty span** — no events in `[cursor, tip]`: advance the cursor to the
//!   tip (its real hash, already resolved). Skips empty Cardano blocks for free.
//! - **normal** — the first non-empty block fits the envelope: ingest it whole;
//!   cursor parks just past its last event (a real block boundary).
//! - **oversized** (rare) — a single Cardano block carries more than `max_utxos`
//!   cNIGHT events: ingest a whole-transaction prefix up to the envelope and
//!   park the cursor at a real tx boundary *inside* the block; the next inherent
//!   resumes there, draining the block across several Midnight blocks.
//!
//! The data fetch reuses the bounded `bulk_pull` (one envelope's worth plus a
//! sentinel) — enough to hold the first block whole, or the first `max_utxos+1`
//! rows of an oversized one. Cache integration (serving whole blocks from the
//! sliding window instead of Postgres) is a pure-perf layer to be added under
//! `select_one_block`; it must not change this output.

use crate::data_source::cnight_grouped::CNightGroupedUtxos;
use crate::data_source::cnight_observation::MidnightCNightObservationDataSourceError;
use crate::data_source::cnight_observation_bulk::bulk_pull;
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use sidechain_domain::McBlockHash;
use sqlx::PgPool;

/// Derive the v2 (one-block) inherent for `[start_position, current_tip]`.
///
/// `tx_capacity`/`max_utxos` are the on-chain budget at the parent;
/// `max_utxos == tx_capacity * 64` is the runtime `process_tokens` bound and is
/// the only cap that binds here (one Cardano block is far under it except in the
/// oversized case).
pub async fn derive_inherent_v2(
	pool: &PgPool,
	config: &CNightAddresses,
	start_position: &CardanoPosition,
	current_tip: McBlockHash,
	_tx_capacity: usize,
	max_utxos: usize,
) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
	// Resolve the committed tip to a position; `end` (tip + 1) is the cursor used
	// when the whole span is empty.
	let end: CardanoPosition = crate::db::get_block_by_hash(pool, current_tip.clone())
		.await?
		.ok_or(MidnightCNightObservationDataSourceError::MissingBlockReference(current_tip))?
		.into();
	let end = end.increment();

	// One envelope's worth plus a sentinel: holds the first non-empty block
	// whole, or the first `max_utxos + 1` rows of an oversized block (enough to
	// cap it). A row-limited pull is cut back to its proven-complete prefix;
	// feeding the cut as the empty-span cursor means we can never advance past
	// it (a non-empty first block parks the cursor at a tx boundary at or
	// below the cut anyway, since every event in the set sits below it).
	let (events, cut) =
		bulk_pull(pool, config, start_position, &end, max_utxos.saturating_add(1)).await?;
	let covered_end = cut.unwrap_or(end);

	Ok(select_one_block(events, start_position, max_utxos, covered_end))
}

/// Pure one-block selection + truncation, factored out so the consensus rule is
/// unit-testable without a database. `events` are the events in `[cursor, tip]`
/// (grouped and sorted, as `bulk_pull` returns them); `tip_end` is the
/// tip-derived cursor used when the span holds no events.
fn select_one_block(
	events: CNightGroupedUtxos,
	cursor: &CardanoPosition,
	max_utxos: usize,
	tip_end: CardanoPosition,
) -> ObservedUtxos {
	if events.is_empty() {
		// Empty span: advance to the tip (real hash). Empty Cardano blocks
		// between cursor and tip are skipped for free.
		return ObservedUtxos { start: cursor.clone(), end: tip_end, utxos: Vec::new() };
	}

	// Only the first non-empty Cardano block is ingested this inherent, capped
	// to a whole-tx prefix of the envelope. `tx_capacity` does not bind on the
	// v2 path, so only `max_utxos` is enforced; a lone tx bigger than the whole
	// envelope is admitted alone (see `take_envelope_prefix`).
	let (admitted, _capped) = events.take_first_block().take_envelope_prefix(usize::MAX, max_utxos);

	// Cursor = just past the last admitted tx: a real position. Normally the
	// block boundary (whole block ingested); a tx boundary inside the block while
	// draining an oversized one. (`admitted` is never empty here — the envelope
	// prefix of a non-empty group always admits its first tx — so the cursor
	// fallback is defensive only.)
	let end = admitted.last_position().cloned().unwrap_or_else(|| cursor.clone()).increment();
	ObservedUtxos { start: cursor.clone(), end, utxos: admitted.into_utxos() }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, RegistrationData, UtxoIndexInTx,
	};
	use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;
	use sidechain_domain::McTxHash;

	/// Group raw test events the way `bulk_pull` would.
	fn grouped(events: Vec<ObservedUtxo>) -> CNightGroupedUtxos {
		CNightGroupedUtxos::from_unsorted(events)
	}

	/// One UTXO at `(block, tx_index)`.
	fn utxo(block: u32, tx_index: u32) -> ObservedUtxo {
		ObservedUtxo {
			header: ObservedUtxoHeader {
				tx_position: pos(block, tx_index),
				tx_hash: McTxHash([0u8; 32]),
				utxo_tx_hash: McTxHash([0u8; 32]),
				utxo_index: UtxoIndexInTx(0),
			},
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_reward_address: CardanoRewardAddressBytes([0u8; 29]),
				dust_public_key: vec![0u8; 33].try_into().unwrap(),
			}),
		}
	}

	fn pos(block: u32, tx_index: u32) -> CardanoPosition {
		CardanoPosition {
			block_hash: McBlockHash([0u8; 32]),
			block_number: block,
			block_timestamp: Default::default(),
			tx_index_in_block: tx_index,
		}
	}

	/// Empty span: cursor jumps to the tip, no UTXOs.
	#[test]
	fn empty_span_advances_to_tip() {
		let got = select_one_block(CNightGroupedUtxos::default(), &pos(5, 0), 12_800, pos(100, 0));
		assert!(got.utxos.is_empty());
		assert_eq!(got.end, pos(100, 0));
	}

	/// A single block under the envelope is ingested whole; cursor parks at the
	/// block boundary, NOT the tip.
	#[test]
	fn ingests_one_whole_block() {
		// block 5 with 3 distinct txs; tip far away at block 100.
		let events = vec![utxo(5, 0), utxo(5, 1), utxo(5, 2)];
		let got = select_one_block(grouped(events), &pos(5, 0), 12_800, pos(100, 0));
		assert_eq!(got.utxos.len(), 3);
		assert_eq!(got.end, pos(5, 2).increment(), "cursor parks past block 5's last tx");
		assert_ne!(got.end, pos(100, 0), "must not jump to tip with events present");
	}

	/// Only the FIRST non-empty block is taken, even when later blocks are
	/// present in the fetched window.
	#[test]
	fn takes_only_the_first_block() {
		let events = vec![utxo(5, 0), utxo(6, 0), utxo(7, 0)];
		let got = select_one_block(grouped(events), &pos(5, 0), 12_800, pos(100, 0));
		assert_eq!(got.utxos.len(), 1);
		assert_eq!(got.utxos[0].header.tx_position.block_number, 5);
		assert_eq!(got.end, pos(5, 0).increment());
	}

	/// Oversized block: a whole-tx prefix up to the envelope is admitted and the
	/// cursor parks at a real tx boundary INSIDE the block (the drain).
	#[test]
	fn oversized_block_drains_at_tx_boundary() {
		// block 5 with 5 single-UTXO txs, envelope 3 → admit txs 0,1,2.
		let events: Vec<_> = (0..5u32).map(|t| utxo(5, t)).collect();
		let got = select_one_block(grouped(events), &pos(5, 0), 3, pos(100, 0));
		assert_eq!(got.utxos.len(), 3, "capped to the envelope on a whole-tx boundary");
		assert_eq!(got.end, pos(5, 2).increment(), "cursor parks mid-block to resume the drain");
		assert!(got.end < pos(6, 0), "did not advance past the oversized block");
	}

	/// A single transaction with more UTXOs than the envelope is admitted whole
	/// (so the runtime rejects loudly rather than the node stalling).
	#[test]
	fn lone_oversized_tx_admitted_whole() {
		// one tx (shared position) with 5 UTXOs, envelope 3.
		let events: Vec<_> = (0..5u32).map(|_| utxo(5, 0)).collect();
		let got = select_one_block(grouped(events), &pos(5, 0), 3, pos(100, 0));
		assert_eq!(got.utxos.len(), 5, "lone over-envelope tx admitted whole, not split");
		assert_eq!(got.end, pos(5, 0).increment());
	}
}

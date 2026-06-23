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

//! Guard tests for the frozen v1 cNIGHT derivation
//! (`data_source::cnight_observation_v1::derive_inherent_v1`).
//!
//! v1 authored finalized mainnet history; any future node must re-derive its
//! inherents byte-for-byte or `check_inherent` rejects historical blocks on
//! import. These tests are the empirical backstop for that — in particular for
//! the one documented faithfulness deviation (the `get_low/high_bounds`
//! `ORDER BY id` added since 1.0.1), which only a run against real db-sync data
//! can settle.
//!
//! Both tests are env-gated (skip-with-message when unset) so the normal
//! `cargo test` stays green without a database.
//!
//! ## `v1_walk_invariants` — needs only a db-sync
//!
//! Set `CNIGHT_V1_REPLAY_DATABASE_URL` to a db-sync postgres connection string.
//! Walks a Cardano block range, threading the cursor exactly as the chain would
//! (each block's hash becomes the next tip), and asserts the invariants v1 must
//! always satisfy: the cursor never regresses, the UTXO count stays within the
//! `tx_capacity * 64` envelope, and the derivation is deterministic (identical
//! across two runs of the same input). Does not prove "== mainnet", but catches
//! panics, non-determinism, and envelope violations against real data.
//!
//! ## `v1_replay_matches_fixture` — the gold standard
//!
//! Also set `CNIGHT_V1_REPLAY_FIXTURE` to a JSON file of ground-truth inherents
//! exported from an archive node / RPC for the SAME network the db-sync follows:
//!
//! ```json
//! [
//!   {
//!     "tx_capacity": 200,
//!     "start_position":          { "block_hash": "0x..", "block_number": 1, "block_timestamp": 0, "tx_index_in_block": 0 },
//!     "tip_hash":                "0x..",
//!     "expected_utxos":          [ /* ObservedUtxo */ ],
//!     "expected_next_position":  { "block_hash": "0x..", "block_number": 2, "block_timestamp": 0, "tx_index_in_block": 0 }
//!   }
//! ]
//! ```
//!
//! For each entry it runs `derive_inherent_v1` and asserts the produced
//! `utxos` and cursor equal the on-chain inherent. A failure here is a genuine
//! replay divergence — investigate before shipping any node that would import
//! that history.

use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxo};
use midnight_primitives_mainchain_follower::data_source::cnight_observation_v1::derive_inherent_v1;
use sidechain_domain::McBlockHash;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;

/// v1's frozen UTXO-per-tx over-estimate; the envelope is `tx_capacity * 64`.
const V1_OVERESTIMATE: usize = 64;

fn env(key: &str) -> Option<String> {
	std::env::var(key).ok()
}

fn load_addresses() -> CNightAddresses {
	let path = env("CNIGHT_V1_REPLAY_ADDRESSES")
		.unwrap_or_else(|| "../../res/qanet/cnight-addresses.json".to_string());
	let text =
		std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read addresses {path}: {e}"));
	serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse addresses {path}: {e}"))
}

fn mc_hash(bytes: Vec<u8>) -> McBlockHash {
	McBlockHash(bytes.try_into().expect("db-sync block hash is 32 bytes"))
}

#[tokio::test]
async fn v1_walk_invariants() {
	let Some(database_url) = env("CNIGHT_V1_REPLAY_DATABASE_URL") else {
		eprintln!(
			"SKIP cnight_v1_replay::v1_walk_invariants: set CNIGHT_V1_REPLAY_DATABASE_URL \
			 to a db-sync postgres connection string to run it."
		);
		return;
	};
	let addresses = load_addresses();
	let tx_capacity: usize =
		env("CNIGHT_V1_REPLAY_TX_CAPACITY").and_then(|v| v.parse().ok()).unwrap_or(200);
	let envelope = tx_capacity * V1_OVERESTIMATE;

	let pool = PgPoolOptions::new()
		.max_connections(4)
		.connect(&database_url)
		.await
		.expect("connect to db-sync postgres");

	let from_block: i64 =
		env("CNIGHT_V1_REPLAY_FROM_BLOCK").and_then(|v| v.parse().ok()).unwrap_or(0);
	// Keep the default range small — every step is a full derivation round-trip.
	let to_block: i64 = env("CNIGHT_V1_REPLAY_TO_BLOCK")
		.and_then(|v| v.parse().ok())
		.unwrap_or(from_block + 500);

	let blocks = sqlx::query(
		"SELECT block_no::bigint AS block_no, hash FROM block \
		 WHERE block_no >= $1 AND block_no <= $2 AND block_no IS NOT NULL ORDER BY block_no",
	)
	.bind(from_block)
	.bind(to_block)
	.fetch_all(&pool)
	.await
	.expect("query block range");
	assert!(!blocks.is_empty(), "no blocks in [{from_block}, {to_block}] — widen the range");

	// Start the cursor at the first block, tx 0.
	let first_block: i64 = blocks[0].get::<i64, _>("block_no");
	let mut cursor = CardanoPosition {
		block_hash: McBlockHash([0u8; 32]),
		block_number: u32::try_from(first_block).expect("block_no fits u32"),
		block_timestamp: Default::default(),
		tx_index_in_block: 0,
	};

	for row in &blocks {
		let tip = mc_hash(row.get::<Vec<u8>, _>("hash"));

		let a = derive_inherent_v1(&pool, &addresses, &cursor, tip.clone(), tx_capacity)
			.await
			.expect("derive_inherent_v1 (first run)");
		// Determinism: identical inputs must yield an identical inherent.
		let b = derive_inherent_v1(&pool, &addresses, &cursor, tip.clone(), tx_capacity)
			.await
			.expect("derive_inherent_v1 (second run)");
		assert_eq!(a.utxos, b.utxos, "v1 derivation non-deterministic (utxos) at {cursor:?}");
		assert_eq!(a.end, b.end, "v1 derivation non-deterministic (cursor) at {cursor:?}");

		// Envelope: the inherent must fit the runtime's `process_tokens` bound.
		assert!(
			a.utxos.len() <= envelope,
			"v1 inherent exceeds envelope ({} > {envelope}) at {cursor:?}",
			a.utxos.len(),
		);
		// Cursor never regresses (process_tokens enforces `next >= prev`).
		assert!(a.end >= cursor, "v1 cursor regressed: {:?} < {cursor:?}", a.end);

		cursor = a.end;
	}
	eprintln!(
		"v1_walk_invariants: walked {} blocks [{from_block}, {to_block}], cursor at {cursor:?}",
		blocks.len()
	);
}

#[tokio::test]
async fn v1_replay_matches_fixture() {
	let (Some(database_url), Some(fixture_path)) =
		(env("CNIGHT_V1_REPLAY_DATABASE_URL"), env("CNIGHT_V1_REPLAY_FIXTURE"))
	else {
		eprintln!(
			"SKIP cnight_v1_replay::v1_replay_matches_fixture: set CNIGHT_V1_REPLAY_DATABASE_URL \
			 and CNIGHT_V1_REPLAY_FIXTURE (ground-truth inherents JSON) to run it."
		);
		return;
	};
	let addresses = load_addresses();

	let pool = PgPoolOptions::new()
		.max_connections(4)
		.connect(&database_url)
		.await
		.expect("connect to db-sync postgres");

	let text = std::fs::read_to_string(&fixture_path)
		.unwrap_or_else(|e| panic!("read fixture {fixture_path}: {e}"));
	// Parse via `Value` so the test needs no serde-derive dependency; the typed
	// fields deserialize through the existing `Deserialize` impls.
	let cases: Vec<serde_json::Value> =
		serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse fixture {fixture_path}: {e}"));
	assert!(!cases.is_empty(), "fixture {fixture_path} is empty");

	let mut checked = 0usize;
	for (i, case) in cases.iter().enumerate() {
		let tx_capacity = case["tx_capacity"]
			.as_u64()
			.unwrap_or_else(|| panic!("case {i}: tx_capacity")) as usize;
		let start: CardanoPosition = serde_json::from_value(case["start_position"].clone())
			.unwrap_or_else(|e| panic!("case {i}: start_position: {e}"));
		let tip: McBlockHash = serde_json::from_value(case["tip_hash"].clone())
			.unwrap_or_else(|e| panic!("case {i}: tip_hash: {e}"));
		let expected_utxos: Vec<ObservedUtxo> =
			serde_json::from_value(case["expected_utxos"].clone())
				.unwrap_or_else(|e| panic!("case {i}: expected_utxos: {e}"));
		let expected_next: CardanoPosition =
			serde_json::from_value(case["expected_next_position"].clone())
				.unwrap_or_else(|e| panic!("case {i}: expected_next_position: {e}"));

		let got = derive_inherent_v1(&pool, &addresses, &start, tip, tx_capacity)
			.await
			.unwrap_or_else(|e| panic!("case {i}: derive_inherent_v1: {e}"));

		assert_eq!(
			got.utxos, expected_utxos,
			"case {i}: v1 replay utxos diverge from on-chain at start {start:?}"
		);
		assert_eq!(
			got.end, expected_next,
			"case {i}: v1 replay cursor diverges from on-chain at start {start:?}"
		);
		checked += 1;
	}
	eprintln!("v1_replay_matches_fixture: {checked} on-chain inherents replayed byte-identically");
}

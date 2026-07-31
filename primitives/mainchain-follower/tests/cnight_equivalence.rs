// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Equivalence test for the cNIGHT observation data sources.
//!
//! The bulk in-memory `BulkCachedCNightObservationDataSource` is a performance
//! optimization over the per-call `MidnightCNightObservationDataSourceImpl`.
//! Both implement the same consensus-affecting contract, so for the same query
//! they must return identical `ObservedUtxos`. This test runs both against a
//! real db-sync Postgres over a range of Cardano blocks and asserts equality.
//!
//! It is **gated on `CNIGHT_EQUIV_DATABASE_URL`** and silently skips when that
//! is unset, so it is a no-op in normal `cargo test` / CI. Point it at a db-sync
//! instance (e.g. qanet) to run it:
//!
//! ```bash
//! CNIGHT_EQUIV_DATABASE_URL='postgres://user:pass@host:5432/cexplorer?sslmode=require' \
//!   cargo test -p midnight-primitives-mainchain-follower --test cnight_equivalence -- --nocapture
//! ```
//!
//! Optional overrides:
//! - `CNIGHT_EQUIV_ADDRESSES` path to a cnight-addresses.json (default: `res/qanet/cnight-addresses.json`)
//! - `CNIGHT_EQUIV_FROM_BLOCK`       first Cardano block_no (default: 0)
//! - `CNIGHT_EQUIV_TO_BLOCK`         last Cardano block_no (default: max in db)
//! - `CNIGHT_EQUIV_TX_CAPACITY`      whole-tx capacity per call (default: 200)
//! - `CNIGHT_EQUIV_UTXO_OVERESTIMATE` per-query SQL over-fetch bound (default: 12800)
//! - `CNIGHT_EQUIV_MAX_COMPARISONS`  cap on number of (start,tip) queries (default: 500)
//!
//! NOTE: the standard source clips its SQL window to `LIVE_PULL_BLOCK_DELTA`
//! (64) blocks — an optimization that is only equivalent if `tx_capacity` worth
//! of transactions always fits within that span. A divergence reported here is
//! therefore a genuine finding: either a bug in the bulk source, or evidence
//! that the clip assumption does not hold on this data.

use std::sync::Arc;

use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, TimestampUnixMillis,
};
use midnight_primitives_mainchain_follower::data_source::{
	BulkCacheConfig, BulkCachedCNightObservationDataSource, DEFAULT_WINDOW_SIZE,
	MidnightCNightObservationDataSourceImpl, bulk_pull,
};
use midnight_primitives_mainchain_follower::inherent_provider::MidnightCNightObservationDataSource;
use sidechain_domain::McBlockHash;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;

/// Matches the bulk source's internal cache-warming over-fetch cap.
const LARGE_LIMIT: usize = 5_000_000;

fn env_parsed<T: std::str::FromStr>(key: &str) -> Option<T> {
	std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn whole_block_position(block_number: u32, tx_index_in_block: u32) -> CardanoPosition {
	CardanoPosition {
		block_hash: McBlockHash([0u8; 32]),
		block_number,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block,
	}
}

#[tokio::test]
async fn bulk_source_matches_standard_over_block_range() {
	let Ok(database_url) = std::env::var("CNIGHT_EQUIV_DATABASE_URL") else {
		eprintln!(
			"SKIP cnight_equivalence: set CNIGHT_EQUIV_DATABASE_URL to a db-sync \
			 postgres connection string to run this test."
		);
		return;
	};

	// cNIGHT addresses — defaults to the committed qanet config. These must
	// match the network the db-sync instance is following.
	let addresses_path = std::env::var("CNIGHT_EQUIV_ADDRESSES")
		.unwrap_or_else(|_| "../../res/qanet/cnight-addresses.json".to_string());
	let addresses: CNightAddresses = serde_json::from_str(
		&std::fs::read_to_string(&addresses_path)
			.unwrap_or_else(|e| panic!("read {addresses_path}: {e}")),
	)
	.unwrap_or_else(|e| panic!("parse {addresses_path}: {e}"));

	let pool = PgPoolOptions::new()
		.max_connections(8)
		.connect(&database_url)
		.await
		.expect("connect to db-sync postgres");

	// Block range to compare over (block_no cast to bigint to avoid int4/int8
	// schema differences between db-sync versions).
	let from_block: i64 = env_parsed("CNIGHT_EQUIV_FROM_BLOCK").unwrap_or(0);
	let to_block: i64 = match env_parsed("CNIGHT_EQUIV_TO_BLOCK") {
		Some(v) => v,
		None => sqlx::query("SELECT max(block_no)::bigint AS m FROM block")
			.fetch_one(&pool)
			.await
			.expect("query max block_no")
			.get::<Option<i64>, _>("m")
			.expect("block table is empty"),
	};
	assert!(to_block > from_block, "empty block range [{from_block}, {to_block}]");

	// Consensus knobs — override to match the runtime config of the network.
	let tx_capacity: usize = env_parsed("CNIGHT_EQUIV_TX_CAPACITY").unwrap_or(200);
	let utxo_overestimate: usize = env_parsed("CNIGHT_EQUIV_UTXO_OVERESTIMATE").unwrap_or(12_800);

	let rows = sqlx::query(
		"SELECT block_no::bigint AS block_no, hash FROM block \
		 WHERE block_no >= $1 AND block_no <= $2 AND block_no IS NOT NULL \
		 ORDER BY block_no",
	)
	.bind(from_block)
	.bind(to_block)
	.fetch_all(&pool)
	.await
	.expect("query block hashes");
	assert!(!rows.is_empty(), "no blocks found in [{from_block}, {to_block}]");

	let blocks: Vec<(u32, McBlockHash)> = rows
		.iter()
		.map(|r| {
			let block_no: i64 = r.get("block_no");
			let hash: Vec<u8> = r.get("hash");
			let hash: [u8; 32] = hash.try_into().expect("db-sync block.hash is 32 bytes");
			(u32::try_from(block_no).expect("block_no fits in u32"), McBlockHash(hash))
		})
		.collect();

	let window_from = blocks.first().unwrap().0;
	let window_to = blocks.last().unwrap().0;

	// Standard (oracle) source — queries db-sync directly on every call.
	let standard = MidnightCNightObservationDataSourceImpl::new(pool.clone(), None, 0);

	// Bulk/cached source — pre-populate its window over the full range so the
	// in-memory cache-serving path (not the db fallback) is what we compare.
	let window_start = whole_block_position(window_from, 0);
	let window_end =
		whole_block_position(window_to, u32::try_from(i32::MAX).expect("i32::MAX is non-negative"));
	let events = bulk_pull(&pool, &addresses, &window_start, &window_end, LARGE_LIMIT)
		.await
		.expect("bulk_pull window");
	eprintln!("cached window [{window_from}, {window_to}] holds {} events", events.len());

	let db_fallback = Arc::new(MidnightCNightObservationDataSourceImpl::new(pool.clone(), None, 0));
	let bulk = BulkCachedCNightObservationDataSource::new(
		events,
		BulkCacheConfig {
			window_start_block: window_from,
			window_end_block: window_to,
			window_size: DEFAULT_WINDOW_SIZE,
			stability_margin: 0, // irrelevant for a pre-populated, static window
			pool: pool.clone(),
			db_fallback,
			cnight_addresses: addresses.clone(),
			metrics_opt: None,
		},
	);

	// Compare both sources over many (start, tip) queries within the window.
	//
	// The tip is held a small number of blocks ahead of `start`
	// (`CNIGHT_EQUIV_TIP_DELTA`, default = LIVE_PULL_BLOCK_DELTA = 64). This is
	// the realistic sync regime *and* the regime where the standard source's
	// 64-block clip does not bind, so any divergence is a genuine bulk-source
	// bug rather than the by-design clip difference. Set a larger TIP_DELTA to
	// deliberately probe the clip boundary.
	let tip_delta: usize = env_parsed("CNIGHT_EQUIV_TIP_DELTA").unwrap_or(64);

	let max_comparisons: usize = env_parsed("CNIGHT_EQUIV_MAX_COMPARISONS").unwrap_or(500);
	let step = (blocks.len() / max_comparisons.max(1)).max(1);

	let mut compared = 0usize;
	let mut mismatches = 0usize;
	for (i, (block_no, block_hash)) in blocks.iter().enumerate().step_by(step) {
		let start = CardanoPosition {
			block_hash: block_hash.clone(),
			block_number: *block_no,
			block_timestamp: TimestampUnixMillis(0),
			tx_index_in_block: 0,
		};
		let tip = blocks[(i + tip_delta).min(blocks.len() - 1)].1.clone();
		let standard_result = standard
			.get_utxos_up_to_capacity(
				&addresses,
				&start,
				tip.clone(),
				tx_capacity,
				utxo_overestimate,
			)
			.await
			.expect("standard source query");
		let bulk_result = bulk
			.get_utxos_up_to_capacity(
				&addresses,
				&start,
				tip.clone(),
				tx_capacity,
				utxo_overestimate,
			)
			.await
			.expect("bulk source query");

		compared += 1;
		// `ObservedUtxos` has no `PartialEq`, so compare its fields directly.
		let equal = standard_result.start == bulk_result.start
			&& standard_result.end == bulk_result.end
			&& standard_result.utxos == bulk_result.utxos;
		if !equal {
			mismatches += 1;
			eprintln!(
				"MISMATCH at start block {block_no}: standard {} utxos [{:?} .. {:?}], bulk {} utxos [{:?} .. {:?}]",
				standard_result.utxos.len(),
				standard_result.start,
				standard_result.end,
				bulk_result.utxos.len(),
				bulk_result.start,
				bulk_result.end,
			);
			for (i, (a, b)) in
				standard_result.utxos.iter().zip(bulk_result.utxos.iter()).enumerate()
			{
				if a != b {
					eprintln!(
						"  first differing utxo at index {i}:\n    standard: {a:?}\n    bulk:     {b:?}"
					);
					break;
				}
			}
		}
	}

	eprintln!("compared {compared} queries, {mismatches} mismatch(es)");
	assert_eq!(
		mismatches, 0,
		"{mismatches}/{compared} queries diverged between the bulk and standard cNIGHT sources"
	);
}

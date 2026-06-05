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

//! Database Queries
//!
//! This module provides database queries used for cNight token observation
//! To get a better understanding of how these queries are working, see the schema documentation for db-sync:
//! https://github.com/IntersectMBO/cardano-db-sync/blob/master/doc/schema.md
use crate::db::{
	AssetCreateRow, AssetSpendRow, Block, DeregistrationRow, PagedQuery, QueryBounds,
	RegistrationRow,
};
use log::info;
use sidechain_domain::*;
use sqlx::{Pool, Postgres, error::Error as SqlxError};

pub async fn get_registrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	auth_token_ident: i64,
	query: &PagedQuery<'_>,
) -> Result<Vec<RegistrationRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	sqlx::query_as!(
		RegistrationRow,
		r#"
SELECT
    datum.value::jsonb AS "full_datum!: _",
    block.block_no AS "block_number!: _",
    block.hash AS "block_hash: _",
    block.time AS "block_timestamp: _",
    tx.block_index AS "tx_index_in_block: _",
    tx.hash AS "tx_hash: _",
    tx_out.index AS "utxo_index: _"
FROM block
    JOIN tx ON tx.block_id = block.id
    JOIN tx_out ON tx_out.tx_id = tx.id
    JOIN datum ON tx_out.data_hash = datum.hash
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE tx.id >= $9 AND tx.id <= $10
    AND tx_out.id >= $11 AND tx_out.id <= $12
    AND ma_tx_out.id >= $13 AND ma_tx_out.id <= $14
    AND block.block_no >= $3 AND block.block_no <= $5
    AND tx_out.address = $1
    AND ma_tx_out.ident = $2
    AND ma_tx_out.quantity = 1
    AND (block.block_no > $3 OR (block.block_no = $3 AND tx.block_index >= $4))
    AND (block.block_no < $5 OR (block.block_no = $5 AND tx.block_index < $6))
ORDER BY block.block_no, tx.block_index
LIMIT $7 OFFSET $8;
        "#,
		smart_contract_address,
		auth_token_ident,
		query.start.block_number as i32,
		query.start.tx_index_in_block as i32,
		query.end.block_number as i32,
		query.end.tx_index_in_block as i32,
		query.limit as i32,
		query.offset as i32,
		query.low_bound.tx_id,
		query.high_bound.tx_id,
		query.low_bound.tx_out_id,
		query.high_bound.tx_out_id,
		query.low_bound.ma_tx_out_id,
		query.high_bound.ma_tx_out_id,
	)
	.fetch_all(pool)
	.await
}

pub async fn get_deregistrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	query: &PagedQuery<'_>,
) -> Result<Vec<DeregistrationRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	// NOTE: Ordered by transaction index (i.e. index of transaction within block)
	// Once one valid deregistration can occur in a single tx, so we don't have to worry about
	// ordering within txs

	sqlx::query_as!(
		DeregistrationRow,
		r#"
SELECT
    datum.value::jsonb AS "full_datum!: _",
    block.block_no as "block_number!: _",
    block.hash as "block_hash: _",
    block.time as "block_timestamp: _",
    tx.block_index as "tx_index_in_block: _",
    tx.hash AS "tx_hash: _",
    tx_tx_out.hash as "utxo_tx_hash: _",
    tx_out.index as "utxo_index: _"
FROM block
    JOIN tx ON tx.block_id = block.id
    JOIN tx_in ON tx_in.tx_in_id = tx.id
    JOIN tx_out ON tx_out.tx_id = tx_in.tx_out_id
                AND tx_out.index = tx_in.tx_out_index
    JOIN tx as tx_tx_out ON tx_out.tx_id = tx_tx_out.id
    JOIN datum ON datum.hash = tx_out.data_hash
WHERE block.block_no >= $2 AND block.block_no <= $4
    AND tx_out.address = $1
    AND (block.block_no > $2 OR (block.block_no = $2 AND tx.block_index >= $3))
    AND (block.block_no < $4 OR (block.block_no = $4 AND tx.block_index < $5))
    AND tx.id >= $8 AND tx.id <=$9
    AND tx_in.id >= $10 AND tx_in.id <= $11
ORDER BY block.block_no, tx.block_index
LIMIT $6 OFFSET $7;
        "#,
		smart_contract_address,
		query.start.block_number as i32,
		query.start.tx_index_in_block as i32,
		query.end.block_number as i32,
		query.end.tx_index_in_block as i32,
		query.limit as i32,
		query.offset as i32,
		query.low_bound.tx_id,
		query.high_bound.tx_id,
		query.low_bound.tx_in_id,
		query.high_bound.tx_in_id,
	)
	.fetch_all(pool)
	.await
}

pub(crate) async fn get_asset_creates(
	pool: &Pool<Postgres>,
	ident: i64,
	query: &PagedQuery<'_>,
) -> Result<Vec<AssetCreateRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	sqlx::query_as!(
		AssetCreateRow,
		r#"
SELECT
    block.block_no AS "block_number!: _",
    block.hash AS "block_hash: _",
    block.time AS "block_timestamp: _",
    tx.block_index AS "tx_index_in_block: _",
    ma_tx_out.quantity::BIGINT AS "quantity!",
    tx_out.address AS holder_address,
    tx.hash AS "tx_hash: _",
    tx_out.index AS "utxo_index: _"
FROM block
    JOIN tx ON tx.block_id = block.id
    JOIN tx_out ON tx_out.tx_id = tx.id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE tx.id >= $8 AND tx.id <= $9
    AND tx_out.id >= $10 AND tx_out.id <= $11
    AND ma_tx_out.id >= $12 AND ma_tx_out.id <= $13
    AND block.block_no >= $2 AND block.block_no <= $4
    AND ma_tx_out.ident = $1
    AND (block.block_no > $2 OR (block.block_no = $2 AND tx.block_index >= $3))
    AND (block.block_no < $4 OR (block.block_no = $4 AND tx.block_index < $5))
ORDER BY block.block_no, tx.block_index, tx_out.index
LIMIT $6 OFFSET $7;
    "#,
		ident,
		query.start.block_number as i32,
		query.start.tx_index_in_block as i32,
		query.end.block_number as i32,
		query.end.tx_index_in_block as i32,
		query.limit as i32,
		query.offset as i32,
		query.low_bound.tx_id,
		query.high_bound.tx_id,
		query.low_bound.tx_out_id,
		query.high_bound.tx_out_id,
		query.low_bound.ma_tx_out_id,
		query.high_bound.ma_tx_out_id,
	)
	.fetch_all(pool)
	.await
}

pub(crate) async fn get_asset_spends(
	pool: &Pool<Postgres>,
	ident: i64,
	query: &PagedQuery<'_>,
) -> Result<Vec<AssetSpendRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	sqlx::query_as!(
		AssetSpendRow,
		r#"
SELECT
    spending_block.block_no AS "block_number!: _",
    spending_block.hash AS "block_hash: _",
    spending_block.time AS "block_timestamp: _",
    spending_tx.block_index AS "tx_index_in_block: _",
    ma_tx_out.quantity::BIGINT AS "quantity!",
    tx_out.address AS holder_address,
    tx.hash AS "utxo_tx_hash: _",
    tx_out.index AS "utxo_index: _",
    spending_tx.hash AS "spending_tx_hash: _"
FROM block AS spending_block
    JOIN tx AS spending_tx ON spending_tx.block_id = spending_block.id
    JOIN tx_in ON tx_in.tx_in_id = spending_tx.id
    JOIN tx_out ON tx_out.tx_id = tx_in.tx_out_id
                AND tx_out.index = tx_in.tx_out_index
    JOIN tx ON tx_out.tx_id = tx.id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE spending_block.block_no >= $2 AND spending_block.block_no <= $4
    AND ma_tx_out.ident = $1
    AND (spending_block.block_no > $2 OR (spending_block.block_no = $2 AND spending_tx.block_index >= $3))
    AND (spending_block.block_no < $4 OR (spending_block.block_no = $4 AND spending_tx.block_index < $5))
    AND spending_tx.id >= $8 AND spending_tx.id <=$9
    AND tx_in.id >= $10 AND tx_in.id <= $11
ORDER BY spending_block.block_no, spending_tx.block_index, tx_out.index
LIMIT $6 OFFSET $7;
    "#,
		ident,
		query.start.block_number as i32,
		query.start.tx_index_in_block as i32,
		query.end.block_number as i32,
		query.end.tx_index_in_block as i32,
		query.limit as i32,
		query.offset as i32,
		query.low_bound.tx_id,
		query.high_bound.tx_id,
		query.low_bound.tx_in_id,
		query.high_bound.tx_in_id,
	)
	.fetch_all(pool)
	.await
}

async fn index_exists(pool: &Pool<Postgres>, index_name: &str) -> Result<bool, sqlx::Error> {
	sqlx::query("select * from pg_indexes where indexname = $1")
		.bind(index_name)
		.fetch_all(pool)
		.await
		.map(|rows| rows.len() == 1)
}

async fn create_index_if_not_exists(
	pool: &Pool<Postgres>,
	index_name: &str,
	sql: &str,
) -> Result<(), sqlx::Error> {
	if index_exists(pool, index_name).await? {
		info!("Index '{index_name}' already exists");
	} else {
		info!("Creating index '{index_name}', this might take a while...");
		sqlx::query(sql).execute(pool).await?;
		info!("Index '{index_name}' has been created");
	}
	Ok(())
}

/// Creates indexes that optimize the cNight observation queries.
/// These are critical for genesis generation performance when scanning
/// the full Cardano blockchain for registration/asset events.
pub async fn create_cnight_observation_indexes(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
	// For registrations & deregistrations: filter on tx_out.address
	create_index_if_not_exists(
		pool,
		"idx_tx_out_address",
		"CREATE INDEX IF NOT EXISTS idx_tx_out_address ON tx_out USING hash (address)",
	)
	.await?;

	// For asset creates & spends: filter on multi_asset(policy, name)
	create_index_if_not_exists(
		pool,
		"idx_multi_asset_policy_name",
		"CREATE INDEX IF NOT EXISTS idx_multi_asset_policy_name ON multi_asset(policy, name)",
	)
	.await?;

	// For ma_tx_out joins: composite index on (tx_out_id, ident) to efficiently join
	// from tx_out into ma_tx_out and resolve the multi_asset foreign key in a single lookup,
	// avoiding a full scan over ~1 billion rows.
	create_index_if_not_exists(
		pool,
		"idx_ma_tx_out_id_ident",
		"CREATE INDEX IF NOT EXISTS idx_ma_tx_out_id_ident ON ma_tx_out(tx_out_id, ident)",
	)
	.await?;

	// For block range scans
	create_index_if_not_exists(
		pool,
		"idx_block_block_no",
		"CREATE INDEX IF NOT EXISTS idx_block_block_no ON block(block_no)",
	)
	.await?;

	// For tx joins on block_id
	create_index_if_not_exists(
		pool,
		"idx_tx_block_id",
		"CREATE INDEX IF NOT EXISTS idx_tx_block_id ON tx(block_id)",
	)
	.await?;

	// For tx_out joins on tx_id
	create_index_if_not_exists(
		pool,
		"idx_tx_out_tx_id",
		"CREATE INDEX IF NOT EXISTS idx_tx_out_tx_id ON tx_out(tx_id)",
	)
	.await?;

	// For datum joins on data_hash
	create_index_if_not_exists(
		pool,
		"idx_tx_out_data_hash",
		"CREATE INDEX IF NOT EXISTS idx_tx_out_data_hash ON tx_out(data_hash)",
	)
	.await?;

	// For deregistration/spend joins on tx_in
	create_index_if_not_exists(
		pool,
		"idx_tx_in_tx_in_id",
		"CREATE INDEX IF NOT EXISTS idx_tx_in_tx_in_id ON tx_in(tx_in_id)",
	)
	.await?;

	// For tx_in joins on (tx_out_id, tx_out_index)
	create_index_if_not_exists(
		pool,
		"idx_tx_in_tx_out_id_tx_out_index",
		"CREATE INDEX IF NOT EXISTS idx_tx_in_tx_out_id_tx_out_index ON tx_in(tx_out_id, tx_out_index)",
	)
	.await?;

	Ok(())
}

/// Lower autovacuum_analyze_scale_factor on the cardano-db-sync hot tables that
/// midnight-node queries. Postgres's default of 0.1 means autoanalyze only fires after
/// 10% row growth — for append-heavy multi-million-row tables (tx_out, ma_tx_out, etc.)
/// that threshold takes weeks to hit, so the planner runs on weeks-stale statistics and
/// picks bad join orders for high-cardinality lookups (observed ~430s queries on a
/// preview/preprod cnight observation lookup against an otherwise idle DB).
///
/// Lowering to 0.01 keeps stats fresh as db-sync ingests blocks. Idempotent.
pub async fn apply_cnight_observation_autovacuum_tuning(
	pool: &Pool<Postgres>,
) -> Result<(), sqlx::Error> {
	const TABLES: &[&str] = &["block", "tx", "tx_out", "tx_in", "ma_tx_out", "datum"];
	for table in TABLES {
		info!("Applying autovacuum tuning to '{table}'");
		let sql = format!(
			"ALTER TABLE {table} SET (autovacuum_analyze_scale_factor = 0.01, autovacuum_vacuum_scale_factor = 0.05)"
		);
		sqlx::query(&sql).execute(pool).await?;
	}
	Ok(())
}

/// Query to get the block by its hash
pub(crate) async fn get_block_by_hash(
	pool: &Pool<Postgres>,
	hash: McBlockHash,
) -> Result<Option<Block>, SqlxError> {
	sqlx::query_as::<_, Block>(
		r#"
SELECT
    block_no AS block_number,
    hash AS hash,
    epoch_no AS epoch_number,
    slot_no AS slot_number,
    time,
    tx_count
FROM block
WHERE hash = $1
"#,
	)
	.bind(hash.0)
	.fetch_optional(pool)
	.await
}

/// Gets coarse bounds of table ids.
/// Guarantees:
/// * tx_id belongs to a transaction made before given block
/// * tx_out_id belongs to transaction output of transaction from the previous step
/// * ma_tx_out_id belongs to an multi asset transaction output that was created not after the transaction output of the previous step
pub async fn get_low_bounds(
	pool: &Pool<Postgres>,
	block_no: i64,
) -> Result<Option<QueryBounds>, SqlxError> {
	sqlx::query_as!(
		QueryBounds,
		r#"
SELECT
    low_tx.tx_id AS "tx_id!",
    low_tx_out.tx_out_id AS "tx_out_id!",
    low_ma_tx_out.ma_tx_out_id AS "ma_tx_out_id!",
    low_tx_in.tx_in_id AS "tx_in_id!"
FROM
    (SELECT COALESCE ((SELECT id FROM block WHERE block_no = $1 LIMIT 1), 0) AS id) AS block,
    LATERAL (SELECT COALESCE((SELECT id FROM tx WHERE block_id < block.id ORDER BY block_id DESC LIMIT 1), 0) AS tx_id) AS low_tx,
    LATERAL (SELECT COALESCE((SELECT id FROM tx_out WHERE tx_id <= low_tx.tx_id ORDER BY tx_id DESC LIMIT 1), 0) AS tx_out_id) AS low_tx_out,
    LATERAL (SELECT COALESCE((SELECT id FROM ma_tx_out WHERE tx_out_id <= low_tx_out.tx_out_id ORDER BY tx_out_id DESC LIMIT 1), 0) AS ma_tx_out_id) AS low_ma_tx_out,
    LATERAL (SELECT COALESCE((SELECT id FROM tx_in WHERE tx_in.tx_in_id <= low_tx.tx_id ORDER BY tx_in_id DESC LIMIT 1), 0) AS tx_in_id) AS low_tx_in;
"#,
		block_no as i32,
	)
	.fetch_optional(pool)
	.await
}

/// Gets coarse bounds of table ids.
/// Guarantees:
/// * tx_id belongs to a transaction made after given block
/// * tx_out_id belongs to transaction output of transaction from the previous step
/// * ma_tx_out_id belongs to an multi asset transaction output that was created not before the transaction output of the previous step
pub async fn get_high_bounds(
	pool: &Pool<Postgres>,
	block_no: i64,
) -> Result<Option<QueryBounds>, SqlxError> {
	// 9223372036854775807 is 2^63-1, the max value of Postgres 'bigint' and Rust 'i64'
	sqlx::query_as!(
		QueryBounds,
		r#"
SELECT
    high_tx.tx_id AS "tx_id!",
    high_tx_out.tx_out_id AS "tx_out_id!",
    high_ma_tx_out.ma_tx_out_id AS "ma_tx_out_id!",
    high_tx_in.tx_in_id AS "tx_in_id!"
FROM
    (SELECT id FROM block WHERE block_no = $1 LIMIT 1) AS block,
    LATERAL (SELECT COALESCE((SELECT id FROM tx WHERE block_id > block.id ORDER BY block_id ASC LIMIT 1), 9223372036854775807) AS tx_id) AS high_tx,
    LATERAL (SELECT COALESCE((SELECT id FROM tx_out WHERE tx_id >= high_tx.tx_id ORDER BY tx_id ASC LIMIT 1), 9223372036854775807) AS tx_out_id) AS high_tx_out,
    LATERAL (SELECT COALESCE((SELECT id FROM ma_tx_out WHERE tx_out_id >= high_tx_out.tx_out_id ORDER BY tx_out_id ASC LIMIT 1), 9223372036854775807) AS ma_tx_out_id) AS high_ma_tx_out,
    LATERAL (SELECT COALESCE((SELECT id FROM tx_in WHERE tx_in.tx_in_id >= high_tx.tx_id ORDER BY tx_in_id ASC LIMIT 1), 9223372036854775807) AS tx_in_id) AS high_tx_in;
"#,
		block_no as i32,
	)
	.fetch_optional(pool)
	.await
}

/// Highest stable `block_no` present in db-sync that does not exceed `upper`.
///
/// Stability is the same coarse block-number bound used by the mainchain
/// follower: latest db-sync block minus `cardano_security_parameter +
/// block_stability_margin`. This keeps cNIGHT refresh lookahead out of the
/// rollback-prone tail of Cardano while still clamping to an existing block.
pub async fn get_highest_stable_block_le(
	pool: &Pool<Postgres>,
	upper: u32,
	stability_margin: u32,
) -> Result<Option<u32>, SqlxError> {
	let latest: Option<i64> = sqlx::query_scalar("SELECT max(block_no)::bigint FROM block")
		.fetch_one(pool)
		.await?;
	let Some(latest) = latest else {
		return Ok(None);
	};
	let latest = u32::try_from(latest).unwrap_or(u32::MAX);
	let stable_upper = latest.saturating_sub(stability_margin);
	let bounded_upper = upper.min(stable_upper);

	// Cast to bigint so we decode INT8 regardless of the db-sync `block_no`
	// column width (it is INT4 on current schemas).
	let max: Option<i64> =
		sqlx::query_scalar("SELECT max(block_no)::bigint FROM block WHERE block_no <= $1")
			.bind(i64::from(bounded_upper))
			.fetch_one(pool)
			.await?;
	Ok(max.map(|n| n as u32))
}

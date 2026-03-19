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
use crate::db::{AssetCreateRow, AssetSpendRow, Block, DeregistrationRow, RegistrationRow};
use log::info;
use midnight_primitives_cnight_observation::CardanoPosition;
use sidechain_domain::*;
use sqlx::{Pool, Postgres, error::Error as SqlxError};

#[allow(clippy::too_many_arguments)]
pub async fn get_registrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	auth_token_ident: i64,
	start: &CardanoPosition,
	end: &CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<RegistrationRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	sqlx::query_as::<_, RegistrationRow>(
		r#"
SELECT
    datum.value::jsonb AS full_datum,
    block.block_no AS block_number,
    block.hash AS block_hash,
    block.time AS block_timestamp,
    tx.block_index AS tx_index_in_block,
    tx.hash AS tx_hash,
    tx_out.index AS utxo_index
FROM block
    JOIN tx ON tx.block_id = block.id
    JOIN tx_out ON tx_out.tx_id = tx.id
    JOIN datum ON tx_out.data_hash = datum.hash
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE block.block_no >= $3 AND block.block_no <= $5
    AND tx_out.address = $1
    AND ma_tx_out.ident = $2
    AND ma_tx_out.quantity = 1
    AND (block.block_no > $3 OR (block.block_no = $3 AND tx.block_index >= $4))
    AND (block.block_no < $5 OR (block.block_no = $5 AND tx.block_index < $6))
ORDER BY block.block_no, tx.block_index
LIMIT $7 OFFSET $8;
        "#,
	)
	.bind(smart_contract_address)
	.bind(auth_token_ident)
	.bind(start.block_number as i32)
	.bind(start.tx_index_in_block as i32)
	.bind(end.block_number as i32)
	.bind(end.tx_index_in_block as i32)
	.bind(limit as i32)
	.bind(offset as i32)
	.fetch_all(pool)
	.await
}

pub async fn get_deregistrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	start: &CardanoPosition,
	end: &CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<DeregistrationRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
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
ORDER BY block.block_no, tx.block_index
LIMIT $6 OFFSET $7;
        "#,
		smart_contract_address,
		start.block_number as i32,
		start.tx_index_in_block as i32,
		end.block_number as i32,
		end.tx_index_in_block as i32,
		limit as i32,
		offset as i32
	)
	.fetch_all(pool)
	.await
}

pub(crate) async fn get_asset_creates(
	pool: &Pool<Postgres>,
	ident: i64,
	start: &CardanoPosition,
	end: &CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<AssetCreateRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	sqlx::query_as::<_, AssetCreateRow>(
		r#"
SELECT
    block.block_no AS block_number,
    block.hash AS block_hash,
    block.time AS block_timestamp,
    tx.block_index AS tx_index_in_block,
    ma_tx_out.quantity::BIGINT AS quantity,
    tx_out.address AS holder_address,
    tx.hash AS tx_hash,
    tx_out.index AS utxo_index
FROM block
    JOIN tx ON tx.block_id = block.id
    JOIN tx_out ON tx_out.tx_id = tx.id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE block.block_no >= $2 AND block.block_no <= $4
    AND ma_tx_out.ident = $1
    AND (block.block_no > $2 OR (block.block_no = $2 AND tx.block_index >= $3))
    AND (block.block_no < $4 OR (block.block_no = $4 AND tx.block_index < $5))
ORDER BY block.block_no, tx.block_index, tx_out.index
LIMIT $6 OFFSET $7;
    "#,
	)
	.bind(ident)
	.bind(start.block_number as i32)
	.bind(start.tx_index_in_block as i32)
	.bind(end.block_number as i32)
	.bind(end.tx_index_in_block as i32)
	.bind(limit as i32)
	.bind(offset as i32)
	.fetch_all(pool)
	.await
}

pub(crate) async fn get_asset_spends(
	pool: &Pool<Postgres>,
	ident: i64,
	start: &CardanoPosition,
	end: &CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<AssetSpendRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	sqlx::query_as::<_, AssetSpendRow>(
		r#"
SELECT
    spending_block.block_no AS block_number,
    spending_block.hash AS block_hash,
    spending_block.time AS block_timestamp,
    spending_tx.block_index AS tx_index_in_block,
    ma_tx_out.quantity::BIGINT AS quantity,
    tx_out.address AS holder_address,
    tx.hash AS utxo_tx_hash,
    tx_out.index AS utxo_index,
    spending_tx.hash AS spending_tx_hash
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
ORDER BY spending_block.block_no, spending_tx.block_index, tx_out.index
LIMIT $6 OFFSET $7;
    "#,
	)
	.bind(ident)
	.bind(start.block_number as i32)
	.bind(start.tx_index_in_block as i32)
	.bind(end.block_number as i32)
	.bind(end.tx_index_in_block as i32)
	.bind(limit as i32)
	.bind(offset as i32)
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

/// Query to get the block by its hash
pub(crate) async fn get_block_by_hash(
	pool: &Pool<Postgres>,
	hash: McBlockHash,
) -> Result<Option<Block>, SqlxError> {
	sqlx::query_as!(
		Block,
		r#"
SELECT 
    block_no as "block_number!: _", 
    hash as "hash: _",
    epoch_no as "epoch_number!: _",
    slot_no as "slot_number!: _", 
    time,
    tx_count
FROM block
WHERE hash = $1
"#,
		&hash.0
	)
	.fetch_optional(pool)
	.await
}

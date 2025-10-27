// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
	AssetCreateRow, AssetSpendRow, Block, DeregistrationRow, RedemptionCreateRow,
	RedemptionSpendRow, RegistrationRow,
};
use midnight_primitives_cnight_observation::CardanoPosition;
use sidechain_domain::*;
use sqlx::{Pool, Postgres, error::Error as SqlxError};

// TODO: Compare bytes not hex strings for policy_id and asset_name

#[allow(clippy::too_many_arguments)]
pub async fn get_redemption_creates(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	policy_id: [u8; 28],
	asset_name: &[u8],
	start: CardanoPosition,
	end: CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<RedemptionCreateRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	sqlx::query_as!(
		RedemptionCreateRow,
		r#"
SELECT
    datum.value::jsonb AS "full_datum!: _",
    block.block_no as "block_number!: _",
    block.hash as "block_hash: _",
    block.time as "block_timestamp: _",
    tx.block_index as "tx_index_in_block: _",
    tx.hash AS "tx_hash: _",
    tx_out.index AS "utxo_index: _",
    ma_tx_out.quantity::BIGINT AS "quantity!: _"
FROM tx_out
    JOIN datum ON tx_out.data_hash = datum.hash
    JOIN tx ON tx.id = tx_out.tx_id
    JOIN block ON block.id = tx.block_id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
    JOIN multi_asset ma ON ma.id = ma_tx_out.ident
WHERE tx_out.address = $1
    AND ma.policy = $2
    AND ma.name = $3
    AND (block.block_no, tx.block_index) >= ($4, $5)
    AND (block.block_no, tx.block_index) < ($6, $7)
ORDER BY block.block_no, tx.block_index
LIMIT $8 OFFSET $9;
        "#,
		smart_contract_address,
		&policy_id,
		asset_name,
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

#[allow(clippy::too_many_arguments)]
pub async fn get_redemption_spends(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	policy_id: [u8; 28],
	asset_name: &[u8],
	start: CardanoPosition,
	end: CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<RedemptionSpendRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	// NOTE: Ordered by transaction index (i.e. index of transaction within block)
	// Once one valid deregistration can occur in a single tx, so we don't have to worry about
	// ordering within txs

	sqlx::query_as!(
		RedemptionSpendRow,
		r#"
SELECT 
    datum.value::jsonb AS "full_datum!: _",
    block.block_no as "block_number!: _",
    block.hash as "block_hash: _",
    block.time as "block_timestamp: _",
    tx.block_index as "tx_index_in_block: _",
    tx.hash AS "tx_hash: _",
    tx_tx_out.hash as "utxo_tx_hash: _",
    tx_out.index as "utxo_index: _",
    ma_tx_out.quantity::BIGINT AS "quantity!: _"
FROM tx_out
    JOIN tx_in ON tx_in.tx_out_id = tx_out.tx_id
                AND tx_in.tx_out_index = tx_out.index
    JOIN tx ON tx.id = tx_in.tx_in_id
    JOIN tx as tx_tx_out ON tx_out.tx_id = tx_tx_out.id
    JOIN datum ON datum.hash = tx_out.data_hash
    JOIN block ON block.id = tx.block_id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
    JOIN multi_asset ma ON ma.id = ma_tx_out.ident
WHERE tx_out.address = $1
    AND ma.policy = $2
    AND ma.name = $3
    AND (block.block_no, tx.block_index) >= ($4, $5)
    AND (block.block_no, tx.block_index) < ($6, $7)
ORDER BY block.block_no, tx.block_index
LIMIT $8 OFFSET $9;
        "#,
		smart_contract_address,
		&policy_id,
		asset_name,
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

pub async fn get_registrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	start: CardanoPosition,
	end: CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<RegistrationRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	sqlx::query_as!(
		RegistrationRow,
		r#"
SELECT
    datum.value::jsonb AS "full_datum!: _",
    block.block_no as "block_number!: _",
    block.hash as "block_hash: _",
    block.time as "block_timestamp: _",
    tx.block_index as "tx_index_in_block: _",
    tx.hash AS "tx_hash: _",
    tx_out.index AS "utxo_index: _"
FROM tx_out
    JOIN datum ON tx_out.data_hash = datum.hash
    JOIN tx ON tx.id = tx_out.tx_id
    JOIN block ON block.id = tx.block_id
WHERE tx_out.address = $1
    AND (block.block_no, tx.block_index) >= ($2, $3)
    AND (block.block_no, tx.block_index) < ($4, $5)
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

pub async fn get_deregistrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	start: CardanoPosition,
	end: CardanoPosition,
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
FROM tx_out
    JOIN tx_in ON tx_in.tx_out_id = tx_out.tx_id
                AND tx_in.tx_out_index = tx_out.index
    JOIN tx ON tx.id = tx_in.tx_in_id
    JOIN tx as tx_tx_out ON tx_out.tx_id = tx_tx_out.id
    JOIN datum ON datum.hash = tx_out.data_hash
    JOIN block ON block.id = tx.block_id
WHERE tx_out.address = $1
    AND (block.block_no, tx.block_index) >= ($2, $3)
    AND (block.block_no, tx.block_index) < ($4, $5)
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
	policy_id: [u8; 28],
	asset_name: &[u8],
	start: CardanoPosition,
	end: CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<AssetCreateRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	sqlx::query_as!(
		AssetCreateRow,
		r#"
SELECT
    block.block_no as "block_number!: _",
    block.hash as "block_hash: _",
    block.time as "block_timestamp: _",
    tx.block_index as "tx_index_in_block: _",
    ma_tx_out.quantity::BIGINT AS "quantity!: _",
    tx_out.address AS "holder_address: _",
    tx.hash AS "tx_hash: _",
    tx_out.index AS "utxo_index: _"
FROM ma_tx_out
    JOIN multi_asset ma ON ma.id = ma_tx_out.ident
    JOIN tx_out ON tx_out.id = ma_tx_out.tx_out_id
    JOIN tx ON tx_out.tx_id = tx.id
    JOIN block ON tx.block_id = block.id
WHERE ma.policy = $1
    AND ma.name = $2
    AND (block.block_no, tx.block_index) >= ($3, $4)
    AND (block.block_no, tx.block_index) < ($5, $6)
ORDER BY block.block_no, tx.block_index, tx_out.index
LIMIT $7 OFFSET $8;
    "#,
		&policy_id,
		asset_name,
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

pub(crate) async fn get_asset_spends(
	pool: &Pool<Postgres>,
	policy_id: [u8; 28],
	asset_name: &[u8],
	start: CardanoPosition,
	end: CardanoPosition,
	limit: usize,
	offset: usize,
) -> Result<Vec<AssetSpendRow>, SqlxError> {
	assert!(limit < i32::MAX as usize);
	assert!(offset < i32::MAX as usize);
	let rows = sqlx::query_as!(
		AssetSpendRow,
		r#"
SELECT
    spending_block.block_no as "block_number!: _",
    spending_block.hash as "block_hash: _",
    spending_block.time as block_timestamp,
    spending_tx.block_index as "tx_index_in_block: _",
    ma_tx_out.quantity::BIGINT AS "quantity!: _",
    tx_out.address AS "holder_address: _",
    tx.hash AS "utxo_tx_hash: _",
    tx_out.index AS "utxo_index: _",
    spending_tx.hash AS "spending_tx_hash: _"
FROM ma_tx_out
    JOIN multi_asset ma ON ma.id = ma_tx_out.ident
    JOIN tx_out ON tx_out.id = ma_tx_out.tx_out_id
    JOIN tx_in ON tx_out.tx_id = tx_in.tx_out_id 
                AND tx_out.index = tx_in.tx_out_index
    JOIN tx ON tx_out.tx_id = tx.id
    JOIN tx AS spending_tx ON tx_in.tx_in_id = spending_tx.id
    JOIN block AS spending_block ON spending_tx.block_id = spending_block.id
WHERE ma.policy = $1
    AND ma.name = $2
    AND (spending_block.block_no, spending_tx.block_index) >= ($3, $4)
    AND (spending_block.block_no, spending_tx.block_index) < ($5, $6)
ORDER BY spending_block.block_no, spending_tx.block_index, tx_out.index
LIMIT $7 OFFSET $8;
    "#,
		&policy_id,
		asset_name,
		start.block_number as i32,
		start.tx_index_in_block as i32,
		end.block_number as i32,
		end.tx_index_in_block as i32,
		limit as i32,
		offset as i32
	)
	.fetch_all(pool)
	.await?;

	Ok(rows)
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

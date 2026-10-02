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
use db_sync_sqlx::{
	DbSyncIndexSpec, DbSyncQueryConfig, DbSyncSchemaMode, ResolvedDbSyncAddressMode,
	ResolvedDbSyncQueryConfig, ResolvedDbSyncTxInputMode, manage_indexes,
};
use log::{info, warn};
use sidechain_domain::*;
use sqlx::{Pool, Postgres, error::Error as SqlxError};

pub async fn get_registrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	auth_token_ident: i64,
	query: &PagedQuery<'_>,
	config: ResolvedDbSyncQueryConfig,
) -> Result<Vec<RegistrationRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	let (address_join, address_column) = address_query_parts(config.address_mode);
	let sql = format!(
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
    {address_join}
    JOIN datum ON tx_out.data_hash = datum.hash
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE tx.id >= $9 AND tx.id <= $10
    AND tx_out.id >= $11 AND tx_out.id <= $12
    AND ma_tx_out.id >= $13 AND ma_tx_out.id <= $14
    AND block.block_no >= $3 AND block.block_no <= $5
    AND {address_column} = $1
    AND ma_tx_out.ident = $2
    AND ma_tx_out.quantity = 1
    AND (block.block_no > $3 OR (block.block_no = $3 AND tx.block_index >= $4))
    AND (block.block_no < $5 OR (block.block_no = $5 AND tx.block_index < $6))
ORDER BY block.block_no, tx.block_index
LIMIT $7 OFFSET $8;
	"#
	);
	sqlx::query_as::<_, RegistrationRow>(&sql)
		.bind(smart_contract_address)
		.bind(auth_token_ident)
		.bind(query.start.block_number as i32)
		.bind(query.start.tx_index_in_block as i32)
		.bind(query.end.block_number as i32)
		.bind(query.end.tx_index_in_block as i32)
		.bind(query.limit as i32)
		.bind(query.offset as i32)
		.bind(query.low_bound.tx_id)
		.bind(query.high_bound.tx_id)
		.bind(query.low_bound.tx_out_id)
		.bind(query.high_bound.tx_out_id)
		.bind(query.low_bound.ma_tx_out_id)
		.bind(query.high_bound.ma_tx_out_id)
		.fetch_all(pool)
		.await
}

pub async fn get_deregistrations(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	query: &PagedQuery<'_>,
	config: ResolvedDbSyncQueryConfig,
) -> Result<Vec<DeregistrationRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	// NOTE: Ordered by transaction index (i.e. index of transaction within block)
	// Once one valid deregistration can occur in a single tx, so we don't have to worry about
	// ordering within txs

	let (address_join, address_column) = address_query_parts(config.address_mode);
	let (input_join, input_bound) = match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => (
			"JOIN tx_in ON tx_in.tx_in_id = tx.id\n    JOIN tx_out ON tx_out.tx_id = tx_in.tx_out_id AND tx_out.index = tx_in.tx_out_index",
			"AND tx_in.id >= $10 AND tx_in.id <= $11",
		),
		ResolvedDbSyncTxInputMode::Consumed => {
			("JOIN tx_out ON tx_out.consumed_by_tx_id = tx.id", "")
		},
	};
	let sql = format!(
		r#"
SELECT
    datum.value::jsonb AS full_datum,
    block.block_no as block_number,
    block.hash as block_hash,
    block.time as block_timestamp,
    tx.block_index as tx_index_in_block,
    tx.hash AS tx_hash,
    tx_tx_out.hash as utxo_tx_hash,
    tx_out.index as utxo_index
FROM block
    JOIN tx ON tx.block_id = block.id
    {input_join}
    {address_join}
    JOIN tx as tx_tx_out ON tx_out.tx_id = tx_tx_out.id
    JOIN datum ON datum.hash = tx_out.data_hash
WHERE block.block_no >= $2 AND block.block_no <= $4
    AND {address_column} = $1
    AND (block.block_no > $2 OR (block.block_no = $2 AND tx.block_index >= $3))
    AND (block.block_no < $4 OR (block.block_no = $4 AND tx.block_index < $5))
    AND tx.id >= $8 AND tx.id <=$9
    {input_bound}
ORDER BY block.block_no, tx.block_index
LIMIT $6 OFFSET $7;
	"#
	);
	let query_builder = sqlx::query_as::<_, DeregistrationRow>(&sql)
		.bind(smart_contract_address)
		.bind(query.start.block_number as i32)
		.bind(query.start.tx_index_in_block as i32)
		.bind(query.end.block_number as i32)
		.bind(query.end.tx_index_in_block as i32)
		.bind(query.limit as i32)
		.bind(query.offset as i32)
		.bind(query.low_bound.tx_id)
		.bind(query.high_bound.tx_id);
	let query_builder = match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => {
			query_builder.bind(query.low_bound.tx_in_id).bind(query.high_bound.tx_in_id)
		},
		ResolvedDbSyncTxInputMode::Consumed => query_builder,
	};
	query_builder.fetch_all(pool).await
}

pub(crate) async fn get_asset_creates(
	pool: &Pool<Postgres>,
	ident: i64,
	query: &PagedQuery<'_>,
	config: ResolvedDbSyncQueryConfig,
) -> Result<Vec<AssetCreateRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	let (address_join, address_column) = address_query_parts(config.address_mode);
	let sql = format!(
		r#"
SELECT
    block.block_no AS block_number,
    block.hash AS block_hash,
    block.time AS block_timestamp,
    tx.block_index AS tx_index_in_block,
    ma_tx_out.quantity::BIGINT AS quantity,
    {address_column} AS holder_address,
    tx.hash AS tx_hash,
    tx_out.index AS utxo_index
FROM block
    JOIN tx ON tx.block_id = block.id
    JOIN tx_out ON tx_out.tx_id = tx.id
    {address_join}
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
	"#
	);
	sqlx::query_as::<_, AssetCreateRow>(&sql)
		.bind(ident)
		.bind(query.start.block_number as i32)
		.bind(query.start.tx_index_in_block as i32)
		.bind(query.end.block_number as i32)
		.bind(query.end.tx_index_in_block as i32)
		.bind(query.limit as i32)
		.bind(query.offset as i32)
		.bind(query.low_bound.tx_id)
		.bind(query.high_bound.tx_id)
		.bind(query.low_bound.tx_out_id)
		.bind(query.high_bound.tx_out_id)
		.bind(query.low_bound.ma_tx_out_id)
		.bind(query.high_bound.ma_tx_out_id)
		.fetch_all(pool)
		.await
}

pub(crate) async fn get_asset_spends(
	pool: &Pool<Postgres>,
	ident: i64,
	query: &PagedQuery<'_>,
	config: ResolvedDbSyncQueryConfig,
) -> Result<Vec<AssetSpendRow>, SqlxError> {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	let (address_join, address_column) = address_query_parts(config.address_mode);
	let (input_join, input_bound) = match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => (
			"JOIN tx_in ON tx_in.tx_in_id = spending_tx.id\n    JOIN tx_out ON tx_out.tx_id = tx_in.tx_out_id AND tx_out.index = tx_in.tx_out_index",
			"AND tx_in.id >= $10 AND tx_in.id <= $11",
		),
		ResolvedDbSyncTxInputMode::Consumed => {
			("JOIN tx_out ON tx_out.consumed_by_tx_id = spending_tx.id", "")
		},
	};
	let sql = format!(
		r#"
SELECT
    spending_block.block_no AS block_number,
    spending_block.hash AS block_hash,
    spending_block.time AS block_timestamp,
    spending_tx.block_index AS tx_index_in_block,
    ma_tx_out.quantity::BIGINT AS quantity,
    {address_column} AS holder_address,
    tx.hash AS utxo_tx_hash,
    tx_out.index AS utxo_index,
    spending_tx.hash AS spending_tx_hash
FROM block AS spending_block
    JOIN tx AS spending_tx ON spending_tx.block_id = spending_block.id
    {input_join}
    {address_join}
    JOIN tx ON tx_out.tx_id = tx.id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
WHERE spending_block.block_no >= $2 AND spending_block.block_no <= $4
    AND ma_tx_out.ident = $1
    AND (spending_block.block_no > $2 OR (spending_block.block_no = $2 AND spending_tx.block_index >= $3))
    AND (spending_block.block_no < $4 OR (spending_block.block_no = $4 AND spending_tx.block_index < $5))
    AND spending_tx.id >= $8 AND spending_tx.id <=$9
    {input_bound}
ORDER BY spending_block.block_no, spending_tx.block_index, tx_out.index
LIMIT $6 OFFSET $7;
	"#
	);
	let query_builder = sqlx::query_as::<_, AssetSpendRow>(&sql)
		.bind(ident)
		.bind(query.start.block_number as i32)
		.bind(query.start.tx_index_in_block as i32)
		.bind(query.end.block_number as i32)
		.bind(query.end.tx_index_in_block as i32)
		.bind(query.limit as i32)
		.bind(query.offset as i32)
		.bind(query.low_bound.tx_id)
		.bind(query.high_bound.tx_id);
	let query_builder = match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => {
			query_builder.bind(query.low_bound.tx_in_id).bind(query.high_bound.tx_in_id)
		},
		ResolvedDbSyncTxInputMode::Consumed => query_builder,
	};
	query_builder.fetch_all(pool).await
}

fn address_query_parts(address_mode: ResolvedDbSyncAddressMode) -> (&'static str, &'static str) {
	match address_mode {
		ResolvedDbSyncAddressMode::Inline => ("", "tx_out.address"),
		ResolvedDbSyncAddressMode::AddressTable => (
			"JOIN address tx_out_address ON tx_out_address.id = tx_out.address_id",
			"tx_out_address.address",
		),
	}
}

fn cnight_index_specs(config: ResolvedDbSyncQueryConfig) -> Vec<DbSyncIndexSpec> {
	let mut indexes = vec![
		DbSyncIndexSpec {
			name: "idx_ma_tx_out_ident",
			relation: "ma_tx_out",
			access_methods: &["btree"],
			keys: &["ident"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ma_tx_out_ident ON ma_tx_out(ident)",
		},
		DbSyncIndexSpec {
			name: "idx_multi_asset_policy_name",
			relation: "multi_asset",
			access_methods: &["btree"],
			keys: &["policy", "name"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_multi_asset_policy_name ON multi_asset(policy, name)",
		},
		DbSyncIndexSpec {
			name: "idx_ma_tx_out_id_ident",
			relation: "ma_tx_out",
			access_methods: &["btree"],
			keys: &["tx_out_id"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ma_tx_out_id_ident ON ma_tx_out(tx_out_id, ident)",
		},
		DbSyncIndexSpec {
			name: "idx_block_block_no",
			relation: "block",
			access_methods: &["btree"],
			keys: &["block_no"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_block_block_no ON block(block_no)",
		},
		DbSyncIndexSpec {
			name: "idx_tx_block_id",
			relation: "tx",
			access_methods: &["btree"],
			keys: &["block_id"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_block_id ON tx(block_id)",
		},
		DbSyncIndexSpec {
			name: "idx_tx_out_tx_id",
			relation: "tx_out",
			access_methods: &["btree"],
			keys: &["tx_id"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_tx_id ON tx_out(tx_id)",
		},
	];

	match config.address_mode {
		ResolvedDbSyncAddressMode::Inline => indexes.push(DbSyncIndexSpec {
			name: "idx_tx_out_address",
			relation: "tx_out",
			access_methods: &["hash", "btree"],
			keys: &["address"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_address ON tx_out USING hash(address)",
		}),
		ResolvedDbSyncAddressMode::AddressTable => indexes.extend([
			DbSyncIndexSpec {
				name: "idx_address_address",
				relation: "address",
				access_methods: &["hash", "btree"],
				keys: &["address"],
				create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_address_address ON address USING hash(address)",
			},
			DbSyncIndexSpec {
				name: "idx_tx_out_address_id",
				relation: "tx_out",
				access_methods: &["btree"],
				keys: &["address_id"],
				create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_address_id ON tx_out(address_id)",
			},
		]),
	}

	match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => indexes.extend([
			DbSyncIndexSpec {
				name: "idx_tx_in_tx_in_id",
				relation: "tx_in",
				access_methods: &["btree"],
				keys: &["tx_in_id"],
				create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_in_tx_in_id ON tx_in(tx_in_id)",
			},
			DbSyncIndexSpec {
				name: "idx_tx_in_tx_out_id_tx_out_index",
				relation: "tx_in",
				access_methods: &["btree"],
				keys: &["tx_out_id", "tx_out_index"],
				create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_in_tx_out_id_tx_out_index ON tx_in(tx_out_id, tx_out_index)",
			},
		]),
		ResolvedDbSyncTxInputMode::Consumed => indexes.push(DbSyncIndexSpec {
			name: "idx_tx_out_consumed_by_tx_id",
			relation: "tx_out",
			access_methods: &["btree"],
			keys: &["consumed_by_tx_id"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_consumed_by_tx_id ON tx_out(consumed_by_tx_id)",
		}),
	}

	indexes
}

/// Applies or verifies all database optimizations used by cNIGHT genesis observation.
pub async fn manage_cnight_observation_schema(
	pool: &Pool<Postgres>,
	config: ResolvedDbSyncQueryConfig,
	mode: DbSyncSchemaMode,
) -> Result<(), sqlx::Error> {
	manage_indexes(pool, mode, &cnight_index_specs(config)).await?;
	manage_cnight_observation_autovacuum_tuning(pool, config, mode).await
}

/// Backward-compatible helper that applies cNIGHT indexes for the detected legacy layout.
pub async fn create_cnight_observation_indexes(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
	let config = DbSyncQueryConfig::default().resolve(pool).await?;
	manage_indexes(pool, DbSyncSchemaMode::Apply, &cnight_index_specs(config)).await
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
	let config = DbSyncQueryConfig::default().resolve(pool).await?;
	manage_cnight_observation_autovacuum_tuning(pool, config, DbSyncSchemaMode::Apply).await
}

async fn manage_cnight_observation_autovacuum_tuning(
	pool: &Pool<Postgres>,
	config: ResolvedDbSyncQueryConfig,
	mode: DbSyncSchemaMode,
) -> Result<(), sqlx::Error> {
	if mode == DbSyncSchemaMode::Skip {
		warn!("Skipping db-sync autovacuum tuning and verification");
		return Ok(());
	}

	let mut tables = vec!["block", "tx", "tx_out", "ma_tx_out", "datum"];
	if config.tx_input_mode == ResolvedDbSyncTxInputMode::TxIn {
		tables.push("tx_in");
	}
	if config.address_mode == ResolvedDbSyncAddressMode::AddressTable {
		tables.push("address");
	}

	for table in tables {
		if mode == DbSyncSchemaMode::Apply {
			info!("Applying autovacuum tuning to '{table}'");
			let sql = format!(
				"ALTER TABLE {table} SET (autovacuum_analyze_scale_factor = 0.01, autovacuum_vacuum_scale_factor = 0.05)"
			);
			sqlx::query(&sql).execute(pool).await?;
			continue;
		}

		let options = sqlx::query_scalar::<_, Option<Vec<String>>>(
			"SELECT reloptions FROM pg_catalog.pg_class WHERE oid = to_regclass($1)",
		)
		.bind(table)
		.fetch_one(pool)
		.await?
		.unwrap_or_default();
		let analyze_ok =
			options.iter().any(|value| value == "autovacuum_analyze_scale_factor=0.01");
		let vacuum_ok = options.iter().any(|value| value == "autovacuum_vacuum_scale_factor=0.05");
		if !analyze_ok || !vacuum_ok {
			warn!(
				"Table '{table}' does not have Midnight's recommended autovacuum reloptions; cluster-level settings may still be sufficient"
			);
		}
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
	config: ResolvedDbSyncQueryConfig,
) -> Result<Option<QueryBounds>, SqlxError> {
	let (tx_in_select, tx_in_join) = match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => (
			"low_tx_in.tx_in_id AS tx_in_id",
			", LATERAL (SELECT COALESCE((SELECT id FROM tx_in WHERE tx_in.tx_in_id <= low_tx.tx_id ORDER BY tx_in_id DESC LIMIT 1), 0) AS tx_in_id) AS low_tx_in",
		),
		// `tx_in_id` is not used by consumed-mode queries. Keep the existing bounds
		// structure populated without reading a table that may intentionally be empty.
		ResolvedDbSyncTxInputMode::Consumed => ("low_tx.tx_id AS tx_in_id", ""),
	};
	let sql = format!(
		r#"
SELECT
    low_tx.tx_id AS tx_id,
    low_tx_out.tx_out_id AS tx_out_id,
    low_ma_tx_out.ma_tx_out_id AS ma_tx_out_id,
    {tx_in_select}
FROM
    (SELECT COALESCE ((SELECT id FROM block WHERE block_no = $1 LIMIT 1), 0) AS id) AS block,
    LATERAL (SELECT COALESCE((SELECT id FROM tx WHERE block_id < block.id ORDER BY block_id DESC LIMIT 1), 0) AS tx_id) AS low_tx,
    LATERAL (SELECT COALESCE((SELECT id FROM tx_out WHERE tx_id <= low_tx.tx_id ORDER BY tx_id DESC LIMIT 1), 0) AS tx_out_id) AS low_tx_out,
	LATERAL (SELECT COALESCE((SELECT id FROM ma_tx_out WHERE tx_out_id <= low_tx_out.tx_out_id ORDER BY tx_out_id DESC LIMIT 1), 0) AS ma_tx_out_id) AS low_ma_tx_out
	{tx_in_join};
"#
	);
	sqlx::query_as::<_, QueryBounds>(&sql)
		.bind(block_no as i32)
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
	config: ResolvedDbSyncQueryConfig,
) -> Result<Option<QueryBounds>, SqlxError> {
	// 9223372036854775807 is 2^63-1, the max value of Postgres 'bigint' and Rust 'i64'
	let (tx_in_select, tx_in_join) = match config.tx_input_mode {
		ResolvedDbSyncTxInputMode::TxIn => (
			"high_tx_in.tx_in_id AS tx_in_id",
			", LATERAL (SELECT COALESCE((SELECT id FROM tx_in WHERE tx_in.tx_in_id >= high_tx.tx_id ORDER BY tx_in_id ASC LIMIT 1), 9223372036854775807) AS tx_in_id) AS high_tx_in",
		),
		ResolvedDbSyncTxInputMode::Consumed => ("high_tx.tx_id AS tx_in_id", ""),
	};
	let sql = format!(
		r#"
SELECT
    high_tx.tx_id AS tx_id,
    high_tx_out.tx_out_id AS tx_out_id,
    high_ma_tx_out.ma_tx_out_id AS ma_tx_out_id,
    {tx_in_select}
FROM
    (SELECT id FROM block WHERE block_no = $1 LIMIT 1) AS block,
    LATERAL (SELECT COALESCE((SELECT id FROM tx WHERE block_id > block.id ORDER BY block_id ASC LIMIT 1), 9223372036854775807) AS tx_id) AS high_tx,
    LATERAL (SELECT COALESCE((SELECT id FROM tx_out WHERE tx_id >= high_tx.tx_id ORDER BY tx_id ASC LIMIT 1), 9223372036854775807) AS tx_out_id) AS high_tx_out,
	LATERAL (SELECT COALESCE((SELECT id FROM ma_tx_out WHERE tx_out_id >= high_tx_out.tx_out_id ORDER BY tx_out_id ASC LIMIT 1), 9223372036854775807) AS ma_tx_out_id) AS high_ma_tx_out
	{tx_in_join};
"#
	);
	sqlx::query_as::<_, QueryBounds>(&sql)
		.bind(block_no as i32)
		.fetch_optional(pool)
		.await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		data_source::candidates_data_source::get_utxos_for_address, db::get_governance_body_utxo,
	};
	use db_sync_sqlx::{Address, BlockNumber};
	use midnight_primitives_cnight_observation::CardanoPosition;
	use sidechain_domain::{PolicyId, UtxoId, UtxoIndex};
	use sqlx::{PgPool, postgres::PgPoolOptions};
	use testcontainers_modules::{
		postgres::Postgres,
		testcontainers::{ImageExt, runners::AsyncRunner},
	};

	fn paged_query<'a>(start: &'a CardanoPosition, end: &'a CardanoPosition) -> PagedQuery<'a> {
		PagedQuery {
			start,
			end,
			limit: 100,
			offset: 0,
			low_bound: QueryBounds { tx_id: 0, tx_out_id: 0, ma_tx_out_id: 0, tx_in_id: 0 },
			high_bound: QueryBounds {
				tx_id: i64::MAX,
				tx_out_id: i64::MAX,
				ma_tx_out_id: i64::MAX,
				tx_in_id: i64::MAX,
			},
		}
	}

	async fn assert_observations(
		pool: &PgPool,
		query: &PagedQuery<'_>,
		config: ResolvedDbSyncQueryConfig,
	) {
		assert_eq!(get_registrations(pool, "script", 7, query, config).await.unwrap().len(), 1);
		assert_eq!(get_deregistrations(pool, "script", query, config).await.unwrap().len(), 1);
		assert_eq!(get_asset_creates(pool, 7, query, config).await.unwrap().len(), 1);
		let spends = get_asset_spends(pool, 7, query, config).await.unwrap();
		assert_eq!(spends.len(), 1);
		assert_eq!(spends[0].holder_address, "script");
	}

	async fn assert_candidate_and_federated_queries(
		pool: &PgPool,
		config: ResolvedDbSyncQueryConfig,
	) {
		let outputs =
			get_utxos_for_address(pool, &Address("script".to_owned()), BlockNumber(1), config)
				.await
				.unwrap();
		assert_eq!(outputs.len(), 1);
		assert_eq!(outputs[0].address, "script");
		assert_eq!(
			outputs[0].tx_inputs,
			vec![UtxoId { tx_hash: McTxHash([5; 32]), index: UtxoIndex(1) }]
		);

		let governance_utxo =
			get_governance_body_utxo(pool, "script", &PolicyId([0x44; 28]), 1, config)
				.await
				.unwrap()
				.unwrap();
		assert_eq!(governance_utxo.block_number, BlockNumber(1));
		assert_eq!(governance_utxo.tx_hash.0, [0x11; 32]);
	}

	#[test]
	fn cnight_index_manifest_matches_resolved_layout() {
		for tx_input_mode in [ResolvedDbSyncTxInputMode::TxIn, ResolvedDbSyncTxInputMode::Consumed]
		{
			for address_mode in
				[ResolvedDbSyncAddressMode::Inline, ResolvedDbSyncAddressMode::AddressTable]
			{
				let indexes =
					cnight_index_specs(ResolvedDbSyncQueryConfig { tx_input_mode, address_mode });
				let has_keys = |relation, keys: &[&str]| {
					indexes.iter().any(|index| index.relation == relation && index.keys == keys)
				};

				assert_eq!(
					has_keys("tx_in", &["tx_in_id"]),
					tx_input_mode == ResolvedDbSyncTxInputMode::TxIn
				);
				assert_eq!(
					has_keys("tx_in", &["tx_out_id", "tx_out_index"]),
					tx_input_mode == ResolvedDbSyncTxInputMode::TxIn
				);
				assert_eq!(
					has_keys("tx_out", &["consumed_by_tx_id"]),
					tx_input_mode == ResolvedDbSyncTxInputMode::Consumed
				);
				assert_eq!(
					has_keys("tx_out", &["address"]),
					address_mode == ResolvedDbSyncAddressMode::Inline
				);
				assert_eq!(
					has_keys("address", &["address"]),
					address_mode == ResolvedDbSyncAddressMode::AddressTable
				);
				assert_eq!(
					has_keys("tx_out", &["address_id"]),
					address_mode == ResolvedDbSyncAddressMode::AddressTable
				);
			}
		}
	}

	#[tokio::test]
	async fn production_queries_support_all_layout_combinations() {
		let postgres = Postgres::default().with_tag("17.2").start().await.unwrap();
		let database_url = format!(
			"postgres://postgres:postgres@127.0.0.1:{}/postgres",
			postgres.get_host_port_ipv4(5432).await.unwrap()
		);
		let pool = PgPoolOptions::new().connect(&database_url).await.unwrap();
		sqlx::raw_sql(
			r#"
CREATE TABLE block (
    id BIGINT PRIMARY KEY,
    block_no INTEGER NOT NULL,
    hash BYTEA NOT NULL,
    time TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    epoch_no INTEGER NOT NULL,
    slot_no BIGINT NOT NULL,
    tx_count BIGINT NOT NULL
);
CREATE TABLE tx (
    id BIGINT PRIMARY KEY,
    block_id BIGINT NOT NULL,
    block_index INTEGER NOT NULL,
    hash BYTEA NOT NULL
);
CREATE TABLE address (
    id BIGINT PRIMARY KEY,
    address VARCHAR NOT NULL
);
CREATE TABLE datum (
    hash BYTEA PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE tx_out (
    id BIGINT PRIMARY KEY,
    tx_id BIGINT NOT NULL,
    index SMALLINT NOT NULL,
    address VARCHAR NOT NULL,
    address_id BIGINT NOT NULL,
    data_hash BYTEA,
    consumed_by_tx_id BIGINT
);
CREATE TABLE tx_in (
    id BIGINT PRIMARY KEY,
    tx_in_id BIGINT NOT NULL,
    tx_out_id BIGINT NOT NULL,
    tx_out_index SMALLINT NOT NULL
);
CREATE TABLE ma_tx_out (
    id BIGINT PRIMARY KEY,
    tx_out_id BIGINT NOT NULL,
    ident BIGINT NOT NULL,
    quantity BIGINT NOT NULL
);
CREATE TABLE multi_asset (
    id BIGINT PRIMARY KEY,
    policy BYTEA NOT NULL,
    name BYTEA NOT NULL
);

INSERT INTO block VALUES
    (1, 0, decode(repeat('00', 32), 'hex'), TIMESTAMP '2025-12-31 23:59:54', 0, 0, 1),
    (2, 1, decode(repeat('01', 32), 'hex'), TIMESTAMP '2026-01-01 00:00:00', 0, 1, 1),
    (3, 2, decode(repeat('02', 32), 'hex'), TIMESTAMP '2026-01-01 00:00:06', 0, 2, 1);
INSERT INTO tx VALUES
    (5, 1, 0, decode(repeat('05', 32), 'hex')),
    (10, 2, 0, decode(repeat('11', 32), 'hex')),
    (20, 3, 0, decode(repeat('22', 32), 'hex'));
INSERT INTO address VALUES (1, 'script'), (2, 'previous');
INSERT INTO datum VALUES
    (decode(repeat('33', 32), 'hex'), '{"constructor": 0, "fields": []}');
INSERT INTO tx_out VALUES
    (500, 5, 1, 'previous', 2, NULL, 10),
    (1000, 10, 0, 'script', 1, decode(repeat('33', 32), 'hex'), 20);
INSERT INTO tx_in VALUES (2500, 10, 5, 1), (3000, 20, 10, 0);
INSERT INTO ma_tx_out VALUES (2000, 1000, 7, 1);
INSERT INTO multi_asset VALUES (7, decode(repeat('44', 28), 'hex'), ''::bytea);
"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		let start = CardanoPosition { block_number: 1, ..Default::default() };
		let end = CardanoPosition { block_number: 3, ..Default::default() };
		let query = paged_query(&start, &end);

		for tx_input_mode in [ResolvedDbSyncTxInputMode::TxIn, ResolvedDbSyncTxInputMode::Consumed]
		{
			for address_mode in
				[ResolvedDbSyncAddressMode::Inline, ResolvedDbSyncAddressMode::AddressTable]
			{
				let config = ResolvedDbSyncQueryConfig { tx_input_mode, address_mode };
				assert_observations(&pool, &query, config).await;
				assert_candidate_and_federated_queries(&pool, config).await;
			}
		}

		// Prove consumed/address-table mode does not accidentally retain a runtime dependency on
		// either legacy representation.
		sqlx::raw_sql("ALTER TABLE tx_out DROP COLUMN address; DROP TABLE tx_in;")
			.execute(&pool)
			.await
			.unwrap();
		let consumed_address_table_config = ResolvedDbSyncQueryConfig {
			tx_input_mode: ResolvedDbSyncTxInputMode::Consumed,
			address_mode: ResolvedDbSyncAddressMode::AddressTable,
		};
		assert_observations(&pool, &query, consumed_address_table_config).await;
		assert_candidate_and_federated_queries(&pool, consumed_address_table_config).await;
		assert!(get_low_bounds(&pool, 2, consumed_address_table_config).await.unwrap().is_some());
		assert!(
			get_high_bounds(&pool, 1, consumed_address_table_config)
				.await
				.unwrap()
				.is_some()
		);
	}
}

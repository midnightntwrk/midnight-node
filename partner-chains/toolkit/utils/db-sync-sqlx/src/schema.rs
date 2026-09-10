// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{DbSyncSchemaMode, ResolvedDbSyncAddressMode, ResolvedDbSyncQueryConfig};
use log::info;
use sqlx::{FromRow, Pool, Postgres};

/// A recommended index used by db-sync-backed queries.
#[derive(Debug, Clone, Copy)]
pub struct DbSyncIndexSpec {
	/// Name used when the index is created by Midnight.
	pub name: &'static str,
	/// Relation resolved on the connection's `search_path`.
	pub relation: &'static str,
	/// Access methods that can serve the query. The first is used in diagnostics.
	pub access_methods: &'static [&'static str],
	/// Required leading index keys or expressions.
	pub keys: &'static [&'static str],
	/// DDL used in `apply` mode when no compatible index exists.
	pub create_sql: &'static str,
}

/// Indexes used by candidate and Ariadne-parameter queries.
pub fn candidate_index_specs(config: ResolvedDbSyncQueryConfig) -> Vec<DbSyncIndexSpec> {
	let mut indexes = vec![
		DbSyncIndexSpec {
			name: "idx_ma_tx_out_ident",
			relation: "ma_tx_out",
			access_methods: &["btree"],
			keys: &["ident"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ma_tx_out_ident ON ma_tx_out(ident)",
		},
		DbSyncIndexSpec {
			name: "idx_ma_tx_out_id_ident",
			relation: "ma_tx_out",
			access_methods: &["btree"],
			// The standard db-sync tx_out_id index is sufficient because each output has a
			// bounded asset set. Apply keeps the historical covering-index DDL when no
			// tx_out_id-leading index exists.
			keys: &["tx_out_id"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ma_tx_out_id_ident ON ma_tx_out(tx_out_id, ident)",
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
		crate::ResolvedDbSyncTxInputMode::TxIn => indexes.extend([
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
		crate::ResolvedDbSyncTxInputMode::Consumed => indexes.push(DbSyncIndexSpec {
			name: "idx_tx_out_consumed_by_tx_id",
			relation: "tx_out",
			access_methods: &["btree"],
			keys: &["consumed_by_tx_id"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_consumed_by_tx_id ON tx_out(consumed_by_tx_id)",
		}),
	}

	indexes
}

/// Applies or verifies an index manifest. Verification accepts an index with any name when it is
/// valid, ready, non-partial, uses an accepted access method, and has the requested leading keys.
pub async fn manage_indexes(
	pool: &Pool<Postgres>,
	mode: DbSyncSchemaMode,
	indexes: &[DbSyncIndexSpec],
) -> Result<(), sqlx::Error> {
	if mode == DbSyncSchemaMode::Skip {
		info!("Skipping db-sync index management and verification");
		return Ok(());
	}

	let mut missing = Vec::new();
	for index in indexes {
		if has_compatible_index(pool, index).await? {
			info!(
				"Compatible index for {}({}) already exists",
				index.relation,
				index.keys.join(", ")
			);
			continue;
		}

		if mode == DbSyncSchemaMode::Verify {
			missing.push(format!(
				"{} USING {} ({})",
				index.relation,
				index.access_methods.join(" or "),
				index.keys.join(", ")
			));
			continue;
		}

		info!("Creating db-sync index '{}'; this may take a while", index.name);
		sqlx::query(index.create_sql).execute(pool).await?;

		if !has_compatible_index(pool, index).await? {
			let message = format!(
				"index '{}' exists but does not provide a valid {} index on {}({}); remove or rename the conflicting index and retry",
				index.name,
				index.access_methods.join(" or "),
				index.relation,
				index.keys.join(", ")
			);
			return Err(sqlx::Error::Protocol(message));
		}
	}

	if missing.is_empty() {
		Ok(())
	} else {
		Err(sqlx::Error::Protocol(format!(
			"db_sync_schema_mode=verify found missing or unusable indexes: {}. See docs/configuration-guide.md for operator-managed SQL",
			missing.join("; ")
		)))
	}
}

#[derive(Debug, FromRow)]
struct IndexDefinition {
	access_method: String,
	is_valid: bool,
	is_ready: bool,
	predicate: Option<String>,
	keys: Vec<String>,
}

async fn has_compatible_index(
	pool: &Pool<Postgres>,
	spec: &DbSyncIndexSpec,
) -> Result<bool, sqlx::Error> {
	let definitions = sqlx::query_as::<_, IndexDefinition>(
		r#"
SELECT
    access_method.amname AS access_method,
    index.indisvalid AS is_valid,
    index.indisready AS is_ready,
    pg_get_expr(index.indpred, index.indrelid) AS predicate,
    ARRAY(
        SELECT COALESCE(
            attribute.attname::text,
            pg_get_indexdef(index.indexrelid, key_column.position::integer, true)
        )
        FROM unnest(index.indkey::smallint[]) WITH ORDINALITY
            AS key_column(attribute_number, position)
        LEFT JOIN pg_catalog.pg_attribute AS attribute
            ON attribute.attrelid = index.indrelid
            AND attribute.attnum = key_column.attribute_number
        WHERE key_column.position <= index.indnkeyatts
        ORDER BY key_column.position
    ) AS keys
FROM pg_catalog.pg_index AS index
JOIN pg_catalog.pg_class AS index_class ON index_class.oid = index.indexrelid
JOIN pg_catalog.pg_am AS access_method ON access_method.oid = index_class.relam
WHERE index.indrelid = to_regclass($1)
"#,
	)
	.bind(spec.relation)
	.fetch_all(pool)
	.await?;

	Ok(definitions.iter().any(|definition| {
		definition.is_valid
			&& definition.is_ready
			&& definition.predicate.is_none()
			&& spec.access_methods.contains(&definition.access_method.as_str())
			&& definition.keys.len() >= spec.keys.len()
			&& definition
				.keys
				.iter()
				.zip(spec.keys)
				.all(|(actual, expected)| actual == expected)
	}))
}

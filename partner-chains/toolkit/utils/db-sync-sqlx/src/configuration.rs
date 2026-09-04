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

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

/// Selects how transaction inputs are read from Cardano db-sync.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DbSyncTxInputMode {
	/// Preserve the legacy behaviour: use `tx_in` when it is populated, otherwise use
	/// `tx_out.consumed_by_tx_id`.
	#[default]
	Auto,
	/// Read transaction inputs from the `tx_in` table (`tx_out.value = "enable"`, or
	/// `force_tx_in = true`).
	TxIn,
	/// Read transaction inputs from `tx_out.consumed_by_tx_id`
	/// (`tx_out.value = "consumed"`).
	Consumed,
}

/// Selects how output addresses are stored by Cardano db-sync.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DbSyncAddressMode {
	/// Read address columns directly from `tx_out` (`use_address_table = false`).
	#[default]
	Inline,
	/// Join `tx_out.address_id` to `address.id` (`use_address_table = true`).
	AddressTable,
}

/// Controls whether Midnight changes or checks the db-sync database schema.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DbSyncSchemaMode {
	/// Create missing recommended indexes and apply database-level query tuning.
	#[default]
	Apply,
	/// Perform read-only verification of recommended indexes and query tuning.
	Verify,
	/// Do not apply or verify recommended indexes or query tuning. Query-layout validation is
	/// still performed before queries are run.
	Skip,
}

/// Requested db-sync query layout.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct DbSyncQueryConfig {
	/// Requested transaction-input representation.
	pub tx_input_mode: DbSyncTxInputMode,
	/// Requested output-address representation.
	pub address_mode: DbSyncAddressMode,
}

/// Resolved transaction-input representation used by SQL queries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ResolvedDbSyncTxInputMode {
	TxIn,
	Consumed,
}

/// Resolved address representation used by SQL queries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ResolvedDbSyncAddressMode {
	Inline,
	AddressTable,
}

/// Validated db-sync layout used by SQL queries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ResolvedDbSyncQueryConfig {
	/// Validated transaction-input representation.
	pub tx_input_mode: ResolvedDbSyncTxInputMode,
	/// Validated output-address representation.
	pub address_mode: ResolvedDbSyncAddressMode,
}

impl DbSyncQueryConfig {
	/// Validates and resolves this layout against the database visible on the connection's
	/// `search_path`. Explicit modes fail fast when their required columns are unavailable.
	pub async fn resolve(
		self,
		pool: &Pool<Postgres>,
	) -> Result<ResolvedDbSyncQueryConfig, sqlx::Error> {
		let tx_input_mode = match self.tx_input_mode {
			DbSyncTxInputMode::Auto => detect_tx_input_mode(pool).await?,
			DbSyncTxInputMode::TxIn => {
				require_columns(pool, "tx_in", &["tx_in_id", "tx_out_id", "tx_out_index"]).await?;
				ResolvedDbSyncTxInputMode::TxIn
			},
			DbSyncTxInputMode::Consumed => {
				require_columns(pool, "tx_out", &["consumed_by_tx_id"]).await?;
				ResolvedDbSyncTxInputMode::Consumed
			},
		};

		let address_mode = match self.address_mode {
			DbSyncAddressMode::Inline => {
				require_columns(pool, "tx_out", &["address"]).await?;
				ResolvedDbSyncAddressMode::Inline
			},
			DbSyncAddressMode::AddressTable => {
				require_columns(pool, "tx_out", &["address_id"]).await?;
				require_columns(pool, "address", &["id", "address"]).await?;
				ResolvedDbSyncAddressMode::AddressTable
			},
		};

		Ok(ResolvedDbSyncQueryConfig { tx_input_mode, address_mode })
	}
}

async fn detect_tx_input_mode(
	pool: &Pool<Postgres>,
) -> Result<ResolvedDbSyncTxInputMode, sqlx::Error> {
	let has_tx_in = has_columns(pool, "tx_in", &["tx_in_id", "tx_out_id", "tx_out_index"]).await?;
	let has_consumed = has_columns(pool, "tx_out", &["consumed_by_tx_id"]).await?;

	if has_tx_in {
		let populated = sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM tx_in)")
			.fetch_one(pool)
			.await?;
		if populated {
			return Ok(ResolvedDbSyncTxInputMode::TxIn);
		}
	}

	if has_consumed {
		let populated = sqlx::query_scalar::<_, bool>(
			"SELECT EXISTS (SELECT 1 FROM tx_out WHERE consumed_by_tx_id IS NOT NULL)",
		)
		.fetch_one(pool)
		.await?;
		if populated {
			return Ok(ResolvedDbSyncTxInputMode::Consumed);
		}
	}

	match (has_tx_in, has_consumed) {
		(true, false) => Ok(ResolvedDbSyncTxInputMode::TxIn),
		(false, true) => Ok(ResolvedDbSyncTxInputMode::Consumed),
		(true, true) => Err(sqlx::Error::Protocol(
			"db-sync transaction-input layout is ambiguous because both supported representations are empty; set db_sync_tx_input_mode to tx_in or consumed explicitly"
				.to_string(),
		)),
		(false, false) => Err(sqlx::Error::Protocol(
			"db-sync transaction-input layout is unsupported: expected tx_in(tx_in_id, tx_out_id, tx_out_index) or tx_out.consumed_by_tx_id"
				.to_string(),
		)),
	}
}

async fn require_columns(
	pool: &Pool<Postgres>,
	relation: &str,
	columns: &[&str],
) -> Result<(), sqlx::Error> {
	if has_columns(pool, relation, columns).await? {
		return Ok(());
	}

	Err(sqlx::Error::Protocol(format!(
		"configured db-sync layout requires {relation}({}), but those columns are not available on the current search_path",
		columns.join(", ")
	)))
}

async fn has_columns(
	pool: &Pool<Postgres>,
	relation: &str,
	columns: &[&str],
) -> Result<bool, sqlx::Error> {
	let present = sqlx::query_scalar::<_, i64>(
		r#"
SELECT COUNT(*)
FROM pg_catalog.pg_attribute
WHERE attrelid = to_regclass($1)
  AND attname = ANY($2)
  AND attnum > 0
  AND NOT attisdropped
"#,
	)
	.bind(relation)
	.bind(columns)
	.fetch_one(pool)
	.await?;

	Ok(present == columns.len() as i64)
}

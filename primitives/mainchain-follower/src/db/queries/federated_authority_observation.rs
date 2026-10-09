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

//! Database Queries for Federated Authority Observation
//!
//! This module provides database queries used for federated authority observation
//! To get a better understanding of how these queries are working, see the schema documentation for db-sync:
//! https://github.com/IntersectMBO/cardano-db-sync/blob/master/doc/schema.md

use crate::db::GovernanceBodyUtxoRow;
use db_sync_sqlx::{ResolvedDbSyncAddressMode, ResolvedDbSyncQueryConfig};
use sidechain_domain::PolicyId;
use sqlx::{Pool, Postgres, error::Error as SqlxError};

/// This query finds the most recent UTXO up to and including the specified block that matches:
/// - A provided script address
/// - A provided policy ID (for the native asset)
///
/// It is assumed that spending governance UTXO is always replacement and never removal, so the query does not check if the UTXO is spent.
pub async fn get_governance_body_utxo(
	pool: &Pool<Postgres>,
	script_address: &str,
	policy_id: &PolicyId,
	block_number: u32,
	config: ResolvedDbSyncQueryConfig,
) -> Result<Option<GovernanceBodyUtxoRow>, SqlxError> {
	let (address_join, address_column) = match config.address_mode {
		ResolvedDbSyncAddressMode::Inline => ("", "tx_out.address"),
		ResolvedDbSyncAddressMode::AddressTable => (
			"JOIN address tx_out_address ON tx_out_address.id = tx_out.address_id",
			"tx_out_address.address",
		),
	};
	let sql = format!(
		r#"
SELECT
    datum.value::jsonb AS full_datum,
    block.block_no as block_number,
    block.hash as block_hash,
    tx.block_index as tx_index_in_block,
    tx.hash AS tx_hash,
    tx_out.index AS utxo_index
FROM tx_out
    {address_join}
    JOIN datum ON tx_out.data_hash = datum.hash
    JOIN tx ON tx.id = tx_out.tx_id
    JOIN block ON block.id = tx.block_id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
    JOIN multi_asset ma ON ma.id = ma_tx_out.ident
WHERE {address_column} = $1
    AND ma.policy = $2
    AND block.block_no <= $3
ORDER BY block.block_no DESC, tx.block_index DESC
LIMIT 1
        "#
	);
	sqlx::query_as::<_, GovernanceBodyUtxoRow>(&sql)
		.bind(script_address)
		.bind(policy_id.0)
		.bind(block_number as i32)
		.fetch_optional(pool)
		.await
}

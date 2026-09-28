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

use crate::db::DbDatum;
use sqlx::PgPool;

#[derive(sqlx::FromRow)]
pub struct VirtualAccountUtxo {
	pub asset_name: Vec<u8>,
	pub lovelace: String,
	pub full_datum: DbDatum,
}

/// Read the unspent account NFTs at the selected Cardano block.
pub async fn get_virtual_account_utxos(
	pool: &PgPool,
	policy: &[u8; 28],
	block_number: u32,
) -> Result<Vec<VirtualAccountUtxo>, sqlx::Error> {
	sqlx::query_as::<_, VirtualAccountUtxo>(
		r#"
SELECT ma.name AS asset_name, tx_out.value::text AS lovelace,
       datum.value::jsonb AS full_datum
FROM tx_out
JOIN tx ON tx.id = tx_out.tx_id
JOIN block ON block.id = tx.block_id
JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
JOIN multi_asset ma ON ma.id = ma_tx_out.ident
JOIN datum ON datum.id = tx_out.inline_datum_id
WHERE ma.policy = $1 AND ma_tx_out.quantity = 1
  AND octet_length(ma.name) = 29
  AND substring(ma.name from 1 for 1) IN (decode('00', 'hex'), decode('01', 'hex'))
  AND block.block_no <= $2
  AND NOT EXISTS (
      SELECT 1 FROM tx_in
      JOIN tx spending_tx ON spending_tx.id = tx_in.tx_in_id
      JOIN block spending_block ON spending_block.id = spending_tx.block_id
      WHERE tx_in.tx_out_id = tx_out.tx_id AND tx_in.tx_out_index = tx_out.index
        AND spending_block.block_no <= $2
  )
ORDER BY ma.name
"#,
	)
	.bind(policy.as_slice())
	.bind(block_number as i32)
	.fetch_all(pool)
	.await
}

#[derive(sqlx::FromRow)]
pub struct PoolParameters {
	pub id: i64,
	pub margin: f64,
	pub reward_account: Vec<u8>,
}

/// Read the latest effective pool certificate visible at the observation block.
pub async fn get_pool_parameters(
	pool: &PgPool,
	pool_id: &[u8; 28],
	epoch: u32,
	block_number: u32,
) -> Result<Option<PoolParameters>, sqlx::Error> {
	sqlx::query_as::<_, PoolParameters>(
		r#"
SELECT pu.id, pu.margin, sa.hash_raw AS reward_account
FROM pool_update pu
JOIN pool_hash ph ON ph.id = pu.hash_id
JOIN stake_address sa ON sa.id = pu.reward_addr_id
JOIN tx ON tx.id = pu.registered_tx_id
JOIN block ON block.id = tx.block_id
WHERE ph.hash_raw = $1 AND pu.active_epoch_no <= $2
  AND (block.block_no <= $3 OR block.epoch_no IS NULL)
ORDER BY pu.active_epoch_no DESC, tx.id DESC, pu.cert_index DESC
LIMIT 1
"#,
	)
	.bind(pool_id.as_slice())
	.bind(i64::from(epoch))
	.bind(block_number as i32)
	.fetch_optional(pool)
	.await
}

/// Read owners of the selected pool certificate in credential order.
pub async fn get_pool_owners(pool: &PgPool, update_id: i64) -> Result<Vec<Vec<u8>>, sqlx::Error> {
	sqlx::query_scalar(
		r#"
SELECT DISTINCT sa.hash_raw
FROM pool_owner po
JOIN stake_address sa ON sa.id = po.addr_id
WHERE po.pool_update_id = $1
ORDER BY sa.hash_raw
"#,
	)
	.bind(update_id)
	.fetch_all(pool)
	.await
}

/// Read the immutable delegation snapshot used for committee selection.
pub async fn get_pool_delegators(
	pool: &PgPool,
	pool_id: &[u8; 28],
	epoch: u32,
) -> Result<Vec<(Vec<u8>, String)>, sqlx::Error> {
	sqlx::query_as(
		r#"
SELECT sa.hash_raw, SUM(es.amount)::text
FROM epoch_stake es
JOIN pool_hash ph ON ph.id = es.pool_id
JOIN stake_address sa ON sa.id = es.addr_id
WHERE ph.hash_raw = $1 AND es.epoch_no = $2
GROUP BY sa.hash_raw
ORDER BY sa.hash_raw
"#,
	)
	.bind(pool_id.as_slice())
	.bind(epoch as i32)
	.fetch_all(pool)
	.await
}

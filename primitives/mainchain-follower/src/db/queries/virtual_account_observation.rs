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

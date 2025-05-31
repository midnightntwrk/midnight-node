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

use crate::db_datum::DbDatum;
use cardano_serialization_lib::PlutusData;
use sidechain_domain::*;
use sqlx::database::{HasArguments, HasValueRef};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::error::Error as SqlxError;
use sqlx::postgres::PgTypeInfo;
use sqlx::types::chrono::NaiveDateTime;
use sqlx::{Decode, Encode, Pool, Postgres};

// TODO: this is necessary boilerplate until after pc 1.6 when the dbsync-sqlx crate can be pointed to with general db types.
// Note that we must impl this macro for db types that we want.
/// Generates sqlx implementations for an unsigned wrapper of types that are signed.
/// We expect that values will have always 0 as the most significant bit.
/// For example TxIndex is in range of [0, 2^15-1], it will be u16 in domain,
/// but it requires encoding and decoding like i16.
/// See txindex, word31 and word63 types in db-sync schema definition.
macro_rules! sqlx_implementations_for_wrapper {
	($WRAPPED:ty, $DBTYPE:expr, $NAME:ty, $DOMAIN:ty) => {
		impl sqlx::Type<Postgres> for $NAME {
			fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
				PgTypeInfo::with_name($DBTYPE)
			}
		}

		impl<'r> Decode<'r, Postgres> for $NAME
		where
			$WRAPPED: Decode<'r, Postgres>,
		{
			fn decode(value: <Postgres as HasValueRef<'r>>::ValueRef) -> Result<Self, BoxDynError> {
				let decoded: $WRAPPED = <$WRAPPED as Decode<Postgres>>::decode(value)?;
				Ok(Self(decoded.try_into()?))
			}
		}

		#[cfg(test)]
		impl From<$WRAPPED> for $NAME {
			fn from(value: $WRAPPED) -> Self {
				Self(value.try_into().expect("value from domain fits in type db type"))
			}
		}

		impl<'q> Encode<'q, Postgres> for $NAME {
			fn encode_by_ref(
				&self,
				buf: &mut <Postgres as HasArguments<'q>>::ArgumentBuffer,
			) -> IsNull {
				buf.extend(&self.0.to_be_bytes());
				IsNull::No
			}
		}

		impl From<$NAME> for $DOMAIN {
			fn from(value: $NAME) -> Self {
				Self(value.0)
			}
		}

		impl From<$DOMAIN> for $NAME {
			fn from(value: $DOMAIN) -> Self {
				Self(value.0)
			}
		}
	};
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct Asset {
	pub policy_id: PolicyId,
	pub asset_name: AssetName,
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct AssetName(pub Vec<u8>);

impl From<sidechain_domain::AssetName> for AssetName {
	fn from(name: sidechain_domain::AssetName) -> Self {
		Self(name.0.to_vec())
	}
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SlotNumber(pub u64);
sqlx_implementations_for_wrapper!(i64, "INT8", SlotNumber, McSlotNumber);

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub struct Block {
	pub block_no: BlockNumber,
	pub hash: [u8; 32],
	pub epoch_no: EpochNumber,
	pub slot_no: SlotNumber,
	pub time: NaiveDateTime,
}

impl From<Block> for MainchainBlock {
	fn from(b: Block) -> Self {
		MainchainBlock {
			number: McBlockNumber(b.block_no.0),
			hash: McBlockHash(b.hash),
			epoch: McEpochNumber(b.epoch_no.0),
			slot: McSlotNumber(b.slot_no.0),
			timestamp: b.time.and_utc().timestamp().try_into().expect("i64 timestamp is valid u64"),
		}
	}
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BlockNumber(pub u32);
sqlx_implementations_for_wrapper!(i32, "INT4", BlockNumber, McBlockNumber);

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct EpochNumber(pub u32);
sqlx_implementations_for_wrapper!(i32, "INT4", EpochNumber, McEpochNumber);

pub async fn get_datum_for_address(
	pool: &Pool<Postgres>,
	smart_contract_address: &str,
	min_block_no: i64,
	max_block_no: i64,
) -> Result<Vec<PlutusData>, SqlxError> {
	let sql = r#"
            SELECT
            datum.value::jsonb AS full_datum,
            encode(tx.hash, 'hex') AS tx_hash,
            tx_out.index AS tx_index,
            block.block_no,
            block.time
            FROM tx_out
            JOIN datum ON tx_out.data_hash = datum.hash
            JOIN tx ON tx.id = tx_out.tx_id
            JOIN block ON block.id = tx.block_id
            WHERE tx_out.address = $1
            AND block.block_no BETWEEN $2 AND $3
            ORDER BY block.block_no ASC, tx.id ASC, tx_out.index ASC;
        "#;

	let rows: Vec<DbDatum> = sqlx::query_as(sql)
		.bind(smart_contract_address)
		.bind(min_block_no)
		.bind(max_block_no)
		.fetch_all(pool)
		.await?;
	Ok(rows.into_iter().map(|d| d.0).collect())
}

pub(crate) async fn get_spends_for_asset_in_block_range(
	pool: &Pool<Postgres>,
	policy_id_hex: &str,
	asset_name_hex: &str,
	min_block_no: i64,
	max_block_no: i64,
) -> Result<Vec<PolicyUtxoRow>, SqlxError> {
	let sql = r#"
        SELECT
            encode(ma.policy, 'hex')     AS policy_hex,
            encode(ma.name, 'hex')       AS asset_name_hex,
            ma_tx_out.quantity::BIGINT   AS quantity,
            tx_out.address               AS holder_address,
            tx_out.value::BIGINT         AS ada_lovelace,
            encode(spending_tx.hash, 'hex') AS creating_tx_hash,
            spending_block.block_no      AS block_no,
            spending_block.time          AS time
        FROM ma_tx_out
        JOIN multi_asset   ma           ON ma.id = ma_tx_out.ident
        JOIN tx_out        ON tx_out.id = ma_tx_out.tx_out_id
        JOIN tx_in         ON tx_out.tx_id = tx_in.tx_out_id AND tx_out.index = tx_in.tx_out_index
        JOIN tx            AS spending_tx ON tx_in.tx_in_id = spending_tx.id
        JOIN block         AS spending_block ON spending_tx.block_id = spending_block.id
        WHERE encode(ma.policy, 'hex') = $1
          AND encode(ma.name, 'hex') = $2
          AND spending_block.block_no >= $3
          AND spending_block.block_no <= $4
        ORDER BY spending_block.block_no ASC, spending_tx.id ASC, tx_out.index ASC
    "#;

	let rows = sqlx::query_as::<_, PolicyUtxoRow>(sql)
		.bind(policy_id_hex)
		.bind(asset_name_hex)
		.bind(min_block_no)
		.bind(max_block_no)
		.fetch_all(pool)
		.await?;

	Ok(rows)
}

pub async fn get_latest_block_no(pool: &Pool<Postgres>) -> Result<i64, SqlxError> {
	let row: (i32,) = sqlx::query_as("SELECT MAX(block_no) FROM block").fetch_one(pool).await?;
	Ok(row.0 as i64)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicyUtxoRow {
	pub policy_hex: String,
	pub asset_name_hex: String,
	pub quantity: i64,
	pub holder_address: String,
	pub ada_lovelace: i64,
	pub creating_tx_hash: String,
	pub block_no: i32,
	pub time: NaiveDateTime,
}

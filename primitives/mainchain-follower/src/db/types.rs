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

use cardano_serialization_lib::{
	PlutusData, PlutusDatumSchema::DetailedSchema, encode_json_value_to_plutus_datum,
};
use db_sync_sqlx::{
	BlockNumber as DbBlockNumber, EpochNumber, SlotNumber, TxHash as DbTxHash,
	TxIndex as DbUtxoIndexInTx, TxIndexInBlock as DbTxIndexInBlock,
};
use midnight_primitives_cnight_observation::CardanoPosition;

use sidechain_domain::McBlockHash;
use sqlx::error::BoxDynError;
use sqlx::postgres::PgTypeInfo;
use sqlx::types::JsonValue;
use sqlx::{
	Decode, FromRow, Postgres, Row, ValueRef, postgres::PgRow, types::chrono::NaiveDateTime,
};

/// Wraps PlutusData to provide sqlx::Decode and sqlx::Type implementations
#[derive(Debug, Clone, PartialEq)]
pub struct DbDatum(pub PlutusData);

impl sqlx::Type<Postgres> for DbDatum {
	fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
		PgTypeInfo::with_name("JSONB")
	}
}

impl<'r> Decode<'r, Postgres> for DbDatum
where
	JsonValue: Decode<'r, Postgres>,
{
	fn decode(value: <Postgres as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
		let value: JsonValue = <JsonValue as Decode<Postgres>>::decode(value)?;
		let datum = encode_json_value_to_plutus_datum(value, DetailedSchema);
		Ok(DbDatum(datum?))
	}
}

/// Decodes a datum that is NULL or not valid Plutus data as `None`, for addresses anyone can pay
/// to, where one bad datum must not fail the whole query.
pub(crate) struct DbDatumIfValid(Option<PlutusData>);

impl sqlx::Type<Postgres> for DbDatumIfValid {
	fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
		PgTypeInfo::with_name("JSONB")
	}
}

impl<'r> Decode<'r, Postgres> for DbDatumIfValid {
	fn decode(value: <Postgres as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
		if value.is_null() {
			return Ok(Self(None));
		}
		// serde_json rejects JSON nested past 128 levels, which a datum of 64 nested lists reaches.
		let Ok(value) = <JsonValue as Decode<Postgres>>::decode(value) else {
			return Ok(Self(None));
		};
		Ok(Self(encode_json_value_to_plutus_datum(value, DetailedSchema).ok()))
	}
}

impl From<DbDatumIfValid> for Option<DbDatum> {
	fn from(datum: DbDatumIfValid) -> Self {
		datum.0.map(DbDatum)
	}
}

impl<'r> FromRow<'r, PgRow> for DbDatum {
	fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
		let json_val: JsonValue = row.try_get("full_datum")?;
		let datum = encode_json_value_to_plutus_datum(json_val, DetailedSchema)
			.map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
		Ok(DbDatum(datum))
	}
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub struct Block {
	pub block_number: DbBlockNumber,
	pub hash: [u8; 32],
	pub epoch_number: EpochNumber,
	pub slot_number: SlotNumber,
	pub time: NaiveDateTime,
	pub tx_count: i64,
}

impl From<Block> for CardanoPosition {
	fn from(b: Block) -> Self {
		CardanoPosition {
			block_hash: McBlockHash(b.hash),
			block_number: b.block_number.0,
			block_timestamp: b.time.and_utc().into(),
			tx_index_in_block: b.tx_count as u32,
		}
	}
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, sqlx::Type)]
#[sqlx(transparent)]
pub struct DbBlockHash(pub [u8; 32]);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RegistrationRow {
	#[sqlx(try_from = "DbDatumIfValid")]
	pub full_datum: Option<DbDatum>,
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeregistrationRow {
	#[sqlx(try_from = "DbDatumIfValid")]
	pub full_datum: Option<DbDatum>,
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub tx_hash: DbTxHash,
	pub utxo_tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssetCreateRow {
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub quantity: i64,
	pub holder_address: String,
	pub tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssetSpendRow {
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub quantity: i64,
	pub holder_address: String,
	pub utxo_tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
	pub spending_tx_hash: DbTxHash,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BlockTxIndexRow {
	pub block_hash: String,
	pub tx_hash: String,
	pub tx_index: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GovernanceBodyUtxoRow {
	pub full_datum: DbDatum,
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
}

/// Encapsulates ids of three big tables.
/// Used to filter out most of the tables before joining them.
/// db_sync id types are 'bigint' which have bound like 'i64'
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct QueryBounds {
	pub tx_id: i64,
	pub tx_out_id: i64,
	pub ma_tx_out_id: i64,
	pub tx_in_id: i64,
}

/// A block-range + pagination window shared by the cNight observation queries.
/// `start`/`end` scope blocks and tx index; `low_bound`/`high_bound` pre-filter
/// the underlying id-indexed tables so the SQL planner avoids full scans.
#[derive(Debug, Clone, Copy)]
pub struct PagedQuery<'a> {
	pub start: &'a CardanoPosition,
	pub end: &'a CardanoPosition,
	pub limit: usize,
	pub offset: usize,
	pub low_bound: QueryBounds,
	pub high_bound: QueryBounds,
}

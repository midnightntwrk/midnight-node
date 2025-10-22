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

use cardano_serialization_lib::{
	PlutusData, PlutusDatumSchema::DetailedSchema, encode_json_value_to_plutus_datum,
};
use midnight_primitives_native_token_observation::CardanoPosition;
use sqlx::error::BoxDynError;
use sqlx::postgres::PgTypeInfo;
use sqlx::types::JsonValue;
use sqlx::{Decode, FromRow, Postgres, Row, postgres::PgRow};

use sidechain_domain::*;

use sqlx::{Encode, encode::IsNull, types::chrono::NaiveDateTime};

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

impl<'r> FromRow<'r, PgRow> for DbDatum {
	fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
		let json_val: JsonValue = row.try_get("full_datum")?;
		let datum = encode_json_value_to_plutus_datum(json_val, DetailedSchema)
			.map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
		Ok(DbDatum(datum))
	}
}

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
			fn decode(
				value: <Postgres as sqlx::Database>::ValueRef<'r>,
			) -> Result<Self, BoxDynError> {
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
				buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer<'q>,
			) -> Result<IsNull, BoxDynError> {
				buf.extend(&self.0.to_be_bytes());
				Ok(IsNull::No)
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

// #[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
// pub(crate) struct Asset {
// 	pub policy_id: PolicyId,
// 	pub asset_name: AssetName,
// }

// #[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
// pub(crate) struct AssetName(pub Vec<u8>);

// impl From<sidechain_domain::AssetName> for AssetName {
// 	fn from(name: sidechain_domain::AssetName) -> Self {
// 		Self(name.0.to_vec())
// 	}
// }

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SlotNumber(pub u64);
sqlx_implementations_for_wrapper!(i64, "INT8", SlotNumber, McSlotNumber);

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
			block_hash: b.hash,
			block_number: b.block_number.0,
			block_timestamp: b.time.and_utc().into(),
			tx_index_in_block: b.tx_count as u32,
		}
	}
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DbBlockNumber(pub u32);
sqlx_implementations_for_wrapper!(i32, "INT4", DbBlockNumber, McBlockNumber);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, sqlx::Type)]
#[sqlx(transparent)]
pub struct DbBlockHash(pub [u8; 32]);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct DbTxIndexInBlock(pub u32);
sqlx_implementations_for_wrapper!(i32, "INT4", DbTxIndexInBlock, McTxIndexInBlock);

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct DbTxHash(pub [u8; TX_HASH_SIZE]);

#[derive(Debug, Clone)]
pub struct DbUtxoIndexInTx(pub u16);

impl sqlx::Type<Postgres> for DbUtxoIndexInTx {
	fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
		sqlx::postgres::PgTypeInfo::with_name("INT2")
	}
}

impl<'r> Decode<'r, Postgres> for DbUtxoIndexInTx {
	fn decode(value: <Postgres as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
		let value = <i16 as Decode<Postgres>>::decode(value)?;
		Ok(Self(value as u16))
	}
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct EpochNumber(pub u32);
sqlx_implementations_for_wrapper!(i32, "INT4", EpochNumber, McEpochNumber);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RedemptionCreateRow {
	pub full_datum: DbDatum,
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
	pub quantity: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RedemptionSpendRow {
	pub full_datum: DbDatum,
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub tx_hash: DbTxHash,
	pub utxo_tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
	pub quantity: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RegistrationRow {
	pub full_datum: DbDatum,
	pub block_number: DbBlockNumber,
	pub block_hash: DbBlockHash,
	pub block_timestamp: NaiveDateTime,
	pub tx_index_in_block: DbTxIndexInBlock,
	pub tx_hash: DbTxHash,
	pub utxo_index: DbUtxoIndexInTx,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeregistrationRow {
	pub full_datum: DbDatum,
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

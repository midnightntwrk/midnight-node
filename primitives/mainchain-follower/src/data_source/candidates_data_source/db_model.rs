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
use db_sync_sqlx::{Address, Asset, BlockNumber, EpochNumber, SlotNumber, TxIndex, TxIndexInBlock};
use log::info;
use sidechain_domain::{McTxHash, UtxoId, UtxoIndex};
use sqlx::error::BoxDynError;
use sqlx::postgres::PgTypeInfo;
use sqlx::types::JsonValue;
use sqlx::{Decode, PgPool, Pool, Postgres};
use std::cell::OnceCell;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Wraps PlutusData to provide sqlx::Decode and sqlx::Type implementations
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbDatum(pub PlutusData);

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

/// Error type returned by Db-Sync based data sources
#[derive(Debug, PartialEq, thiserror::Error)]
#[allow(dead_code, clippy::enum_variant_names)]
pub enum DataSourceError {
	/// Indicates that the Db-Sync database rejected a request as invalid
	#[error("Bad request: `{0}`.")]
	BadRequest(String),
	/// Indicates that an internal error occured when querying the Db-Sync database
	#[error("Internal error of data source: `{0}`.")]
	InternalDataSourceError(String),
	/// Indicates that expected data was not found when querying the Db-Sync database
	#[error(
		"'{0}' not found. Possible causes: data source configuration error, db-sync not synced fully, or data not set on the main chain."
	)]
	ExpectedDataNotFound(String),
	/// Indicates that data returned by the Db-Sync database is invalid
	#[error(
		"Invalid data. {0} Possible cause is an error in Plutus scripts or data source is outdated."
	)]
	InvalidData(String),
}

/// Wrapper error type for [sqlx::Error]
#[derive(Debug)]
pub struct SqlxError(sqlx::Error);

impl From<sqlx::Error> for SqlxError {
	fn from(value: sqlx::Error) -> Self {
		SqlxError(value)
	}
}

impl From<SqlxError> for DataSourceError {
	fn from(e: SqlxError) -> Self {
		DataSourceError::InternalDataSourceError(e.0.to_string())
	}
}

impl From<SqlxError> for Box<dyn std::error::Error + Send + Sync> {
	fn from(e: SqlxError) -> Self {
		e.0.into()
	}
}

/// Db-Sync `tx_in.value` configuration field
#[derive(Debug, PartialEq, Copy, Clone)]
pub(crate) enum TxInConfiguration {
	/// Transaction inputs are linked using `tx_in` table
	Enabled,
	/// Transaction inputs are linked using `consumed_by_tx_id` column in `tx_out` table
	Consumed,
}

impl TxInConfiguration {
	pub(crate) async fn from_connection(pool: &Pool<Postgres>) -> Result<Self, SqlxError> {
		let tx_in_exists = sqlx::query_scalar::<_, i64>(
			"select count(*) from information_schema.tables where table_name = 'tx_in';",
		)
		.fetch_one(pool)
		.await? == 1;

		if !tx_in_exists {
			return Ok(Self::Consumed);
		}

		let tx_in_populated = sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM tx_in);")
			.fetch_one(pool)
			.await?;

		if tx_in_populated {
			return Ok(Self::Enabled);
		}

		Ok(Self::Consumed)
	}
}

/// Structure that queries, caches and provides Db-Sync configuration
pub struct DbSyncConfigurationProvider {
	/// Postgres connection pool
	pub(crate) pool: PgPool,
	/// Transaction input configuration used by Db-Sync
	pub(crate) tx_in_config: Arc<Mutex<OnceCell<TxInConfiguration>>>,
}

impl DbSyncConfigurationProvider {
	pub(crate) fn new(pool: PgPool) -> Self {
		Self { tx_in_config: Arc::new(Mutex::new(OnceCell::new())), pool }
	}

	pub(crate) async fn get_tx_in_config(
		&self,
	) -> std::result::Result<TxInConfiguration, DataSourceError> {
		let lock = self.tx_in_config.lock().await;
		if let Some(tx_in_config) = lock.get() {
			Ok(*tx_in_config)
		} else {
			let tx_in_config = TxInConfiguration::from_connection(&self.pool).await?;
			lock.set(tx_in_config).map_err(|_| {
				DataSourceError::InternalDataSourceError(
					"Failed to set tx_in_config in DbSyncConfigurationProvider".into(),
				)
			})?;
			Ok(tx_in_config)
		}
	}
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct Block {
	pub block_no: BlockNumber,
	pub hash: [u8; 32],
	pub epoch_no: EpochNumber,
	pub slot_no: SlotNumber,
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct MainchainTxOutput {
	pub utxo_id: UtxoId,
	pub tx_block_no: BlockNumber,
	pub tx_slot_no: SlotNumber,
	pub tx_epoch_no: EpochNumber,
	pub tx_index_in_block: TxIndexInBlock,
	pub address: String,
	pub datum: Option<PlutusData>,
	pub tx_inputs: Vec<UtxoId>,
}

impl TryFrom<MainchainTxOutputRow> for MainchainTxOutput {
	type Error = sqlx::Error;
	fn try_from(r: MainchainTxOutputRow) -> Result<Self, Self::Error> {
		let tx_inputs: Result<Vec<UtxoId>, _> =
			r.tx_inputs.into_iter().map(|i| UtxoId::from_str(i.as_str())).collect();
		let tx_inputs = tx_inputs.map_err(|e| sqlx::Error::Decode(e.into()))?;
		Ok(MainchainTxOutput {
			utxo_id: UtxoId {
				tx_hash: McTxHash(r.utxo_id_tx_hash),
				index: UtxoIndex(r.utxo_id_index.0),
			},
			tx_block_no: r.tx_block_no,
			tx_slot_no: r.tx_slot_no,
			tx_epoch_no: r.tx_epoch_no,
			tx_index_in_block: r.tx_index_in_block,
			address: r.address,
			datum: r.datum.map(|d| d.0),
			tx_inputs,
		})
	}
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct MainchainTxOutputRow {
	pub utxo_id_tx_hash: [u8; 32],
	pub utxo_id_index: TxIndex,
	pub tx_block_no: BlockNumber,
	pub tx_slot_no: SlotNumber,
	pub tx_epoch_no: EpochNumber,
	pub tx_index_in_block: TxIndexInBlock,
	pub address: String,
	pub datum: Option<DbDatum>,
	pub tx_inputs: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct StakePoolEntry {
	pub pool_hash: [u8; 28],
	pub stake: db_sync_sqlx::StakeDelegation,
}

#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct TokenTxOutput {
	pub origin_tx_hash: [u8; 32],
	pub utxo_index: TxIndex,
	pub tx_epoch_no: EpochNumber,
	pub tx_block_no: BlockNumber,
	pub tx_slot_no: SlotNumber,
	pub tx_block_index: TxIndexInBlock,
	pub datum: Option<DbDatum>,
}

/// Cardano epoch nonce, ie. random 32 bytes generated by Cardano every epoch.
#[derive(Debug, Clone, sqlx::FromRow, PartialEq)]
pub(crate) struct MainchainEpochNonce(pub Vec<u8>);

pub(crate) async fn get_latest_block_for_epoch(
	pool: &Pool<Postgres>,
	epoch: EpochNumber,
) -> Result<Option<Block>, SqlxError> {
	// Query below contains additional filters for slot_no and block_no not null, because
	// there exists blocks in Byron Era with Ouroboros classic consensus that have null values for these fields.
	let sql = "SELECT block.block_no, block.hash, block.epoch_no, block.slot_no
		FROM block
		WHERE block.epoch_no <= $1 AND block.slot_no IS NOT NULL AND block.block_no IS NOT NULL
		ORDER BY block.slot_no DESC
		LIMIT 1";
	Ok(sqlx::query_as::<_, Block>(sql).bind(epoch).fetch_optional(pool).await?)
}

/// Returns number of the latest epoch that is stable - no block in such an epoch can be rolled back.
/// The latest stable epoch is one less than epoch of the highest stable block (HSB),
/// because unstable part could be replaced with blocks sequence starting from the block that has
/// the same epoch as HSB.
pub(crate) async fn get_latest_stable_epoch(
	pool: &Pool<Postgres>,
	security_parameter: u32,
) -> Result<Option<EpochNumber>, SqlxError> {
	let sql = "SELECT stable_block.epoch_no - 1 as epoch_no
FROM block INNER JOIN block as stable_block ON block.block_no - $1 = stable_block.block_no
WHERE block.block_no IS NOT NULL
ORDER BY block.block_no DESC
LIMIT 1";
	#[allow(deprecated)]
	Ok(sqlx::query_as::<_, db_sync_sqlx::EpochNumberRow>(sql)
		.bind(BlockNumber(security_parameter))
		.fetch_optional(pool)
		.await?
		.map(EpochNumber::from))
}

pub(crate) async fn get_stake_distribution(
	pool: &Pool<Postgres>,
	epoch: EpochNumber,
) -> Result<Vec<StakePoolEntry>, SqlxError> {
	let sql = "
        SELECT ph.hash_raw as pool_hash, SUM(es.amount) as stake
        FROM epoch_stake es
        INNER JOIN pool_hash ph ON es.pool_id = ph.id
        WHERE es.epoch_no = $1
        GROUP BY ph.hash_raw";
	Ok(sqlx::query_as::<_, StakePoolEntry>(sql).bind(epoch).fetch_all(pool).await?)
}

/// Returns the token data of the given policy at the given slot.
pub(crate) async fn get_token_utxo_for_epoch(
	pool: &Pool<Postgres>,
	asset: &Asset,
	epoch: EpochNumber,
) -> Result<Option<TokenTxOutput>, SqlxError> {
	// In practice queried assets always have empty name.
	// However, it's important to keep multi_asset.name condition, to enable use of compound index on multi_asset policy and name.
	let sql = "SELECT
			origin_tx.hash        AS origin_tx_hash,
        	tx_out.index          AS utxo_index,
        	origin_block.epoch_no AS tx_epoch_no,
        	origin_block.block_no AS tx_block_no,
        	origin_block.slot_no  AS tx_slot_no,
        	origin_tx.block_index AS tx_block_index,
        	datum.value           AS datum
        FROM ma_tx_out
        INNER JOIN multi_asset          ON ma_tx_out.ident = multi_asset.id
        INNER JOIN tx_out               ON ma_tx_out.tx_out_id = tx_out.id
        INNER JOIN tx origin_tx         ON tx_out.tx_id = origin_tx.id
        INNER JOIN block origin_block   ON origin_tx.block_id = origin_block.id
        LEFT JOIN datum                 ON tx_out.data_hash = datum.hash
        WHERE multi_asset.policy = $1
		AND multi_asset.name = $2
        AND origin_block.epoch_no <= $3
        ORDER BY tx_block_no DESC, origin_tx.block_index DESC
        LIMIT 1";
	Ok(sqlx::query_as::<_, TokenTxOutput>(sql)
		.bind(&asset.policy_id.0)
		.bind(&asset.asset_name.0)
		.bind(epoch)
		.fetch_optional(pool)
		.await?)
}

pub(crate) async fn get_epoch_nonce(
	pool: &Pool<Postgres>,
	epoch: EpochNumber,
) -> Result<Option<MainchainEpochNonce>, SqlxError> {
	let sql = "SELECT nonce FROM epoch_param WHERE epoch_no = $1";
	Ok(sqlx::query_as::<_, MainchainEpochNonce>(sql)
		.bind(epoch)
		.fetch_optional(pool)
		.await?)
}

/// Returns the epoch number for a given block hash
pub async fn get_epoch_for_block_hash(
	pool: &Pool<Postgres>,
	block_hash: &[u8; 32],
) -> Result<Option<EpochNumber>, SqlxError> {
	let sql = "SELECT epoch_no FROM block WHERE hash = $1";
	#[allow(deprecated)]
	Ok(sqlx::query_as::<_, db_sync_sqlx::EpochNumberRow>(sql)
		.bind(block_hash.as_slice())
		.fetch_optional(pool)
		.await?
		.map(EpochNumber::from))
}

pub(crate) async fn get_utxos_for_address(
	pool: &Pool<Postgres>,
	address: &Address,
	block: BlockNumber,
	tx_in_configuration: TxInConfiguration,
) -> Result<Vec<MainchainTxOutput>, SqlxError> {
	match tx_in_configuration {
		TxInConfiguration::Enabled => {
			get_utxos_for_address_tx_in_enabled(pool, address, block).await
		},
		TxInConfiguration::Consumed => {
			get_utxos_for_address_tx_in_consumed(pool, address, block).await
		},
	}
}

pub(crate) async fn get_utxos_for_address_tx_in_enabled(
	pool: &Pool<Postgres>,
	address: &Address,
	block: BlockNumber,
) -> Result<Vec<MainchainTxOutput>, SqlxError> {
	let query = "SELECT
          		origin_tx.hash as utxo_id_tx_hash,
          		tx_out.index as utxo_id_index,
          		origin_block.block_no as tx_block_no,
          		origin_block.slot_no as tx_slot_no,
          		origin_block.epoch_no as tx_epoch_no,
          		origin_tx.block_index as tx_index_in_block,
          		tx_out.address,
          		datum.value as datum,
          		array_agg(concat_ws('#', encode(consumes_tx.hash, 'hex'), consumes_tx_in.tx_out_index)) as tx_inputs
			FROM tx_out
			INNER JOIN tx    origin_tx       ON tx_out.tx_id = origin_tx.id
			INNER JOIN block origin_block    ON origin_tx.block_id = origin_block.id
          	LEFT JOIN tx_in consuming_tx_in  ON tx_out.tx_id = consuming_tx_in.tx_out_id AND tx_out.index = consuming_tx_in.tx_out_index
          	LEFT JOIN tx    consuming_tx     ON consuming_tx_in.tx_in_id = consuming_tx.id
          	LEFT JOIN block consuming_block  ON consuming_tx.block_id = consuming_block.id
			LEFT JOIN tx_in consumes_tx_in   ON consumes_tx_in.tx_in_id = origin_tx.id
          	LEFT JOIN tx_out consumes_tx_out ON consumes_tx_out.tx_id = consumes_tx_in.tx_out_id AND consumes_tx_in.tx_out_index = consumes_tx_out.index
          	LEFT JOIN tx consumes_tx         ON consumes_tx.id = consumes_tx_out.tx_id
          	LEFT JOIN datum                  ON tx_out.data_hash = datum.hash
          	WHERE
          		tx_out.address = $1 AND origin_block.block_no <= $2
          		AND (consuming_tx_in.id IS NULL OR consuming_block.block_no > $2)
          		GROUP BY (
					utxo_id_tx_hash,
					utxo_id_index,
					tx_block_no,
					tx_slot_no,
					tx_epoch_no,
					tx_index_in_block,
					tx_out.address,
					datum
				)";
	let rows = sqlx::query_as::<_, MainchainTxOutputRow>(query)
		.bind(&address.0)
		.bind(block)
		.fetch_all(pool)
		.await?;
	let result: Result<Vec<MainchainTxOutput>, sqlx::Error> =
		rows.into_iter().map(MainchainTxOutput::try_from).collect();
	Ok(result?)
}

pub(crate) async fn get_utxos_for_address_tx_in_consumed(
	pool: &Pool<Postgres>,
	address: &Address,
	block: BlockNumber,
) -> Result<Vec<MainchainTxOutput>, SqlxError> {
	let query = "SELECT
          		origin_tx.hash as utxo_id_tx_hash,
          		tx_out.index as utxo_id_index,
          		origin_block.block_no as tx_block_no,
          		origin_block.slot_no as tx_slot_no,
          		origin_block.epoch_no as tx_epoch_no,
          		origin_tx.block_index as tx_index_in_block,
          		tx_out.address,
          		datum.value as datum,
          		array_agg(concat_ws('#', encode(consumes_tx.hash, 'hex'), consumes_tx_out.index)) as tx_inputs
			FROM tx_out
			INNER JOIN tx    origin_tx       ON tx_out.tx_id = origin_tx.id
			INNER JOIN block origin_block    ON origin_tx.block_id = origin_block.id
			LEFT JOIN tx    consuming_tx     ON tx_out.consumed_by_tx_id = consuming_tx.id
			LEFT JOIN block consuming_block  ON consuming_tx.block_id = consuming_block.id
			LEFT JOIN tx_out consumes_tx_out ON consumes_tx_out.consumed_by_tx_id = origin_tx.id
			LEFT JOIN tx consumes_tx         ON consumes_tx.id = consumes_tx_out.tx_id
			LEFT JOIN datum                  ON tx_out.data_hash = datum.hash
          	WHERE
          		tx_out.address = $1 AND origin_block.block_no <= $2
          		AND (tx_out.consumed_by_tx_id IS NULL OR consuming_block.block_no > $2)
          		GROUP BY (
					utxo_id_tx_hash,
					utxo_id_index,
					tx_block_no,
					tx_slot_no,
					tx_epoch_no,
					tx_index_in_block,
					tx_out.address,
					datum
				)";
	let rows = sqlx::query_as::<_, MainchainTxOutputRow>(query)
		.bind(&address.0)
		.bind(block)
		.fetch_all(pool)
		.await?;
	let result: Result<Vec<MainchainTxOutput>, sqlx::Error> =
		rows.into_iter().map(MainchainTxOutput::try_from).collect();
	Ok(result?)
}

/// Used by `get_token_utxo_for_epoch` (CandidatesDataSourceImpl),
pub(crate) async fn create_idx_ma_tx_out_ident(pool: &Pool<Postgres>) -> Result<(), SqlxError> {
	let exists = index_exists(pool, "idx_ma_tx_out_ident").await?;
	if exists {
		info!("Index 'idx_ma_tx_out_ident' already exists");
	} else {
		let sql = "CREATE INDEX IF NOT EXISTS idx_ma_tx_out_ident ON ma_tx_out(ident)";
		info!("Executing '{}', this might take a while", sql);
		sqlx::query(sql).execute(pool).await?;
		info!("Index 'idx_ma_tx_out_ident' has been created");
	}
	Ok(())
}

/// Used by multiple queries across functionalities.
pub(crate) async fn create_idx_tx_out_address(pool: &Pool<Postgres>) -> Result<(), SqlxError> {
	let exists = index_exists(pool, "idx_tx_out_address").await?;
	if exists {
		info!("Index 'idx_tx_out_address' already exists");
	} else {
		let sql = "CREATE INDEX IF NOT EXISTS idx_tx_out_address ON tx_out USING hash (address)";
		info!("Executing '{}', this might take a long time", sql);
		sqlx::query(sql).execute(pool).await?;
		info!("Index 'idx_tx_out_address' has been created");
	}
	Ok(())
}

/// Check if the index exists.
async fn index_exists(pool: &Pool<Postgres>, index_name: &str) -> Result<bool, sqlx::Error> {
	sqlx::query("select * from pg_indexes where indexname = $1")
		.bind(index_name)
		.fetch_all(pool)
		.await
		.map(|rows| rows.len() == 1)
}

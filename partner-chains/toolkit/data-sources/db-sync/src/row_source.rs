//! Cardano queries of the data sources, implemented for Db-Sync by running them on Postgres
use crate::SqlxError;
pub use crate::db_model::Block;
#[cfg(any(feature = "block-source", feature = "bridge"))]
use crate::db_model::BlockNumber;
#[cfg(feature = "block-source")]
use crate::db_model::SlotNumber;
#[cfg(feature = "bridge")]
use crate::db_model::{Address, Asset, DbSyncConfigurationProvider, TxHash};
#[cfg(feature = "bridge")]
pub use crate::db_model::{BridgeTx, NativeTokenAmount, ResolvedBridgeDataCheckpoint, TxBlockInfo};
#[cfg(feature = "block-source")]
use chrono::NaiveDateTime;
#[cfg(feature = "block-source")]
use sidechain_domain::McBlockHash;
#[cfg(feature = "block-source")]
use sqlx::PgPool;

/// Error returned by a row source
pub enum RowSourceError {
	/// Error of a Db-Sync query
	DbSync(SqlxError),
	/// Error of another row source implementation
	Other(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Debug for RowSourceError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::DbSync(e) => e.fmt(f),
			Self::Other(e) => e.fmt(f),
		}
	}
}

impl From<SqlxError> for RowSourceError {
	fn from(e: SqlxError) -> Self {
		Self::DbSync(e)
	}
}

impl From<RowSourceError> for Box<dyn std::error::Error + Send + Sync> {
	fn from(e: RowSourceError) -> Self {
		match e {
			RowSourceError::DbSync(e) => e.into(),
			RowSourceError::Other(e) => e,
		}
	}
}

/// Cardano block queries used by the block, MC hash, sidechain RPC and bridge data sources
#[cfg(feature = "block-source")]
#[async_trait::async_trait]
pub trait BlockRowSource: Send + Sync + 'static {
	/// Returns the block with the highest block number
	async fn get_latest_block_info(&self) -> Result<Option<Block>, RowSourceError>;

	/// Returns the blocks with numbers in range `from..=to`, ordered by block number
	async fn get_blocks_by_numbers(
		&self,
		from: BlockNumber,
		to: BlockNumber,
	) -> Result<Vec<Block>, RowSourceError>;

	/// Returns the highest block not higher than `max_block_number`, with slot in
	/// `min_slot..=max_slot` and time in `min_time..=max_time`
	async fn get_highest_block(
		&self,
		max_block_number: BlockNumber,
		min_time: NaiveDateTime,
		min_slot: SlotNumber,
		max_time: NaiveDateTime,
		max_slot: SlotNumber,
	) -> Result<Option<Block>, RowSourceError>;

	/// Returns the block with the given hash
	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<Option<Block>, RowSourceError>;
}

#[cfg(feature = "block-source")]
#[async_trait::async_trait]
impl BlockRowSource for PgPool {
	async fn get_latest_block_info(&self) -> Result<Option<Block>, RowSourceError> {
		Ok(crate::db_model::get_latest_block_info(self).await?)
	}

	async fn get_blocks_by_numbers(
		&self,
		from: BlockNumber,
		to: BlockNumber,
	) -> Result<Vec<Block>, RowSourceError> {
		Ok(crate::db_model::get_blocks_by_numbers(self, from, to).await?)
	}

	async fn get_highest_block(
		&self,
		max_block_number: BlockNumber,
		min_time: NaiveDateTime,
		min_slot: SlotNumber,
		max_time: NaiveDateTime,
		max_slot: SlotNumber,
	) -> Result<Option<Block>, RowSourceError> {
		Ok(crate::db_model::get_highest_block(
			self,
			max_block_number,
			min_time,
			min_slot,
			max_time,
			max_slot,
		)
		.await?)
	}

	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<Option<Block>, RowSourceError> {
		Ok(crate::db_model::get_block_by_hash(self, hash).await?)
	}
}

/// Cardano transaction queries used by the token bridge data sources
#[cfg(feature = "bridge")]
#[async_trait::async_trait]
pub trait BridgeRowSource: Send + Sync + 'static {
	/// Returns the block number and index in block of the transaction with the given hash
	async fn get_block_info_for_tx_hash(
		&self,
		tx_hash: TxHash,
	) -> Result<Option<TxBlockInfo>, RowSourceError>;

	/// Returns the bridge transactions after `checkpoint` up to block `to_block`, ordered by position
	async fn get_bridge_txs(
		&self,
		ics_address: &Address,
		reserve_address: &Address,
		native_token: Asset,
		checkpoint: ResolvedBridgeDataCheckpoint,
		to_block: BlockNumber,
		max_txs: Option<u32>,
	) -> Result<Vec<BridgeTx>, RowSourceError>;
}

#[cfg(feature = "bridge")]
#[async_trait::async_trait]
impl BridgeRowSource for DbSyncConfigurationProvider {
	async fn get_block_info_for_tx_hash(
		&self,
		tx_hash: TxHash,
	) -> Result<Option<TxBlockInfo>, RowSourceError> {
		Ok(crate::db_model::get_block_info_for_tx_hash(&self.pool, tx_hash).await?)
	}

	async fn get_bridge_txs(
		&self,
		ics_address: &Address,
		reserve_address: &Address,
		native_token: Asset,
		checkpoint: ResolvedBridgeDataCheckpoint,
		to_block: BlockNumber,
		max_txs: Option<u32>,
	) -> Result<Vec<BridgeTx>, RowSourceError> {
		let tx_in_configuration =
			self.get_tx_in_config().await.map_err(|e| RowSourceError::Other(e.into()))?;
		Ok(crate::db_model::get_bridge_txs(
			tx_in_configuration,
			&self.pool,
			ics_address,
			reserve_address,
			native_token,
			checkpoint,
			to_block,
			max_txs,
		)
		.await?)
	}
}

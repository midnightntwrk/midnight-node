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

//! Cardano queries of the candidates data source

use super::db_model::{self, DataSourceError, DbSyncConfigurationProvider, SqlxError};
pub use super::db_model::{
	Block, MainchainEpochNonce, MainchainTxOutput, StakePoolEntry, TokenTxOutput,
};
use crate::db::MultiAssetCache;
use crate::db::RowSourceError;
use db_sync_sqlx::{Address, Asset, BlockNumber, EpochNumber};
use sqlx::PgPool;

/// Cardano queries of the candidates data source
#[async_trait::async_trait]
pub trait CandidatesRowSource: Send + Sync + 'static {
	/// The block with the highest slot in an epoch not after `epoch`
	async fn get_latest_block_for_epoch(
		&self,
		epoch: EpochNumber,
	) -> Result<Option<Block>, RowSourceError>;

	/// The latest epoch that is stable given the security parameter
	async fn get_latest_stable_epoch(
		&self,
		security_parameter: u32,
	) -> Result<Option<EpochNumber>, RowSourceError>;

	/// Stake per pool in `epoch`
	async fn get_stake_distribution(
		&self,
		epoch: EpochNumber,
	) -> Result<Vec<StakePoolEntry>, RowSourceError>;

	/// The most recent output carrying `asset`, created in an epoch not after `epoch`
	async fn get_token_utxo_for_epoch(
		&self,
		asset: &Asset,
		epoch: EpochNumber,
	) -> Result<Option<TokenTxOutput>, RowSourceError>;

	/// Nonce of `epoch`
	async fn get_epoch_nonce(
		&self,
		epoch: EpochNumber,
	) -> Result<Option<MainchainEpochNonce>, RowSourceError>;

	/// Outputs at `address` that are unspent at the end of block `block`
	async fn get_utxos_for_address(
		&self,
		address: &Address,
		block: BlockNumber,
	) -> Result<Vec<MainchainTxOutput>, RowSourceError>;
}

impl From<SqlxError> for RowSourceError {
	fn from(e: SqlxError) -> Self {
		Self::Other(e.into())
	}
}

impl From<DataSourceError> for RowSourceError {
	fn from(e: DataSourceError) -> Self {
		Self::Other(e.into())
	}
}

/// db-sync row source of the candidates data source
pub struct DbSyncCandidatesRows {
	/// Postgres connection pool
	pool: PgPool,
	/// Configuration used by Db-Sync
	db_sync_config: DbSyncConfigurationProvider,
	/// Cache for resolving multi_asset.id from (policy, name) pairs
	multi_asset_cache: MultiAssetCache,
}

impl DbSyncCandidatesRows {
	/// Creates a row source querying the given db-sync database
	pub(crate) fn new(pool: PgPool) -> Self {
		let multi_asset_cache = MultiAssetCache::new(pool.clone());
		Self {
			pool: pool.clone(),
			db_sync_config: DbSyncConfigurationProvider::new(pool),
			multi_asset_cache,
		}
	}
}

#[async_trait::async_trait]
impl CandidatesRowSource for DbSyncCandidatesRows {
	async fn get_latest_block_for_epoch(
		&self,
		epoch: EpochNumber,
	) -> Result<Option<Block>, RowSourceError> {
		Ok(db_model::get_latest_block_for_epoch(&self.pool, epoch).await?)
	}

	async fn get_latest_stable_epoch(
		&self,
		security_parameter: u32,
	) -> Result<Option<EpochNumber>, RowSourceError> {
		Ok(db_model::get_latest_stable_epoch(&self.pool, security_parameter).await?)
	}

	async fn get_stake_distribution(
		&self,
		epoch: EpochNumber,
	) -> Result<Vec<StakePoolEntry>, RowSourceError> {
		Ok(db_model::get_stake_distribution(&self.pool, epoch).await?)
	}

	async fn get_token_utxo_for_epoch(
		&self,
		asset: &Asset,
		epoch: EpochNumber,
	) -> Result<Option<TokenTxOutput>, RowSourceError> {
		let ident = self
			.multi_asset_cache
			.resolve_ident(&asset.policy_id.0, &asset.asset_name.0)
			.await?;
		Ok(match ident {
			Some(id) => db_model::get_token_utxo_for_epoch(&self.pool, id, epoch).await?,
			None => None,
		})
	}

	async fn get_epoch_nonce(
		&self,
		epoch: EpochNumber,
	) -> Result<Option<MainchainEpochNonce>, RowSourceError> {
		Ok(db_model::get_epoch_nonce(&self.pool, epoch).await?)
	}

	async fn get_utxos_for_address(
		&self,
		address: &Address,
		block: BlockNumber,
	) -> Result<Vec<MainchainTxOutput>, RowSourceError> {
		Ok(db_model::get_utxos_for_address(
			&self.pool,
			address,
			block,
			self.db_sync_config.get_tx_in_config().await?,
		)
		.await?)
	}
}

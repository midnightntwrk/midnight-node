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

//! Cardano queries of the data sources, implemented for db-sync by the queries of this module

use crate::db::{
	AssetCreateRow, AssetSpendRow, Block, DeregistrationRow, GovernanceBodyUtxoRow,
	MultiAssetCache, PagedQuery, QueryBounds, RegistrationRow,
};
use sidechain_domain::{McBlockHash, PolicyId};
use sqlx::PgPool;
use std::fmt::Debug;

/// Error returned by a row source
pub enum RowSourceError {
	/// Error of a db-sync query
	DbSync(sqlx::Error),
	/// Error of another row source implementation
	Other(Box<dyn std::error::Error + Send + Sync>),
}

impl Debug for RowSourceError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::DbSync(e) => e.fmt(f),
			Self::Other(e) => e.fmt(f),
		}
	}
}

impl From<sqlx::Error> for RowSourceError {
	fn from(e: sqlx::Error) -> Self {
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

/// Cardano block lookup shared by the cNIGHT and federated authority observation data sources
#[async_trait::async_trait]
pub trait McBlockByHashRowSource: Send + Sync + 'static {
	/// Returns the block with the given hash
	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<Option<Block>, RowSourceError>;
}

/// Cardano queries of the cNIGHT observation data source
#[async_trait::async_trait]
pub trait CNightObservationRowSource: McBlockByHashRowSource {
	/// Identifier of a native asset in the queries
	type AssetIdent: Debug + Send + Sync;
	/// Pre-filter bounds of a [PagedQuery]
	type Bounds: Copy + Debug + Send + Sync;

	/// `None` if no such asset is known
	async fn resolve_asset(
		&self,
		policy: &[u8],
		name: &[u8],
	) -> Result<Option<Self::AssetIdent>, RowSourceError>;

	/// Bounds of rows made before block `block_no`
	async fn get_low_bounds(&self, block_no: i64) -> Result<Option<Self::Bounds>, RowSourceError>;

	/// Bounds of rows made after block `block_no`
	async fn get_high_bounds(&self, block_no: i64) -> Result<Option<Self::Bounds>, RowSourceError>;

	/// `maxTxSize`, in bytes, of the Cardano epoch holding block `block_no`
	async fn get_max_tx_size(&self, block_no: u32) -> Result<Option<i64>, RowSourceError>;

	/// Outputs at `address` carrying exactly one `auth_token`, created in the query window
	async fn get_registrations(
		&self,
		address: &str,
		auth_token: &Self::AssetIdent,
		query: &PagedQuery<'_, Self::Bounds>,
	) -> Result<Vec<RegistrationRow>, RowSourceError>;

	/// Outputs at `address` spent in the query window
	async fn get_deregistrations(
		&self,
		address: &str,
		query: &PagedQuery<'_, Self::Bounds>,
	) -> Result<Vec<DeregistrationRow>, RowSourceError>;

	/// Outputs carrying `asset`, created in the query window
	async fn get_asset_creates(
		&self,
		asset: &Self::AssetIdent,
		query: &PagedQuery<'_, Self::Bounds>,
	) -> Result<Vec<AssetCreateRow>, RowSourceError>;

	/// Outputs carrying `asset`, spent in the query window
	async fn get_asset_spends(
		&self,
		asset: &Self::AssetIdent,
		query: &PagedQuery<'_, Self::Bounds>,
	) -> Result<Vec<AssetSpendRow>, RowSourceError>;
}

/// Cardano queries of the federated authority observation data source
#[async_trait::async_trait]
pub trait FederatedAuthorityRowSource: McBlockByHashRowSource {
	/// The most recent output at `script_address` carrying a token of `policy_id`, created
	/// up to block `block_number`
	async fn get_governance_body_utxo(
		&self,
		script_address: &str,
		policy_id: &PolicyId,
		block_number: u32,
	) -> Result<Option<GovernanceBodyUtxoRow>, RowSourceError>;
}

#[async_trait::async_trait]
impl McBlockByHashRowSource for PgPool {
	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<Option<Block>, RowSourceError> {
		Ok(crate::db::get_block_by_hash(self, hash).await?)
	}
}

#[async_trait::async_trait]
impl FederatedAuthorityRowSource for PgPool {
	async fn get_governance_body_utxo(
		&self,
		script_address: &str,
		policy_id: &PolicyId,
		block_number: u32,
	) -> Result<Option<GovernanceBodyUtxoRow>, RowSourceError> {
		Ok(crate::db::get_governance_body_utxo(self, script_address, policy_id, block_number)
			.await?)
	}
}

/// db-sync row source of the cNIGHT observation data source
pub struct DbSyncCNightRows {
	pool: PgPool,
	multi_asset_cache: MultiAssetCache,
}

impl DbSyncCNightRows {
	pub(crate) fn new(pool: PgPool) -> Self {
		let multi_asset_cache = MultiAssetCache::new(pool.clone());
		Self { pool, multi_asset_cache }
	}
}

#[async_trait::async_trait]
impl McBlockByHashRowSource for DbSyncCNightRows {
	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<Option<Block>, RowSourceError> {
		self.pool.get_block_by_hash(hash).await
	}
}

#[async_trait::async_trait]
impl CNightObservationRowSource for DbSyncCNightRows {
	/// `multi_asset.id`
	type AssetIdent = i64;
	type Bounds = QueryBounds;

	async fn resolve_asset(
		&self,
		policy: &[u8],
		name: &[u8],
	) -> Result<Option<i64>, RowSourceError> {
		Ok(self.multi_asset_cache.resolve_ident(policy, name).await?)
	}

	async fn get_low_bounds(&self, block_no: i64) -> Result<Option<QueryBounds>, RowSourceError> {
		Ok(crate::db::get_low_bounds(&self.pool, block_no).await?)
	}

	async fn get_high_bounds(&self, block_no: i64) -> Result<Option<QueryBounds>, RowSourceError> {
		Ok(crate::db::get_high_bounds(&self.pool, block_no).await?)
	}

	async fn get_max_tx_size(&self, block_no: u32) -> Result<Option<i64>, RowSourceError> {
		Ok(crate::db::get_max_tx_size(&self.pool, block_no).await?)
	}

	async fn get_registrations(
		&self,
		address: &str,
		auth_token: &i64,
		query: &PagedQuery<'_>,
	) -> Result<Vec<RegistrationRow>, RowSourceError> {
		Ok(crate::db::get_registrations(&self.pool, address, *auth_token, query).await?)
	}

	async fn get_deregistrations(
		&self,
		address: &str,
		query: &PagedQuery<'_>,
	) -> Result<Vec<DeregistrationRow>, RowSourceError> {
		Ok(crate::db::get_deregistrations(&self.pool, address, query).await?)
	}

	async fn get_asset_creates(
		&self,
		asset: &i64,
		query: &PagedQuery<'_>,
	) -> Result<Vec<AssetCreateRow>, RowSourceError> {
		Ok(crate::db::get_asset_creates(&self.pool, *asset, query).await?)
	}

	async fn get_asset_spends(
		&self,
		asset: &i64,
		query: &PagedQuery<'_>,
	) -> Result<Vec<AssetSpendRow>, RowSourceError> {
		Ok(crate::db::get_asset_spends(&self.pool, *asset, query).await?)
	}
}

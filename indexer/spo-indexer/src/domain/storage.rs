// This file is part of midnight-indexer.
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

use crate::domain::{
    Epoch, PoolMetadata, SPO, SPOEpochPerformance, SPOHistory, ValidatorMembership,
};

#[cfg(feature = "cloud")]
/// Sqlx transaction for Postgres.
pub type SqlxTransaction = sqlx::Transaction<'static, sqlx::Postgres>;

#[cfg(feature = "standalone")]
/// Sqlx transaction for Sqlite.
pub type SqlxTransaction = sqlx::Transaction<'static, sqlx::Sqlite>;

#[cfg(not(any(feature = "cloud", feature = "standalone")))]
/// Default to Postgres when no feature is explicitly enabled (workspace builds).
pub type SqlxTransaction = sqlx::Transaction<'static, sqlx::Postgres>;

/// Storage abstraction.
#[trait_variant::make(Send)]
pub trait Storage
where
    Self: Clone + Send + Sync + 'static,
{
    async fn create_tx(&self) -> Result<SqlxTransaction, sqlx::Error>;

    async fn get_latest_epoch(&self) -> Result<Option<Epoch>, sqlx::Error>;

    async fn save_epoch(&self, epoch: &Epoch, tx: &mut SqlxTransaction) -> Result<(), sqlx::Error>;

    async fn save_spo(&self, spo: &SPO, tx: &mut SqlxTransaction) -> Result<(), sqlx::Error>;

    async fn save_membership(
        &self,
        memberships: &[ValidatorMembership],
        tx: &mut SqlxTransaction,
    ) -> Result<(), sqlx::Error>;

    async fn save_spo_performance(
        &self,
        metadata: &SPOEpochPerformance,
        tx: &mut SqlxTransaction,
    ) -> Result<(), sqlx::Error>;

    async fn save_pool_meta(
        &self,
        metadata: &PoolMetadata,
        tx: &mut SqlxTransaction,
    ) -> Result<(), sqlx::Error>;

    async fn save_spo_history(
        &self,
        history: &SPOHistory,
        tx: &mut SqlxTransaction,
    ) -> Result<(), sqlx::Error>;

    /// Return a page of pool_ids known to the system (for stake refreshers).
    /// Implementations should order by most recently updated metadata first when possible.
    async fn get_pool_ids(&self, limit: i64, offset: i64) -> Result<Vec<String>, sqlx::Error>;

    /// Return pool_ids after a given id, lexicographically, for cursor-based rotation.
    async fn get_pool_ids_after(&self, after: &str, limit: i64)
    -> Result<Vec<String>, sqlx::Error>;

    /// Upsert latest stake snapshot for a pool.
    #[allow(clippy::too_many_arguments)]
    async fn save_stake_snapshot(
        &self,
        pool_id: &str,
        live_stake: Option<i64>,
        active_stake: Option<i64>,
        live_delegators: Option<i64>,
        live_saturation: Option<f64>,
        declared_pledge: Option<i64>,
        live_pledge: Option<i64>,
        tx: &mut SqlxTransaction,
    ) -> Result<(), sqlx::Error>;

    /// Append a history row for stake.
    #[allow(clippy::too_many_arguments)]
    async fn insert_stake_history(
        &self,
        pool_id: &str,
        mainchain_epoch: Option<i64>,
        live_stake: Option<i64>,
        active_stake: Option<i64>,
        live_delegators: Option<i64>,
        live_saturation: Option<f64>,
        declared_pledge: Option<i64>,
        live_pledge: Option<i64>,
        tx: &mut SqlxTransaction,
    ) -> Result<(), sqlx::Error>;

    /// Get the timestamp of a block by height (sourced by chain-indexer).
    async fn get_block_timestamp(&self, height: i64) -> Result<Option<i64>, sqlx::Error>;

    /// Refresh cursor helpers.
    async fn get_stake_refresh_cursor(&self) -> Result<Option<String>, sqlx::Error>;
    async fn set_stake_refresh_cursor(&self, pool_id: Option<&str>) -> Result<(), sqlx::Error>;
}

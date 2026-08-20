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
    spo::{
        CommitteeMember, EpochInfo, EpochPerf, FirstValidEpoch, PoolMetadata, PresenceEvent,
        RegisteredStat, RegisteredTotals, Spo, SpoComposite, SpoIdentity, StakeShare,
    },
    storage::NoopStorage,
};

/// Storage abstraction for SPO data.
#[trait_variant::make(Send)]
pub trait SpoStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get SPO identities with pagination.
    async fn get_spo_identities(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SpoIdentity>, sqlx::Error>;

    /// Get SPO identity by pool ID.
    async fn get_spo_identity_by_pool_id(
        &self,
        pool_id: &str,
    ) -> Result<Option<SpoIdentity>, sqlx::Error>;

    /// Get total count of SPOs.
    async fn get_spo_count(&self) -> Result<i64, sqlx::Error>;

    /// Get pool metadata by pool ID.
    async fn get_pool_metadata(&self, pool_id: &str) -> Result<Option<PoolMetadata>, sqlx::Error>;

    /// Get pool metadata list with pagination.
    async fn get_pool_metadata_list(
        &self,
        limit: i64,
        offset: i64,
        with_name_only: bool,
    ) -> Result<Vec<PoolMetadata>, sqlx::Error>;

    /// Get SPO with metadata by pool ID.
    async fn get_spo_by_pool_id(&self, pool_id: &str) -> Result<Option<Spo>, sqlx::Error>;

    /// Get SPO list with optional search.
    async fn get_spo_list(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Spo>, sqlx::Error>;

    /// Get composite SPO data (identity + metadata + performance).
    async fn get_spo_composite_by_pool_id(
        &self,
        pool_id: &str,
        perf_limit: i64,
    ) -> Result<Option<SpoComposite>, sqlx::Error>;

    /// Get SPO identifiers ordered by performance.
    async fn get_stake_pool_operator_ids(&self, limit: i64) -> Result<Vec<String>, sqlx::Error>;

    /// Get latest SPO performance entries.
    async fn get_spo_performance_latest(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error>;

    /// Get SPO performance by SPO key.
    async fn get_spo_performance_by_spo_sk(
        &self,
        spo_sk: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error>;

    /// Get epoch performance for all SPOs.
    async fn get_epoch_performance(
        &self,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error>;

    /// Get current epoch information.
    async fn get_current_epoch_info(&self) -> Result<Option<EpochInfo>, sqlx::Error>;

    /// Get epoch utilization (produced/expected ratio).
    async fn get_epoch_utilization(&self, epoch: i64) -> Result<Option<f64>, sqlx::Error>;

    /// Get committee membership for an epoch.
    async fn get_committee(&self, epoch: i64) -> Result<Vec<CommitteeMember>, sqlx::Error>;

    /// Get cumulative registration totals for an epoch range.
    async fn get_registered_totals_series(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<RegisteredTotals>, sqlx::Error>;

    /// Get registration statistics for an epoch range.
    async fn get_registered_spo_series(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<RegisteredStat>, sqlx::Error>;

    /// Get raw presence events for an epoch range.
    async fn get_registered_presence(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<PresenceEvent>, sqlx::Error>;

    /// Get first valid epoch for each SPO identity.
    async fn get_registered_first_valid_epochs(
        &self,
        upto_epoch: Option<i64>,
    ) -> Result<Vec<FirstValidEpoch>, sqlx::Error>;

    /// Get stake distribution with search and ordering.
    /// Returns (stake_shares, total_live_stake).
    async fn get_stake_distribution(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
        order_desc: bool,
    ) -> Result<(Vec<StakeShare>, f64), sqlx::Error>;
}

#[allow(unused_variables)]
impl SpoStorage for NoopStorage {
    async fn get_spo_identities(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SpoIdentity>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_identity_by_pool_id(
        &self,
        pool_id: &str,
    ) -> Result<Option<SpoIdentity>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_count(&self) -> Result<i64, sqlx::Error> {
        unimplemented!()
    }

    async fn get_pool_metadata(&self, pool_id: &str) -> Result<Option<PoolMetadata>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_pool_metadata_list(
        &self,
        limit: i64,
        offset: i64,
        with_name_only: bool,
    ) -> Result<Vec<PoolMetadata>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_by_pool_id(&self, pool_id: &str) -> Result<Option<Spo>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_list(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Spo>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_composite_by_pool_id(
        &self,
        pool_id: &str,
        perf_limit: i64,
    ) -> Result<Option<SpoComposite>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_stake_pool_operator_ids(&self, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_performance_latest(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_spo_performance_by_spo_sk(
        &self,
        spo_sk: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_epoch_performance(
        &self,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_current_epoch_info(&self) -> Result<Option<EpochInfo>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_epoch_utilization(&self, epoch: i64) -> Result<Option<f64>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_committee(&self, epoch: i64) -> Result<Vec<CommitteeMember>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_registered_totals_series(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<RegisteredTotals>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_registered_spo_series(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<RegisteredStat>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_registered_presence(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<PresenceEvent>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_registered_first_valid_epochs(
        &self,
        upto_epoch: Option<i64>,
    ) -> Result<Vec<FirstValidEpoch>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_stake_distribution(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
        order_desc: bool,
    ) -> Result<(Vec<StakeShare>, f64), sqlx::Error> {
        unimplemented!()
    }
}

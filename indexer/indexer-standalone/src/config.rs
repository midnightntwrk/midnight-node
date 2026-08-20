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

use chain_indexer::{application as chain_app, infra::subxt_node};
use indexer_api::{application as api_app, infra::api};
use indexer_common::{domain::NetworkId, infra::pool, telemetry};
use serde::Deserialize;
use spo_indexer::{
    application::{self as spo_app, StakeRefreshConfig},
    infra::spo_client::{self, HttpPoolConfig},
};
use std::{num::NonZeroUsize, time::Duration};
use wallet_indexer::application as wallet_app;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(with = "byte_unit_serde")]
    pub thread_stack_size: u64,

    #[serde(rename = "application")]
    pub application_config: ApplicationConfig,

    #[serde(rename = "spo", default)]
    pub spo_config: SpoApplicationConfig,

    #[serde(rename = "infra")]
    pub infra_config: InfraConfig,

    #[serde(rename = "telemetry")]
    pub telemetry_config: telemetry::Config,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplicationConfig {
    pub network_id: NetworkId,
    pub blocks_buffer: usize,
    pub caught_up_max_distance: u32,
    pub caught_up_leeway: u32,
    #[serde(with = "humantime_serde", default = "gc_bound_default")]
    pub gc_bound: Duration,
    #[serde(default = "ledger_state_retention_default")]
    pub ledger_state_retention: NonZeroUsize,
    #[serde(with = "humantime_serde")]
    pub active_wallets_query_delay: Duration,
    #[serde(with = "humantime_serde")]
    pub active_wallets_ttl: Duration,
    pub transaction_batch_size: NonZeroUsize,
    #[serde(default = "concurrency_limit_default")]
    pub concurrency_limit: NonZeroUsize,
}

fn gc_bound_default() -> Duration {
    Duration::from_millis(200)
}

fn ledger_state_retention_default() -> NonZeroUsize {
    NonZeroUsize::new(1000).expect("1000 is not zero")
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpoApplicationConfig {
    #[serde(default = "spo_interval_default")]
    pub interval: u32,
    #[serde(default = "spo_stake_refresh_default")]
    pub stake_refresh: StakeRefreshConfig,
}

impl Default for SpoApplicationConfig {
    fn default() -> Self {
        Self {
            interval: spo_interval_default(),
            stake_refresh: spo_stake_refresh_default(),
        }
    }
}

impl From<ApplicationConfig> for chain_app::Config {
    fn from(config: ApplicationConfig) -> Self {
        let ApplicationConfig {
            network_id,
            blocks_buffer,
            caught_up_max_distance,
            caught_up_leeway,
            gc_bound,
            ledger_state_retention,
            ..
        } = config;

        Self {
            network_id,
            blocks_buffer,
            caught_up_max_distance,
            caught_up_leeway,
            gc_bound,
            ledger_state_retention,
        }
    }
}

impl From<ApplicationConfig> for api_app::Config {
    fn from(config: ApplicationConfig) -> Self {
        Self {
            network_id: config.network_id,
        }
    }
}

impl From<ApplicationConfig> for wallet_app::Config {
    fn from(config: ApplicationConfig) -> Self {
        let ApplicationConfig {
            active_wallets_query_delay,
            active_wallets_ttl,
            transaction_batch_size,
            concurrency_limit,
            ..
        } = config;

        Self {
            active_wallets_query_delay,
            active_wallets_ttl,
            transaction_batch_size,
            concurrency_limit,
        }
    }
}

impl From<SpoApplicationConfig> for spo_app::Config {
    fn from(config: SpoApplicationConfig) -> Self {
        Self {
            interval: config.interval,
            stake_refresh: config.stake_refresh,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfraConfig {
    pub run_migrations: bool,

    #[serde(rename = "storage")]
    pub storage_config: pool::sqlite::Config,

    #[serde(rename = "ledger_db")]
    pub ledger_db_config: indexer_common::infra::ledger_db::Config,

    #[serde(rename = "node")]
    pub node_config: subxt_node::Config,

    #[serde(rename = "spo_node")]
    pub spo_node_config: SpoNodeConfig,

    #[serde(rename = "api")]
    pub api_config: api::Config,

    pub secret: secrecy::SecretString,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpoNodeConfig {
    pub url: String,
    #[serde(alias = "blockfrostId")]
    pub blockfrost_id: String,
    #[serde(with = "humantime_serde")]
    pub reconnect_max_delay: Duration,
    pub reconnect_max_attempts: usize,
    #[serde(default)]
    pub http_pool: HttpPoolConfig,
}

impl From<SpoNodeConfig> for spo_client::Config {
    fn from(config: SpoNodeConfig) -> Self {
        Self {
            url: config.url,
            blockfrost_id: secrecy::SecretString::from(config.blockfrost_id),
            reconnect_max_delay: config.reconnect_max_delay,
            reconnect_max_attempts: config.reconnect_max_attempts,
            http_pool: config.http_pool,
        }
    }
}

fn concurrency_limit_default() -> NonZeroUsize {
    NonZeroUsize::MIN
}

fn spo_interval_default() -> u32 {
    5000
}

fn spo_stake_refresh_default() -> StakeRefreshConfig {
    StakeRefreshConfig {
        period_secs: 900,
        page_size: 100,
        max_rps: 2,
    }
}

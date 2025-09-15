// This file is part of midnight-indexer.
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

use indexer_common::{domain::NetworkId, infra::pool, telemetry};
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use std::{num::NonZeroUsize, time::Duration};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub run_migrations: bool,

    #[serde(rename = "application")]
    pub application_config: ApplicationConfig,

    #[serde(rename = "infra")]
    pub infra_config: InfraConfig,

    #[serde(rename = "telemetry")]
    pub telemetry_config: telemetry::Config,
}

#[serde_as]
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ApplicationConfig {
    #[serde_as(as = "DisplayFromStr")]
    pub network_id: NetworkId,
    pub blocks_buffer: usize,
    pub save_ledger_state_after: u32,
    pub caught_up_max_distance: u32,
    pub caught_up_leeway: u32,
    #[serde(with = "humantime_serde")]
    pub active_wallets_repeat_delay: Duration,
    #[serde(with = "humantime_serde")]
    pub active_wallets_ttl: Duration,
    pub transaction_batch_size: NonZeroUsize,
    #[serde(default = "parallelism_default")]
    pub parallelism: NonZeroUsize,
}

impl From<ApplicationConfig> for chain_indexer::application::Config {
    fn from(config: ApplicationConfig) -> Self {
        let ApplicationConfig {
            network_id,
            blocks_buffer,
            save_ledger_state_after,
            caught_up_max_distance,
            caught_up_leeway,
            ..
        } = config;

        Self {
            network_id,
            blocks_buffer,
            save_ledger_state_after,
            caught_up_max_distance,
            caught_up_leeway,
        }
    }
}

impl From<ApplicationConfig> for indexer_api::application::Config {
    fn from(config: ApplicationConfig) -> Self {
        Self {
            network_id: config.network_id,
        }
    }
}

impl From<ApplicationConfig> for wallet_indexer::application::Config {
    fn from(config: ApplicationConfig) -> Self {
        let ApplicationConfig {
            active_wallets_repeat_delay,
            active_wallets_ttl,
            transaction_batch_size,
            parallelism,
            ..
        } = config;

        Self {
            active_wallets_repeat_delay,
            active_wallets_ttl,
            transaction_batch_size,
            parallelism,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfraConfig {
    #[serde(rename = "storage")]
    pub storage_config: pool::sqlite::Config,

    #[serde(rename = "node")]
    pub node_config: chain_indexer::infra::subxt_node::Config,

    #[serde(rename = "api")]
    pub api_config: indexer_api::infra::api::Config,

    pub secret: secrecy::SecretString,
}

fn parallelism_default() -> NonZeroUsize {
    std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

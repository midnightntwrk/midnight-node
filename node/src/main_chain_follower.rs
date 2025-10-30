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

use authority_selection_inherents::AuthoritySelectionDataSource;
use pallet_sidechain_rpc::SidechainRpcDataSource;
use partner_chains_db_sync_data_sources::{
	BlockDataSourceImpl, CandidatesDataSourceImpl, DbSyncBlockDataSourceConfig,
	GovernedMapDataSourceCachedImpl, McFollowerMetrics, McHashDataSourceImpl,
	SidechainRpcDataSourceImpl,
};
use partner_chains_mock_data_sources::{
	AuthoritySelectionDataSourceMock, BlockDataSourceMock, GovernedMapDataSourceMock,
	McHashDataSourceMock, SidechainRpcDataSourceMock,
};
use sc_service::error::Error as ServiceError;
use sidechain_mc_hash::McHashDataSource;
use sp_governed_map::GovernedMapDataSource;

use super::cfg::midnight_cfg::MidnightCfg;
use partner_chains_mock_data_sources::MockRegistrationsConfig;
use sidechain_domain::mainchain_epoch::{Duration, MainchainEpochConfig, Timestamp};
use std::{error::Error, str::FromStr as _, sync::Arc};

use midnight_primitives_mainchain_follower::{
	CNightObservationDataSourceMock, FederatedAuthorityObservationDataSource,
	FederatedAuthorityObservationDataSourceImpl, FederatedAuthorityObservationDataSourceMock,
	MidnightCNightObservationDataSource, MidnightCNightObservationDataSourceImpl,
};

// TODO: Decide if it should be experimental
// #[cfg(feature = "experimental")]

#[derive(Clone)]
pub struct DataSources {
	pub mc_hash: Arc<dyn McHashDataSource + Send + Sync>,
	pub authority_selection: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	pub cnight_observation: Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	pub sidechain_rpc: Arc<dyn SidechainRpcDataSource + Send + Sync>,
	pub governed_map: Arc<dyn GovernedMapDataSource + Send + Sync>,
	pub federated_authority_observation:
		Arc<dyn FederatedAuthorityObservationDataSource + Send + Sync>,
}

pub(crate) async fn create_cached_main_chain_follower_data_sources(
	cfg: MidnightCfg,
	metrics_opt: Option<McFollowerMetrics>,
) -> std::result::Result<DataSources, ServiceError> {
	if cfg.use_main_chain_follower_mock {
		let mock = create_mock_data_sources(cfg.clone()).await.map_err(|err| {
			ServiceError::Application(
				format!("Failed to create main chain follower mock: {err}. Check configuration.")
					.into(),
			)
		})?;

		Ok(mock)
	} else {
		create_cached_data_sources(cfg, metrics_opt).await.map_err(|err| {
			ServiceError::Application(
				format!("Failed to create db-sync main chain follower: {err}").into(),
			)
		})
	}
}

pub async fn create_mock_data_sources(
	cfg: MidnightCfg,
) -> std::result::Result<DataSources, Box<dyn Error + Send + Sync + 'static>> {
	let block = Arc::new(BlockDataSourceMock::new(cfg.mc_epoch_duration_millis as u32));

	let authority_selection_data_source_mock = AuthoritySelectionDataSourceMock {
		registrations_data: MockRegistrationsConfig::read_registrations(
			&cfg.mock_registrations_file.ok_or(missing("mock_registrations_file"))?,
		)?,
	};

	Ok(DataSources {
		sidechain_rpc: Arc::new(SidechainRpcDataSourceMock::new(block.clone())),
		mc_hash: Arc::new(McHashDataSourceMock::new(block)),
		authority_selection: Arc::new(authority_selection_data_source_mock),
		cnight_observation: Arc::new(CNightObservationDataSourceMock::new()),
		governed_map: Arc::new(GovernedMapDataSourceMock::default()),
		federated_authority_observation: Arc::new(
			FederatedAuthorityObservationDataSourceMock::new(),
		),
	})
}

pub const CANDIDATES_FOR_EPOCH_CACHE_SIZE: usize = 64;
pub const GOVERNED_MAP_CACHE_SIZE: u16 = 100;

pub async fn create_cached_data_sources(
	cfg: MidnightCfg,
	metrics_opt: Option<McFollowerMetrics>,
) -> Result<DataSources, Box<dyn Error + Send + Sync + 'static>> {
	let pool = get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		std::time::Duration::from_secs(30),
	)
	.await?;

	log::info!("Creating idx_multi_asset_policy_name_hex index. This may take a while.");
	sqlx::query(
		r#"
			CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_multi_asset_policy_name_hex
			ON multi_asset ((encode(policy, 'hex')), (encode(name, 'hex')));
		"#,
	)
	.execute(&pool)
	.await?;

	let db_sync_block_data_source_config = DbSyncBlockDataSourceConfig {
		cardano_security_parameter: cfg
			.cardano_security_parameter
			.ok_or(missing("cardano_security_parameter"))?,
		cardano_active_slots_coeff: cfg
			.cardano_active_slots_coeff
			.ok_or(missing("cardano_active_slots_coeff"))?,
		block_stability_margin: cfg
			.block_stability_margin
			.ok_or(missing("block_stability_margin"))?,
	};

	let mc = MainchainEpochConfig {
		first_epoch_timestamp_millis: Timestamp::from_unix_millis(
			cfg.mc_first_epoch_timestamp_millis,
		),
		epoch_duration_millis: Duration::from_millis(cfg.mc_epoch_duration_millis),
		first_epoch_number: cfg.mc_first_epoch_number,
		first_slot_number: cfg.mc_first_slot_number,
		slot_duration_millis: Duration::from_millis(cfg.mc_slot_duration_millis),
	};

	let block = Arc::new(BlockDataSourceImpl::from_config(
		pool.clone(), // cfg.mc_epoch_duration_millis
		db_sync_block_data_source_config.clone(),
		&mc,
	));

	let candidates_data_source =
		CandidatesDataSourceImpl::new(pool.clone(), metrics_opt.clone()).await?;
	let candidates_data_source_cached =
		candidates_data_source.cached(CANDIDATES_FOR_EPOCH_CACHE_SIZE)?;

	Ok(DataSources {
		sidechain_rpc: Arc::new(SidechainRpcDataSourceImpl::new(
			block.clone(),
			metrics_opt.clone(),
		)),
		mc_hash: Arc::new(McHashDataSourceImpl::new(block.clone(), metrics_opt.clone())),
		authority_selection: Arc::new(candidates_data_source_cached),
		cnight_observation: Arc::new(MidnightCNightObservationDataSourceImpl::new(
			pool.clone(),
			metrics_opt.clone(),
			1000,
		)),
		governed_map: Arc::new(
			GovernedMapDataSourceCachedImpl::new(
				pool.clone(),
				metrics_opt.clone(),
				GOVERNED_MAP_CACHE_SIZE,
				block,
			)
			.await?,
		),
		federated_authority_observation: Arc::new(
			FederatedAuthorityObservationDataSourceImpl::new(pool, metrics_opt.clone(), 1000),
		),
	})
}

// Helper for users who only need native token observation data source
pub async fn create_cnight_observation_data_source(
	cfg: MidnightCfg,
	metrics_opt: Option<McFollowerMetrics>,
) -> Result<Arc<dyn MidnightCNightObservationDataSource>, Box<dyn Error + Send + Sync + 'static>> {
	let pool = get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		std::time::Duration::from_secs(30),
	)
	.await?;

	Ok(Arc::new(MidnightCNightObservationDataSourceImpl::new(
		pool.clone(),
		metrics_opt.clone(),
		1000,
	)))
}

// Copied from internal utility in partner-chains-db-sync-data-sources
async fn get_connection(
	connection_string: &str,
	acquire_timeout: std::time::Duration,
) -> Result<sqlx::PgPool, Box<dyn Error + Send + Sync + 'static>> {
	let connect_options = sqlx::postgres::PgConnectOptions::from_str(connection_string)?;
	let pool = sqlx::postgres::PgPoolOptions::new()
		.max_connections(5)
		.acquire_timeout(acquire_timeout)
		.connect_with(connect_options.clone())
		.await
		.map_err(|e| {
			PostgresConnectionError(
				connect_options.get_host().to_string(),
				connect_options.get_port(),
				connect_options.get_database().unwrap_or("cexplorer").to_string(),
				e.to_string(),
			)
			.to_string()
		})?;
	Ok(pool)
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Could not connect to database: postgres://***:***@{0}:{1}/{2}; error: {3}")]
struct PostgresConnectionError(String, u16, String, String);

fn missing(field: &str) -> sc_service::Error {
	ServiceError::Application(format!("Missing {field}. Check configuration.").into())
}

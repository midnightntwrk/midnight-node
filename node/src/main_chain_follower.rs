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

use authority_selection_inherents::authority_selection_inputs::AuthoritySelectionDataSource;
use db_sync_follower::{
	block::{BlockDataSourceImpl, DbSyncBlockDataSourceConfig},
	candidates::{CandidatesDataSourceImpl, cached::CandidateDataSourceCached},
	mc_hash::McHashDataSourceImpl,
	metrics::McFollowerMetrics,
	sidechain_rpc::SidechainRpcDataSourceImpl,
};
use main_chain_follower_mock::{
	block::BlockDataSourceMock, candidate::AuthoritySelectionDataSourceMock,
	mc_hash::McHashDataSourceMock, sidechain_rpc::SidechainRpcDataSourceMock,
};
use pallet_sidechain_rpc::SidechainRpcDataSource;
use sc_service::error::Error as ServiceError;
use sidechain_mc_hash::McHashDataSource;

use super::cfg::midnight_cfg::MidnightCfg;
use main_chain_follower_mock::candidate::MockRegistrationsConfig;
use sidechain_domain::mainchain_epoch::{Duration, MainchainEpochConfig, Timestamp};
use std::{error::Error, sync::Arc};

use midnight_primitives_mainchain_follower::{
	MidnightNativeTokenObservationDataSource, MidnightNativeTokenObservationDataSourceImpl,
	NativeTokenObservationDataSourceMock,
};

// TODO: Decide if it should be experimental
// #[cfg(feature = "experimental")]

#[derive(Clone)]
pub struct DataSources {
	pub mc_hash: Arc<dyn McHashDataSource + Send + Sync>,
	pub authority_selection: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	pub native_token: Arc<dyn MidnightNativeTokenObservationDataSource + Send + Sync>,
	pub sidechain_rpc: Arc<dyn SidechainRpcDataSource + Send + Sync>,
}

pub(crate) async fn create_cached_main_chain_follower_data_sources(
	cfg: MidnightCfg,
	metrics_opt: Option<McFollowerMetrics>,
) -> std::result::Result<DataSources, ServiceError> {
	if cfg.use_main_chain_follower_mock {
		create_mock_data_sources(cfg).await.map_err(|err| {
			ServiceError::Application(
				format!("Failed to create main chain follower mock: {err}. Check configuration.")
					.into(),
			)
		})
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

	let mc_epoch_config = MainchainEpochConfig {
		first_epoch_timestamp_millis: sp_core::offchain::Timestamp::from_unix_millis(
			cfg.mc_first_epoch_timestamp_millis,
		),
		epoch_duration_millis: sp_core::offchain::Duration::from_millis(
			cfg.mc_epoch_duration_millis,
		),
		first_epoch_number: cfg.mc_first_epoch_number,
		first_slot_number: cfg.mc_first_slot_number,
	};

	let authority_selection_data_source_mock = AuthoritySelectionDataSourceMock {
		registrations_data: MockRegistrationsConfig::read_registrations(
			cfg.main_chain_follower_mock_registrations_file
				.ok_or(missing("main_chain_follower_mock_registrations_file"))?,
		)?,
		mc_epoch_config,
	};

	Ok(DataSources {
		sidechain_rpc: Arc::new(SidechainRpcDataSourceMock::new(block.clone())),
		mc_hash: Arc::new(McHashDataSourceMock::new(block)),
		authority_selection: Arc::new(authority_selection_data_source_mock),
		native_token: Arc::new(NativeTokenObservationDataSourceMock::new()),
	})
}

pub const CANDIDATES_FOR_EPOCH_CACHE_SIZE: usize = 64;

pub async fn create_cached_data_sources(
	cfg: MidnightCfg,
	metrics_opt: Option<McFollowerMetrics>,
) -> Result<DataSources, Box<dyn Error + Send + Sync + 'static>> {
	let pool = db_sync_follower::data_sources::get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		std::time::Duration::from_secs(30),
	)
	.await?;

	log::info!("Creating idx_tx_out_address index. This may take a while.");
	// Note: temporary fix until after PC 1.6.1
	sqlx::query(
		r#"
		  CREATE INDEX IF NOT EXISTS idx_tx_out_address ON tx_out USING hash (address)
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
	};

	let block = Arc::new(BlockDataSourceImpl::from_config(
		pool.clone(), // cfg.mc_epoch_duration_millis
		db_sync_block_data_source_config.clone(),
		&mc,
	));

	let candidates_data_source =
		CandidatesDataSourceImpl::new(pool.clone(), metrics_opt.clone()).await?;
	let candidates_data_source_cached = CandidateDataSourceCached::new(
		candidates_data_source,
		CANDIDATES_FOR_EPOCH_CACHE_SIZE,
		cfg.cardano_security_parameter.ok_or(missing("cardano_security_parameter"))?,
	);

	Ok(DataSources {
		sidechain_rpc: Arc::new(SidechainRpcDataSourceImpl::new(
			block.clone(),
			metrics_opt.clone(),
		)),
		mc_hash: Arc::new(McHashDataSourceImpl::new(block, metrics_opt.clone())),
		authority_selection: Arc::new(candidates_data_source_cached),
		native_token: Arc::new(MidnightNativeTokenObservationDataSourceImpl::new(
			pool,
			metrics_opt,
			db_sync_block_data_source_config,
			1000,
		)),
	})
}

fn missing(field: &str) -> sc_service::Error {
	ServiceError::Application(format!("Missing {}. Check configuration.", field).into())
}

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

//! Blockfrost-backed implementations of the main chain follower data sources.
//!
//! When `blockfrost_endpoint` is configured, [create_blockfrost_data_sources] builds all six
//! data sources from a Blockfrost-compatible API instead of the db-sync Postgres database.
//! Every implementation replicates the corresponding db-sync SQL semantics exactly — the
//! results are consensus-critical inherent data and must match byte-for-byte what
//! db-sync-backed nodes produce.
//!
//! Not covered here (still Postgres-only): the CLI/genesis helpers in `main_chain_follower.rs`
//! (`create_ics_genesis_pool` and friends).
//!
//! Timing: every trait method and every HTTP call logs its duration under the `blockfrost`
//! log target (`-l blockfrost=debug`) so latencies can be compared with the db-sync
//! Prometheus histograms.

mod authority_selection;
mod block;
mod bridge;
mod client;
mod cnight;
mod convert;
mod federated_authority;
mod support;
#[cfg(test)]
mod testing;

pub use authority_selection::BlockfrostAuthoritySelectionDataSource;
pub use block::BlockfrostBlockDataSource;
pub use bridge::BlockfrostTokenBridgeDataSource;
pub use client::BlockfrostClient;
pub use cnight::BlockfrostCNightObservationDataSource;
pub use federated_authority::BlockfrostFederatedAuthorityObservationDataSource;

use std::sync::Arc;

use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition};
use midnight_primitives_mainchain_follower::data_source::metrics::MidnightDataSourceMetrics;

use self::support::*;
use crate::cfg::midnight_cfg::MidnightCfg;
use crate::main_chain_follower::DataSources;

// ---------------------------------------------------------------------------
// Wiring
// ---------------------------------------------------------------------------

/// Builds all six main chain follower data sources backed by the configured
/// Blockfrost-compatible endpoint.
pub async fn create_blockfrost_data_sources(
	cfg: MidnightCfg,
	// Unused: the per-call implementation needs no cache anchor.
	_cnight_follower_genesis: Option<(CNightAddresses, CardanoPosition)>,
	midnight_metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<DataSources, BoxError> {
	let endpoint = cfg.blockfrost_endpoint.as_deref().ok_or("blockfrost_endpoint not set")?;
	let security_parameter =
		cfg.cardano_security_parameter.ok_or("Missing cardano_security_parameter")?;
	let active_slots_coeff =
		cfg.cardano_active_slots_coeff.ok_or("Missing cardano_active_slots_coeff")?;
	let block_stability_margin =
		cfg.block_stability_margin.ok_or("Missing block_stability_margin")?;

	let client = Arc::new(BlockfrostClient::new(
		endpoint,
		cfg.blockfrost_project_id.as_deref(),
		security_parameter,
	)?);
	log::info!("Main chain follower backend: Blockfrost ({endpoint})");

	let block_source = Arc::new(BlockfrostBlockDataSource::new(
		client.clone(),
		security_parameter,
		active_slots_coeff,
		block_stability_margin,
		cfg.mc_slot_duration_millis,
	));

	Ok(DataSources {
		mc_hash: block_source.clone(),
		sidechain_rpc: block_source,
		authority_selection: Arc::new(BlockfrostAuthoritySelectionDataSource::new(
			client.clone(),
			security_parameter,
			midnight_metrics_opt.clone(),
		)),
		cnight_observation: Arc::new(BlockfrostCNightObservationDataSource::new(
			client.clone(),
			security_parameter,
			cfg.cnight_observation_window_size,
			midnight_metrics_opt.clone(),
		)),
		federated_authority_observation: Arc::new(
			BlockfrostFederatedAuthorityObservationDataSource::new(
				client.clone(),
				midnight_metrics_opt,
			),
		),
		bridge: Arc::new(BlockfrostTokenBridgeDataSource::new(client)),
	})
}

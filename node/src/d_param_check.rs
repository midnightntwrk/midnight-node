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

//! Startup check for the D-parameter vs available permissioned candidates.
//!
//! See <https://github.com/midnightntwrk/midnight-node/issues/1481>. If the D-parameter's
//! `num_permissioned_candidates` is less than the number of permissioned candidates registered on
//! Cardano then no candidate has a guaranteed committee seat, which risks liveness in a federated
//! network. The same check also runs at every session change via
//! [`midnight_node_runtime::log_if_d_param_below_permissioned_candidates`].

use authority_selection_inherents::{
	AuthoritySelectionDataSource, AuthoritySelectionInputs, CommitteeMember,
};
use midnight_node_runtime::{
	CrossChainPublic, Hash, log_if_d_param_below_permissioned_candidates,
	opaque::{Block, SessionKeys},
};
use pallet_system_parameters::SystemParametersApi;
use sc_service::SpawnTaskHandle;
use sidechain_domain::ScEpochNumber;
use sidechain_domain::mainchain_epoch::{
	MainchainEpochConfig, MainchainEpochDerivation, Timestamp,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_session_validator_management::SessionValidatorManagementApi;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Spawn a one-shot startup task that logs an error if `D < P` for the current Cardano epoch.
pub fn spawn_startup_check<C>(
	spawn_handle: &SpawnTaskHandle,
	client: Arc<C>,
	data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	epoch_config: MainchainEpochConfig,
) where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	C::Api: SystemParametersApi<Block, Hash>
		+ SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
{
	spawn_handle.spawn("d-param-startup-check", None, async move {
		if let Err(e) = run_startup_check(client, data_source, epoch_config).await {
			log::warn!(
				"Could not verify D-parameter against permissioned candidates on startup: {e}. \
				 The check will still run at the next session change."
			);
		}
	});
}

async fn run_startup_check<C>(
	client: Arc<C>,
	data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	epoch_config: MainchainEpochConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	C::Api: SystemParametersApi<Block, Hash>
		+ SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
{
	let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
	let mc_epoch =
		epoch_config.timestamp_to_mainchain_epoch(Timestamp::from_unix_millis(now_millis))?;

	let best_hash = client.info().best_hash;
	let api = client.runtime_api();
	let d_parameter = api.get_d_parameter(best_hash)?;
	let scripts = api.get_main_chain_scripts(best_hash)?;

	let ariadne = data_source
		.get_ariadne_parameters(
			mc_epoch,
			scripts.d_parameter_policy_id,
			scripts.permissioned_candidates_policy_id,
		)
		.await?;

	let permissioned = ariadne.permissioned_candidates.unwrap_or_default();
	log_if_d_param_below_permissioned_candidates(&d_parameter, &permissioned);
	Ok(())
}

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

use super::validation_utils::{maybe, path_exists};
use super::{CfgHelp, HelpField, cfg_help, error::CfgError, util::get_keys};
use documented::{Documented, DocumentedFields as _};
use serde::{Deserialize, Serialize};
use serde_valid::{Validate, validation};

#[derive(Debug, Serialize, Deserialize, Default, Validate, Documented)]
#[validate(custom = all_required)]
/// Parameters required for chainspec generation
pub struct ChainSpecCfg {
	/// Required for generic Live network chain spec
	/// Name of the network e.g. devnet1
	#[serde(default)]
	pub chainspec_name: Option<String>,
	/// Required for generic Live network chain spec
	/// Id of the network e.g. devnet
	#[serde(default)]
	pub chainspec_id: Option<String>,
	/// Required for generic Live network chain spec
	/// Path to genesis_state
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_genesis_state: Option<String>,
	/// Required for generic Live network chain spec
	/// Path to genesis_block
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_genesis_block: Option<String>,

	/// Required for generic Live network chain spec
	/// Chain type e.g. live
	#[serde(default)]
	pub chainspec_chain_type: Option<sc_service::ChainType>,
	/// Required for generic Live network chain spec
	/// Partner Chains Chain config file e.g. devnet/pc-chain-config.json
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_pc_chain_config: Option<String>,

	/// Required for generic Live network chain spec
	/// CNight Generates Dust config file e.g. devnet/cngd-config.json
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_cnight_genesis: Option<String>,

	/// Required for generic Live network chain spec
	/// Members of the Council Governance Authority
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_federated_authority_config: Option<String>,
}

fn all_required(cfg: &ChainSpecCfg) -> Result<(), validation::Error> {
	let mut missing: Vec<String> = Vec::new();

	if cfg.chainspec_name.is_some()
		|| cfg.chainspec_id.is_some()
		|| cfg.chainspec_chain_type.is_some()
		|| cfg.chainspec_pc_chain_config.is_some()
		|| cfg.chainspec_cnight_genesis.is_some()
		|| cfg.chainspec_federated_authority_config.is_some()
	{
		if cfg.chainspec_name.is_none() {
			missing.push("chainspec_name".to_string());
		}
		if cfg.chainspec_id.is_none() {
			missing.push("chainspec_id".to_string());
		}
		if cfg.chainspec_chain_type.is_none() {
			missing.push("chainspec_chain_type".to_string());
		}
		if cfg.chainspec_genesis_state.is_none() {
			missing.push("chainspec_genesis_state".to_string());
		}
		if cfg.chainspec_genesis_block.is_none() {
			missing.push("chainspec_genesis_block".to_string());
		}
		if cfg.chainspec_pc_chain_config.is_none() {
			missing.push("chainspec_pc_chain_config".to_string());
		}
		if cfg.chainspec_cnight_genesis.is_none() {
			missing.push("chainspec_cnight_genesis".to_string());
		}
		if cfg.chainspec_federated_authority_config.is_none() {
			missing.push("chainspec_federated_authority_config".to_string());
		}
	}

	if !missing.is_empty() {
		let msg = format!("missing the following env vars for chain-spec generation: {missing:?}");
		Err(validation::Error::Custom(msg))
	} else {
		Ok(())
	}
}

impl CfgHelp for ChainSpecCfg {
	fn help(cur_cfg: Option<&config::Config>) -> Result<Vec<HelpField>, CfgError> {
		cfg_help!(cur_cfg, Self)
	}
}

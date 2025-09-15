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

use super::{CfgHelp, HelpField, cfg_help, error::CfgError, util::get_keys};
use clap::Parser;
use documented::{Documented, DocumentedFields as _};
use sc_cli::RunCmd;
use sc_network::config::MultiaddrWithPeerId;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Clone, Debug, Default, Serialize, Deserialize, Documented, Validate)]
pub struct SubstrateCfg {
	/// REMOVED: USE "args" INSTEAD
	/// The arguments passed to the node, including the binary
	#[validate(
		max_length = 0,
		message = "ARGV is deprecated: use ARGS instead. Do not include the binary name when using ARGS"
	)]
	pub argv: Vec<String>,
	/// The arguments passed to the node
	pub args: Vec<String>,
	/// Extra arguments to append to args
	pub append_args: Vec<String>,
	/// Optional override for base_path. --base-path in argv takes precedence
	pub base_path: Option<String>,
	/// Path to a file containing the node key. Alternative to and takes precedence over --node-key
	pub node_key_file: Option<String>,
	/// Optional override for chain. --chain in argv takes precedence
	pub chain: Option<String>,
	/// Override for --validator in argv
	pub validator: bool,
	/// Appends to the list of bootnodes
	pub bootnodes: Vec<MultiaddrWithPeerId>,
	/// Override for --trie_cache_size. --trie-cache-size in argv takes precedence (unless set to default value).
	pub trie_cache_size: Option<usize>,
}

impl SubstrateCfg {
	pub fn argv(&self) -> Vec<String> {
		[&["midnight-node".to_string()], &self.args[..], &self.append_args[..]].concat()
	}
}

impl CfgHelp for SubstrateCfg {
	fn help(cur_cfg: Option<&config::Config>) -> Result<Vec<HelpField>, CfgError> {
		cfg_help!(cur_cfg, Self)
	}
}

impl TryFrom<SubstrateCfg> for RunCmd {
	type Error = sc_cli::Error;

	fn try_from(value: SubstrateCfg) -> Result<Self, Self::Error> {
		let default_run_cmd = RunCmd::parse_from(&["midnight-node".to_string()]);

		let mut run_cmd = RunCmd::parse_from(value.argv());
		if run_cmd.shared_params.base_path.is_none() && value.base_path.is_some() {
			run_cmd.shared_params.base_path = value.base_path.map(|p| p.into());
		}
		if run_cmd.network_params.node_key_params.node_key.is_some() {
			// NOTE: we can't use `log` here since it's not yet initialized in the main thread
			println!(
				"Warning: NODE_KEY passed as a CLI arg is not recommended. Use NODE_KEY_FILE env-var instead."
			);
		}
		if let Some(filepath) = value.node_key_file {
			let node_key =
				std::fs::read_to_string(&filepath).map(|s| s.trim().to_string()).map_err(|e| {
					sc_cli::Error::Input(format!(
						"error when reading node key file at {filepath}. Error: {e}"
					))
				})?;
			run_cmd.network_params.node_key_params.node_key = Some(node_key);
		}
		if run_cmd.shared_params.chain.is_none() && value.chain.is_some() {
			run_cmd.shared_params.chain = value.chain;
		}
		if !run_cmd.validator {
			run_cmd.validator = value.validator;
		}
		for bootnode in value.bootnodes {
			run_cmd.network_params.bootnodes.push(bootnode);
		}
		// This mostly guarantees --trie-cache-size in argv will take precendent
		// The exception to this is when the user sets --trie-cache-size to the default value
		if run_cmd.import_params.trie_cache_size == default_run_cmd.import_params.trie_cache_size
			&& let Some(trie_cache_size) = value.trie_cache_size
		{
			run_cmd.import_params.trie_cache_size = trie_cache_size;
		}
		Ok(run_cmd)
	}
}

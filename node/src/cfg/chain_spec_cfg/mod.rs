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
use midnight_serialize::NetworkId;
use serde::{Deserialize, Serialize};
use serde_valid::{Validate, validation};
use sidechain_domain::UtxoId;

fn network_id_deser<'de, D>(deserializer: D) -> Result<Option<NetworkId>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s: Option<String> = serde::Deserialize::deserialize(deserializer)?;
	match s {
		Some(s) => match s.as_str() {
			"undeployed" => Ok(Some(NetworkId::Undeployed)),
			"devnet" => Ok(Some(NetworkId::DevNet)),
			"testnet" => Ok(Some(NetworkId::TestNet)),
			"mainnet" => Ok(Some(NetworkId::MainNet)),
			_ => {
				Err(serde::de::Error::custom(format!("failed to deserialize {s:?} as network_id")))
			},
		},
		None => Ok(None),
	}
}

fn network_id_ser<S>(network_id: &Option<NetworkId>, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	match network_id {
		Some(network_id) => match network_id {
			NetworkId::Undeployed => serializer.serialize_str("undeployed"),
			NetworkId::DevNet => serializer.serialize_str("devnet"),
			NetworkId::TestNet => serializer.serialize_str("testnet"),
			NetworkId::MainNet => serializer.serialize_str("mainnet"),
			_ => serializer.serialize_str("unknown"),
		},
		None => serializer.serialize_str("none"),
	}
}

#[derive(Debug, Serialize, Deserialize, Default, Validate, Documented)]
#[validate(custom = all_required)]
/// Parameters required for chainspec generation
pub struct ChainSpecCfg {
	/// chain-spec generation: partner chain parameter for genesis_utxo
	#[serde(default)]
	pub genesis_utxo: Option<UtxoId>,
	/// This file is an output of the partner-chains-cli provided by partnerchains.
	/// It's required to provide configuration at runtime for the node.
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub addresses_json: Option<String>,
	/// Required for generic Live network chain spec
	/// Name of the network e.g. devnet1
	#[serde(default)]
	pub chainspec_name: Option<String>,
	/// Required for generic Live network chain spec
	/// Id of the network e.g. devnet
	#[serde(default)]
	pub chainspec_id: Option<String>,
	/// Required for generic Live network chain spec
	/// Id of the network e.g. devnet
	#[serde(deserialize_with = "network_id_deser")]
	#[serde(serialize_with = "network_id_ser")]
	#[serde(default)]
	pub chainspec_network_id: Option<NetworkId>,
	/// Required for generic Live network chain spec
	/// Path to genesis_state
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_genesis_state: Option<String>,
	/// Required for generic Live network chain spec
	/// Path to genesis_tx
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_genesis_tx: Option<String>,
	/// Required for generic Live network chain spec
	/// Chain type e.g. live
	#[serde(default)]
	pub chainspec_chain_type: Option<sc_service::ChainType>,
	/// Required for generic Live network chain spec
	/// Initial authorities file e.g. devnet/partner-chains-public-keys.json
	#[validate(custom = |s| maybe(s, path_exists))]
	#[serde(default)]
	pub chainspec_initial_authorities: Option<String>,
}

fn all_required(cfg: &ChainSpecCfg) -> Result<(), validation::Error> {
	let mut missing: Vec<String> = Vec::new();

	if cfg.genesis_utxo.is_some() && cfg.addresses_json.is_none() {
		missing.push("addresses_json".to_string());
	}

	if cfg.chainspec_name.is_some()
		|| cfg.chainspec_id.is_some()
		|| cfg.chainspec_network_id.is_some()
		|| cfg.chainspec_chain_type.is_some()
		|| cfg.chainspec_initial_authorities.is_some()
	{
		if cfg.chainspec_name.is_none() {
			missing.push("chainspec_name".to_string());
		}
		if cfg.chainspec_id.is_none() {
			missing.push("chainspec_id".to_string());
		}
		if cfg.chainspec_network_id.is_none() {
			missing.push("chainspec_network_id".to_string());
		}
		if cfg.chainspec_chain_type.is_none() {
			missing.push("chainspec_chain_type".to_string());
		}
		if cfg.chainspec_initial_authorities.is_none() {
			missing.push("chainspec_initial_authorities".to_string());
		}
		if cfg.addresses_json.is_none() {
			missing.push("addresses_json".to_string());
		}
		if cfg.genesis_utxo.is_none() {
			missing.push("genesis_utxo".to_string());
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

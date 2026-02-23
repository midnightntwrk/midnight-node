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

use midnight_primitives_federated_authority_observation::FederatedAuthorityObservationConfig;
use midnight_primitives_ics_observation::IcsConfig;
use midnight_primitives_reserve_observation::ReserveConfig;
use midnight_primitives_system_parameters::SystemParametersConfig;
use pallet_cnight_observation::config::CNightGenesis;
use {
	serde::{Deserialize, Deserializer, Serialize},
	sp_core::crypto::CryptoBytes,
	std::str::FromStr,
};

mod definitions;
pub use definitions::*;

fn from_hex<'de, D, T, const N: usize>(deserializer: D) -> Result<CryptoBytes<N, T>, D::Error>
where
	D: Deserializer<'de>,
{
	let s = <String as serde::Deserialize>::deserialize(deserializer)?;
	let bytes: Vec<u8> = sp_core::bytes::from_hex(&s).map_err(serde::de::Error::custom)?;
	let arr: [u8; N] = bytes.try_into().map_err(|v: Vec<u8>| {
		serde::de::Error::custom(format!("expected {N} bytes, got {}", v.len()))
	})?;
	Ok(CryptoBytes::from_raw(arr))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialAuthorityData {
	#[serde(rename = "aura_pub_key", deserialize_with = "from_hex")]
	pub aura_pubkey: sp_core::sr25519::Public,
	#[serde(rename = "grandpa_pub_key", deserialize_with = "from_hex")]
	pub grandpa_pubkey: sp_core::ed25519::Public,
	#[serde(rename = "sidechain_pub_key", deserialize_with = "from_hex")]
	pub crosschain_pubkey: sp_core::ecdsa::Public,
	#[serde(rename = "beefy_pub_key", deserialize_with = "from_hex")]
	pub beefy_pubkey: sp_core::ecdsa::Public,
}

impl InitialAuthorityData {
	pub fn new_from_uri(uri: &str) -> Self {
		use sp_core::Pair as _;
		let aura_pub_key = sp_core::sr25519::Pair::from_string(uri, None)
			.expect("failed to generate aura keypair from uri")
			.public();
		let grandpa_pub_key = sp_core::ed25519::Pair::from_string(uri, None)
			.expect("failed to generate grandpa keypair from uri")
			.public();
		let ecdsa_pub_key = sp_core::ecdsa::Pair::from_string(uri, None)
			.expect("failed to generate crosschain keypair from uri")
			.public();

		InitialAuthorityData {
			aura_pubkey: aura_pub_key,
			grandpa_pubkey: grandpa_pub_key,
			crosschain_pubkey: ecdsa_pub_key,
			beefy_pubkey: ecdsa_pub_key,
		}
	}

	pub fn load_initial_authorities(data: &str) -> Vec<Self> {
		serde_json::from_str(data).expect("failed to parse initial authorities")
	}

	/// Load initial authorities from permissioned-candidates-config.json
	pub fn load_from_permissioned_candidates_config(config: &serde_json::Value) -> Vec<Self> {
		let authorities_value = config
			.get("initial_permissioned_candidates")
			.expect("no \"initial_permissioned_candidates\" exists")
			.clone();
		serde_json::value::from_value(authorities_value)
			.expect("failed to parse \"initial_permissioned_candidates\"")
	}

	#[deprecated(note = "Use load_from_permissioned_candidates_config instead")]
	pub fn load_from_pc_chain_config(config: &serde_json::Value) -> Vec<Self> {
		Self::load_from_permissioned_candidates_config(config)
	}
}

pub struct EndowedAccount {
	pub pubkey: sp_core::sr25519::Public,
	pub balance: u128,
}

/// Configuration for mainchain scripts used in chain spec generation.
/// This struct is constructed from two separate config files:
/// - `registered-candidates-addresses.json` for `committee_candidates_address`
/// - `permissioned-candidates-config.json` for `permissioned_candidates_policy_id`
#[derive(Clone, Debug, Deserialize)]
pub struct MainChainScripts {
	pub committee_candidates_address: String,
	pub permissioned_candidates_policy_id: String,
}

/// Config loaded from `registered-candidates-addresses.json`
#[derive(Clone, Debug, Deserialize)]
pub struct RegisteredCandidatesAddresses {
	pub committee_candidates_address: String,
}

/// Config loaded from `permissioned-candidates-config.json` (or `permissioned-candidates-addresses.json`)
#[derive(Clone, Debug, Deserialize)]
pub struct PermissionedCandidatesConfig {
	pub permissioned_candidates_policy_id: String,
	#[serde(default)]
	pub initial_permissioned_candidates: Vec<InitialAuthorityData>,
}

impl From<MainChainScripts> for sp_session_validator_management::MainChainScripts {
	fn from(value: MainChainScripts) -> Self {
		let committee_candidate_address = FromStr::from_str(&value.committee_candidates_address)
			.expect("failed to convert committee_candidate_address");

		// TODO: The d_parameter_policy_id field should be removed from
		// sp_session_validator_management::MainChainScripts or made Optional in the future.
		// The DParameter is now read from pallet_system_parameters storage instead of from mainchain.
		let d_parameter_policy_id = sidechain_domain::PolicyId([0u8; 28]);

		let permissioned_candidates_policy_id =
			sidechain_domain::PolicyId::decode_hex(&value.permissioned_candidates_policy_id)
				.expect("failed to decode permissioned_candidates_policy_id as hex");

		Self {
			committee_candidate_address,
			d_parameter_policy_id,
			permissioned_candidates_policy_id,
		}
	}
}

impl MainChainScripts {
	/// Load MainChainScripts from separate config files
	pub fn load_from_configs(
		registered_candidates: &RegisteredCandidatesAddresses,
		permissioned_candidates: &PermissionedCandidatesConfig,
	) -> Self {
		Self {
			committee_candidates_address: registered_candidates
				.committee_candidates_address
				.clone(),
			permissioned_candidates_policy_id: permissioned_candidates
				.permissioned_candidates_policy_id
				.clone(),
		}
	}

	#[deprecated(note = "Use load_from_configs instead")]
	pub fn load_from_pc_chain_config(config: &serde_json::Value) -> Self {
		let value = config
			.get("cardano_addresses")
			.expect("no \"cardano_addresses\" exists")
			.clone();
		serde_json::value::from_value(value).expect("failed to parse \"cardano_addresses\"")
	}
}

pub trait MidnightNetwork {
	fn name(&self) -> &str;
	fn id(&self) -> &str;
	fn genesis_state(&self) -> &[u8];
	fn genesis_block(&self) -> &[u8];
	fn genesis_utxo(&self) -> &str;
	fn main_chain_scripts(&self) -> MainChainScripts;
	fn initial_authorities(&self) -> Vec<InitialAuthorityData>;
	fn federated_authority_config(&self) -> FederatedAuthorityObservationConfig;
	fn system_parameters_config(&self) -> SystemParametersConfig;
	fn cnight_genesis(&self) -> CNightGenesis;
	fn ics_config(&self) -> IcsConfig;
	fn reserve_config(&self) -> ReserveConfig;

	fn root_key(&self) -> Option<sp_core::sr25519::Public> {
		Some(self.initial_authorities()[0].aura_pubkey)
	}
	fn chain_type(&self) -> sc_service::ChainType {
		sc_service::ChainType::Live
	}

	fn network_id(&self) -> String {
		let network_id = if self.id() == "midnight" {
			"mainnet".to_string()
		} else {
			self.id().trim_start_matches("midnight_").to_string()
		};

		let spec = "arbitrary string consisting of alphanumeric characters and hyphens";
		if !network_id.chars().all(|c| c.is_alphanumeric() || c == '-') {
			panic!(
				"network_id does not meet spec. chain_id: {}, network_id: {}, spec: {spec}",
				self.id(),
				network_id
			);
		}

		network_id
	}
}

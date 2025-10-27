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

use pallet_cnight_observation::config::CNightGenesis;
use {
	serde::{Deserialize, Deserializer, Serialize, de::IntoDeserializer},
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
	let bytes: Vec<u8> = sp_core::bytes::from_hex(&s).expect("hex decode failed");
	let bytes = CryptoBytes::from_raw(bytes.try_into().expect("slice to array failed"));
	Ok(bytes)
}

fn from_hex_vec<'de, D>(deserializer: D) -> Result<Vec<sp_core::sr25519::Public>, D::Error>
where
	D: Deserializer<'de>,
{
	let strings: Vec<String> = Vec::deserialize(deserializer)?;
	strings
		.into_iter()
		.map(|s| {
			// Reuse `from_hex` via `serde::de::IntoDeserializer`
			from_hex::<_, sp_core::sr25519::Public, 32>(s.into_deserializer())
				.map(|crypto_bytes| sp_core::sr25519::Public::from_raw(crypto_bytes.0))
		})
		.collect()
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

	pub fn load_from_pc_chain_config(config: &serde_json::Value) -> Vec<Self> {
		let authorities_value = config
			.get("initial_permissioned_candidates")
			.expect("no \"initial_permissioned_candidates\" exists")
			.clone();
		serde_json::value::from_value(authorities_value)
			.expect("failed to parse \"initial_permissioned_candidates\"")
	}
}

pub struct EndowedAccount {
	pub pubkey: sp_core::sr25519::Public,
	pub balance: u128,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MainChainScripts {
	committee_candidates_address: String,
	d_parameter_policy_id: String,
	permissioned_candidates_policy_id: String,
	governed_map: Option<GovernedMapMainChainScripts>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GovernedMapMainChainScripts {
	validator_address: String,
	policy_id: String,
}

impl From<MainChainScripts> for sp_session_validator_management::MainChainScripts {
	fn from(value: MainChainScripts) -> Self {
		let committee_candidate_address = FromStr::from_str(&value.committee_candidates_address)
			.expect("failed to convert committee_candidate_address");

		let d_parameter_policy_id =
			sidechain_domain::PolicyId::decode_hex(&value.d_parameter_policy_id)
				.expect("failed to decode d_parameter_policy_id as hex");

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

impl From<MainChainScripts> for Option<sp_governed_map::MainChainScriptsV1> {
	fn from(value: MainChainScripts) -> Self {
		value.governed_map.map(|governed_map_mainchain_scripts| {
			let validator_address =
				FromStr::from_str(&governed_map_mainchain_scripts.validator_address)
					.expect("failed to decode governed_map.validator_address");

			let policy_id = FromStr::from_str(&governed_map_mainchain_scripts.policy_id)
				.expect("failed to convert governed_map.policy_id");

			sp_governed_map::MainChainScriptsV1 { validator_address, asset_policy_id: policy_id }
		})
	}
}

impl MainChainScripts {
	pub fn load_from_pc_chain_config(config: &serde_json::Value) -> Self {
		let value = config
			.get("cardano_addresses")
			.expect("no \"cardano_addresses\" exists")
			.clone();
		serde_json::value::from_value(value).expect("failed to parse \"cardano_addresses\"")
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialFederedatedAuthority {
	#[serde(deserialize_with = "from_hex_vec")]
	pub members: Vec<sp_core::sr25519::Public>,
}

impl InitialFederedatedAuthority {
	pub fn new_from_uris(uris: Vec<&str>) -> Self {
		use sp_core::Pair as _;

		let sr25519_pubkeys = uris
			.iter()
			.map(|uri| {
				sp_core::sr25519::Pair::from_string(uri, None)
					.expect("failed to generate sr25519 keypair from uri")
					.public()
			})
			.collect();

		InitialFederedatedAuthority { members: sr25519_pubkeys }
	}

	pub fn load_initial_federated_authority(data: &str) -> Vec<Self> {
		serde_json::from_str(data).expect("failed to parse initial federared authority")
	}

	pub fn load_from_federated_authority_config(
		config: &serde_json::Value,
		authority_body: &str,
	) -> Self {
		let authorities_value = config
			.get(authority_body)
			.expect(&format!("no \"{:?}\" exists", authority_body))
			.clone();
		serde_json::value::from_value(authorities_value)
			.expect(&format!("failed to parse \"{:?}\"", authority_body))
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
	fn cnight_genesis(&self) -> CNightGenesis;
	fn council(&self) -> InitialFederedatedAuthority;
	fn technical_committee(&self) -> InitialFederedatedAuthority;

	fn root_key(&self) -> Option<sp_core::sr25519::Public> {
		Some(self.initial_authorities()[0].aura_pubkey)
	}

	fn chain_type(&self) -> sc_service::ChainType {
		sc_service::ChainType::Live
	}
}

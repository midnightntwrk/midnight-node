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

use {
	midnight_serialize::NetworkId,
	serde::{Deserialize, Deserializer, Serialize},
	sp_core::crypto::CryptoBytes,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialAuthorityData {
	pub name: String,
	#[serde(rename = "aura_pub_key", deserialize_with = "from_hex")]
	pub aura_pubkey: sp_core::sr25519::Public,
	#[serde(rename = "grandpa_pub_key", deserialize_with = "from_hex")]
	pub grandpa_pubkey: sp_core::ed25519::Public,
	#[serde(rename = "crosschain_pub_key", deserialize_with = "from_hex")]
	pub crosschain_pubkey: sp_core::ecdsa::Public,
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
		let crosschain_pub_key = sp_core::ecdsa::Pair::from_string(uri, None)
			.expect("failed to generate crosschain keypair from uri")
			.public();

		InitialAuthorityData {
			name: uri.to_string(),
			aura_pubkey: aura_pub_key,
			grandpa_pubkey: grandpa_pub_key,
			crosschain_pubkey: crosschain_pub_key,
		}
	}
	pub fn load_initial_authorities(data: &str) -> Vec<Self> {
		serde_json::from_str(data).expect("failed to parse initial authorities")
	}
}

pub struct EndowedAccount {
	pub name: String,
	pub pubkey: sp_core::sr25519::Public,
	pub balance: u128,
}

pub trait MidnightNetwork {
	fn name(&self) -> &str;
	fn id(&self) -> &str;
	fn network_id(&self) -> NetworkId;
	fn genesis_state(&self) -> &[u8];
	fn genesis_tx(&self) -> &[u8];

	fn initial_authorities(&self) -> Vec<InitialAuthorityData>;

	fn endowed_accounts(&self) -> Vec<EndowedAccount> {
		self.initial_authorities()
			.iter()
			.map(|authority| EndowedAccount {
				name: authority.name.clone(),
				pubkey: authority.aura_pubkey,
				balance: 1 << 60,
			})
			.collect()
	}

	fn root_key(&self) -> Option<sp_core::sr25519::Public> {
		Some(self.initial_authorities()[0].aura_pubkey)
	}

	fn chain_type(&self) -> sc_service::ChainType {
		sc_service::ChainType::Live
	}
}

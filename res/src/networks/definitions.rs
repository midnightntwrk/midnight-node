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

use midnight_serialize::NetworkId;

use super::{InitialAuthorityData, MainChainScripts, MidnightNetwork};

pub struct UndeployedNetwork;
impl MidnightNetwork for UndeployedNetwork {
	fn name(&self) -> &str {
		"undeployed1"
	}

	fn id(&self) -> &str {
		"undeployed"
	}

	fn network_id(&self) -> NetworkId {
		NetworkId::Undeployed
	}

	fn genesis_state(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_state_undeployed.mn")
	}

	fn genesis_tx(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_tx_undeployed.mn")
	}

	#[cfg(feature = "chain-spec")]
	fn chain_type(&self) -> sc_service::ChainType {
		sc_service::ChainType::Local
	}

	#[cfg(feature = "chain-spec")]
	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		vec![InitialAuthorityData::new_from_uri("//Alice")]
	}

	fn genesis_utxo(&self) -> &str {
		"c684d0f7f5fb537d4996032a01a55511f3029cda9bcfc9a76b68e7b12d5a461a#6"
	}

	fn main_chain_scripts(&self) -> super::MainChainScripts {
		let pc_chain_config_str =
			String::from_utf8_lossy(include_bytes!("../../dev/pc-chain-config.json"));

		let pc_chain_config: serde_json::Value =
			serde_json::from_str(&pc_chain_config_str).unwrap();
		super::MainChainScripts::load_from_pc_chain_config(&pc_chain_config)
	}
}
/// Used when `--chain` is not specified when running `build-spec` - it will source chain values from
/// environment variables at runtime rather than hard-coded values at compile-time
pub struct CustomNetwork {
	pub name: String,
	pub id: String,
	pub network_id: NetworkId,
	pub genesis_state: Vec<u8>,
	pub genesis_tx: Vec<u8>,
	pub chain_type: sc_service::ChainType,
	pub initial_authorities: Vec<InitialAuthorityData>,
	pub main_chain_scripts: MainChainScripts,
	pub genesis_utxo: String,
}
impl MidnightNetwork for CustomNetwork {
	fn name(&self) -> &str {
		&self.name
	}

	fn id(&self) -> &str {
		&self.id
	}

	fn network_id(&self) -> NetworkId {
		self.network_id
	}

	fn genesis_state(&self) -> &[u8] {
		&self.genesis_state
	}

	fn genesis_tx(&self) -> &[u8] {
		&self.genesis_tx
	}

	fn chain_type(&self) -> sc_service::ChainType {
		self.chain_type.clone()
	}

	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		self.initial_authorities.clone()
	}

	fn main_chain_scripts(&self) -> MainChainScripts {
		self.main_chain_scripts.clone()
	}

	fn genesis_utxo(&self) -> &str {
		&self.genesis_utxo
	}
}

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

use midnight_primitives_federated_authority_observation::FederatedAuthorityObservationConfig;
use midnight_primitives_ics_observation::IcsConfig;
use midnight_primitives_reserve_observation::ReserveConfig;
use midnight_primitives_system_parameters::SystemParametersConfig;
use pallet_cnight_observation::config::CNightGenesis;

use super::{
	InitialAuthorityData, MainChainScripts, MidnightNetwork, PermissionedCandidatesConfig,
	RegisteredCandidatesAddresses,
};

pub struct UndeployedNetwork;
impl MidnightNetwork for UndeployedNetwork {
	fn name(&self) -> &str {
		"undeployed1"
	}

	fn id(&self) -> &str {
		"undeployed"
	}

	fn genesis_state(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_state_undeployed.mn")
	}

	fn genesis_block(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_block_undeployed.mn")
	}

	fn chain_type(&self) -> sc_service::ChainType {
		sc_service::ChainType::Local
	}

	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		vec![InitialAuthorityData::new_from_uri("//Alice")]
	}

	fn cnight_genesis(&self) -> CNightGenesis {
		let config_str = String::from_utf8_lossy(include_bytes!("../../dev/cnight-config.json"));
		serde_json::from_str(&config_str).unwrap()
	}

	fn federated_authority_config(&self) -> FederatedAuthorityObservationConfig {
		let config_str =
			String::from_utf8_lossy(include_bytes!("../../dev/federated-authority-config.json"));
		serde_json::from_str(&config_str).unwrap()
	}

	fn system_parameters_config(&self) -> SystemParametersConfig {
		let config_str =
			String::from_utf8_lossy(include_bytes!("../../dev/system-parameters-config.json"));
		serde_json::from_str(&config_str).unwrap()
	}

	fn ics_config(&self) -> IcsConfig {
		let config_str = String::from_utf8_lossy(include_bytes!("../../dev/ics-config.json"));
		serde_json::from_str(&config_str).unwrap()
	}

	fn reserve_config(&self) -> ReserveConfig {
		let config_str = String::from_utf8_lossy(include_bytes!("../../dev/reserve-config.json"));
		serde_json::from_str(&config_str).unwrap()
	}

	fn genesis_utxo(&self) -> &str {
		"c684d0f7f5fb537d4996032a01a55511f3029cda9bcfc9a76b68e7b12d5a461a#6"
	}

	fn main_chain_scripts(&self) -> super::MainChainScripts {
		let registered_candidates_str = String::from_utf8_lossy(include_bytes!(
			"../../dev/registered-candidates-addresses.json"
		));
		let registered_candidates: RegisteredCandidatesAddresses =
			serde_json::from_str(&registered_candidates_str).unwrap();

		let permissioned_candidates_str = String::from_utf8_lossy(include_bytes!(
			"../../dev/permissioned-candidates-config.json"
		));
		let permissioned_candidates: PermissionedCandidatesConfig =
			serde_json::from_str(&permissioned_candidates_str).unwrap();

		super::MainChainScripts::load_from_configs(&registered_candidates, &permissioned_candidates)
	}
}
/// Used when `--chain` is not specified when running `build-spec` - it will source chain values from
/// environment variables at runtime rather than hard-coded values at compile-time
pub struct CustomNetwork {
	pub name: String,
	pub id: String,
	pub genesis_state: Vec<u8>,
	pub genesis_block: Vec<u8>,
	pub chain_type: sc_service::ChainType,
	pub initial_authorities: Vec<InitialAuthorityData>,
	pub cnight_genesis: CNightGenesis,
	pub main_chain_scripts: MainChainScripts,
	pub genesis_utxo: String,
	pub federated_authority_config: FederatedAuthorityObservationConfig,
	pub system_parameters_config: SystemParametersConfig,
	pub ics_config: IcsConfig,
	pub reserve_config: ReserveConfig,
}
impl MidnightNetwork for CustomNetwork {
	fn name(&self) -> &str {
		&self.name
	}

	fn id(&self) -> &str {
		&self.id
	}

	fn genesis_state(&self) -> &[u8] {
		&self.genesis_state
	}

	fn genesis_block(&self) -> &[u8] {
		&self.genesis_block
	}

	fn chain_type(&self) -> sc_service::ChainType {
		self.chain_type.clone()
	}

	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		self.initial_authorities.clone()
	}

	fn cnight_genesis(&self) -> CNightGenesis {
		self.cnight_genesis.clone()
	}

	fn federated_authority_config(&self) -> FederatedAuthorityObservationConfig {
		self.federated_authority_config.clone()
	}

	fn system_parameters_config(&self) -> SystemParametersConfig {
		self.system_parameters_config.clone()
	}

	fn ics_config(&self) -> IcsConfig {
		self.ics_config.clone()
	}

	fn reserve_config(&self) -> ReserveConfig {
		self.reserve_config.clone()
	}

	fn main_chain_scripts(&self) -> MainChainScripts {
		self.main_chain_scripts.clone()
	}

	fn genesis_utxo(&self) -> &str {
		&self.genesis_utxo
	}
}

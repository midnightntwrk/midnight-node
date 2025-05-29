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

use super::{InitialAuthorityData, MidnightNetwork};

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
}

pub struct DevnetNetwork;
impl MidnightNetwork for DevnetNetwork {
	fn name(&self) -> &str {
		"devnet1"
	}

	fn id(&self) -> &str {
		"devnet"
	}

	fn network_id(&self) -> NetworkId {
		NetworkId::DevNet
	}

	fn genesis_state(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_state_devnet.mn")
	}

	fn genesis_tx(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_tx_devnet.mn")
	}

	#[cfg(feature = "chain-spec")]
	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		let data = include_str!("../../devnet/partner-chains-public-keys.json");
		InitialAuthorityData::load_initial_authorities(data)
	}
}

pub struct QanetNetwork;
impl MidnightNetwork for QanetNetwork {
	fn name(&self) -> &str {
		"qanet1"
	}

	fn id(&self) -> &str {
		"qanet"
	}

	fn network_id(&self) -> NetworkId {
		NetworkId::DevNet
	}

	fn genesis_state(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_state_qanet.mn")
	}

	fn genesis_tx(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_tx_qanet.mn")
	}

	#[cfg(feature = "chain-spec")]
	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		vec![
			InitialAuthorityData::new_from_uri("//Alice"),
			InitialAuthorityData::new_from_uri("//Bob"),
			InitialAuthorityData::new_from_uri("//Charlie"),
			InitialAuthorityData::new_from_uri("//Dave"),
			InitialAuthorityData::new_from_uri("//Eve"),
			InitialAuthorityData::new_from_uri("//Ferdie"),
			InitialAuthorityData::new_from_uri("//One"),
		]
	}
}

pub struct Testnet02Network;
impl MidnightNetwork for Testnet02Network {
	fn name(&self) -> &str {
		"testnet-02-1"
	}

	fn id(&self) -> &str {
		"testnet-02"
	}

	fn network_id(&self) -> NetworkId {
		NetworkId::TestNet
	}

	fn genesis_state(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_state_testnet-02.mn")
	}

	fn genesis_tx(&self) -> &[u8] {
		include_bytes!("../../genesis/genesis_tx_testnet-02.mn")
	}

	#[cfg(feature = "chain-spec")]
	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		let data = include_str!("../../testnet-02/partner-chains-public-keys.json");
		InitialAuthorityData::load_initial_authorities(data)
	}
}

pub struct MainnetNetwork;
impl MidnightNetwork for MainnetNetwork {
	fn name(&self) -> &str {
		"mainnet1"
	}

	fn id(&self) -> &str {
		"mainnet"
	}

	fn network_id(&self) -> NetworkId {
		NetworkId::MainNet
	}

	fn genesis_state(&self) -> &[u8] {
		todo!()
	}

	fn genesis_tx(&self) -> &[u8] {
		todo!()
	}

	#[cfg(feature = "chain-spec")]
	fn initial_authorities(&self) -> Vec<InitialAuthorityData> {
		todo!()
	}
}

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

use midnight_serialize::{Deserializable, NetworkId, Serializable};

#[cfg(not(target_family = "wasm"))]
pub mod subxt_metadata;

#[cfg(feature = "chain-spec")]
pub mod networks;

pub mod env {
	pub const DEFAULT: &str = include_str!("../cfg/default.toml");
	// TODO: Change dev -> local
	pub const DEV: &str = include_str!("../cfg/dev.toml");
	pub const QANET: &str = include_str!("../cfg/qanet.toml");
	pub const DEVNET: &str = include_str!("../cfg/devnet.toml");
	pub const TESTNET_02: &str = include_str!("../cfg/testnet-02.toml");
}

// Well-known values for live Cardano testnet preview resources.
pub mod native_token_observation_consts {
	// Live registration/mapping-validator contract
	pub const TEST_CNIGHT_REGISTRATIONS_ADDRESS: &str =
		"addr_test1wral0lzw5kpjytmw0gmsdcgctx09au24nt85zma38py8g3crwvpwe";
	// Known native asset policy id for test cNIGHT
	pub const TEST_CNIGHT_CURRENCY_POLICY_ID: [u8; 28] =
		hex_literal::hex!("03cf16101d110dcad9cacb225f0d1e63a8809979e7feb60426995414");

	// Known asset name for test cNIGHT
	pub const TEST_CNIGHT_ASSET_NAME: &str = "";
}

/// Serializes a mn_ledger::serialize-able type into bytes
pub fn serialize_mn<T: Serializable>(
	value: &T,
	network_id: NetworkId,
) -> Result<Vec<u8>, std::io::Error> {
	let size = Serializable::serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	midnight_serialize::serialize(value, &mut bytes, network_id)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize_mn<T: Deserializable, H: std::io::Read>(
	bytes: H,
	network_id: NetworkId,
) -> Result<T, std::io::Error> {
	let val: T = midnight_serialize::deserialize(bytes, network_id)?;
	Ok(val)
}

pub mod undeployed {
	pub mod transactions {
		#[cfg(any(feature = "test", feature = "runtime-benchmarks"))]
		pub const CONTRACT_ADDR: &[u8] =
			include_bytes!("../test-contract/contract_address_undeployed.mn");
		#[cfg(feature = "test")]
		pub const DEPLOY_TX: &[u8] =
			include_bytes!("../test-contract/contract_tx_1_deploy_undeployed.mn");
		#[cfg(feature = "test")]
		pub const STORE_TX: &[u8] =
			include_bytes!("../test-contract/contract_tx_2_store_undeployed.mn");
		#[cfg(feature = "test")]
		pub const CHECK_TX: &[u8] =
			include_bytes!("../test-contract/contract_tx_3_check_undeployed.mn");
		#[cfg(feature = "test")]
		pub const MAINTENANCE_TX: &[u8] =
			include_bytes!("../test-contract/contract_tx_4_change_authority_undeployed.mn");
		#[cfg(feature = "test")]
		pub const ZSWAP_TX: &[u8] = include_bytes!("../test-zswap/zswap_undeployed.mn");
		#[cfg(feature = "test")]
		pub const CLAIM_MINT_TX: &[u8] =
			include_bytes!("../test-claim-mint/claim_mint_undeployed.mn");
	}
}

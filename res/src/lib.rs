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

use std::{
	path::{Path, PathBuf},
	sync::{LazyLock, Mutex},
};

use midnight_serialize::{Deserializable, Serializable, Tagged};

#[cfg(feature = "chain-spec")]
pub mod networks;

pub static CFG_ROOT: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
pub const CFG_PATH: &str = "res/cfg/";
fn config_path() -> PathBuf {
	let root = CFG_ROOT.lock().unwrap();
	if let Some(ref root) = *root {
		Path::new(root).join(CFG_PATH)
	} else {
		std::env::current_dir().unwrap().join(CFG_PATH)
	}
}

pub fn default_cfg() -> String {
	let path = config_path().join("default.toml");

	std::fs::read_to_string(&path)
		.unwrap_or_else(|e| panic!("failed reading default.toml at path {}: {e}", path.display()))
}

pub fn list_configs() -> Vec<String> {
	let paths = std::fs::read_dir(&config_path()).unwrap();
	paths
		.filter_map(|entry| {
			entry.ok().and_then(|e| {
				let p = e.path();
				if p.extension().map_or(false, |ext| ext == "toml") {
					let stem = p.file_stem().map(|s| s.to_string_lossy().to_string());
					if stem.as_ref().map_or(false, |s| s != "default") { stem } else { None }
				} else {
					None
				}
			})
		})
		.collect()
}

pub fn get_config(name: &str) -> Option<String> {
	let mut paths = std::fs::read_dir(&config_path()).unwrap();
	let config_path = paths.find_map(|entry| {
		entry.ok().and_then(|e| {
			let p = e.path();
			if p.extension().map_or(false, |ext| ext == "toml")
				&& p.file_stem().map_or(false, |s| s == name)
			{
				Some(e.path())
			} else {
				None
			}
		})
	});

	config_path.map_or(None, |path| std::fs::read_to_string(path).ok())
}

// Well-known values for live Cardano testnet preview resources.
pub mod cnight_observation_consts {
	// Redemption Validator address
	pub const TEST_CNIGHT_REDEMPTION_VALIDATOR_ADDRESS: &str =
		"addr_test1wz3t0v4r0kwdfnh44m87z4rasp4nj0rcplfpmwxvhhrzhdgl45vx4";
	// Mapping Validator address
	pub const TEST_CNIGHT_MAPPING_VALIDATOR_ADDRESS: &str =
		"addr_test1wral0lzw5kpjytmw0gmsdcgctx09au24nt85zma38py8g3crwvpwe";
	// Known native asset policy id for test cNIGHT
	pub const TEST_CNIGHT_CURRENCY_POLICY_ID: [u8; 28] =
		hex_literal::hex!("03cf16101d110dcad9cacb225f0d1e63a8809979e7feb60426995414");

	// Known asset name for test cNIGHT
	pub const TEST_CNIGHT_ASSET_NAME: &str = "";
}

/// Serializes a mn_ledger::serialize-able type into bytes
pub fn serialize_mn<T: Serializable + Tagged>(value: &T) -> Result<Vec<u8>, std::io::Error> {
	let size = Serializable::serialized_size(value);
	let mut bytes = Vec::with_capacity(size);
	midnight_serialize::tagged_serialize(value, &mut bytes)?;
	Ok(bytes)
}

/// Deserializes a mn_ledger::serialize-able type from bytes
pub fn deserialize_mn<T: Deserializable + Tagged, H: std::io::Read>(
	bytes: H,
) -> Result<T, std::io::Error> {
	let val: T = midnight_serialize::tagged_deserialize(bytes)?;
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

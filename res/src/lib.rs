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

use std::{
	path::{Path, PathBuf},
	sync::{LazyLock, Mutex},
};

use midnight_serialize::{Deserializable, Serializable, Tagged};

#[cfg(feature = "chain-spec")]
pub mod networks;

pub static CFG_ROOT: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
pub const CFG_PATH: &str = "res/cfg/";

/// Locate the workspace root by walking up from the current dir until
/// `res/cfg/default.toml` is found. The working dir differs between build tools
/// (`cargo` runs a crate's tests from the crate dir, `buck2` from the workspace
/// root), so tests anchor `CFG_ROOT` to this instead of a fixed `"../"`.
///
/// # Panics
/// If no `res/cfg/default.toml` exists in the current dir or any ancestor.
pub fn locate_workspace_root() -> PathBuf {
	// buck2 runs tests from the project root in a hermetic sandbox that lacks the
	// res/ tree, and the upward walk can only find fixtures at or above cwd. Let a
	// test target instead point at a staged fixtures root via MN_WORKSPACE_ROOT
	// (unset under cargo, so the walk is the default).
	if let Some(root) = std::env::var_os("MN_WORKSPACE_ROOT") {
		return PathBuf::from(root);
	}
	let mut dir = std::env::current_dir().expect("cwd");
	loop {
		if dir.join(CFG_PATH).join("default.toml").exists() {
			return dir;
		}
		if !dir.pop() {
			panic!("could not locate workspace root ({CFG_PATH}default.toml not found above cwd)");
		}
	}
}
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
		"addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng";
	// Known native asset policy id for test cNIGHT
	pub const TEST_CNIGHT_CURRENCY_POLICY_ID: [u8; 28] =
		hex_literal::hex!("d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1");

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

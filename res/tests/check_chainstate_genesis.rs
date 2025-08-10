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

fn decode_genesis_tx(genesis_tx: &str) -> Vec<u8> {
	hex::decode(hex::decode(&genesis_tx[22..]).expect("failed to decode genesis tx"))
		.expect("failed to decode genesis tx")
}
fn load_genesis_tx_file(genesis_tx_path: &std::path::PathBuf) -> Vec<u8> {
	std::fs::read(genesis_tx_path).unwrap_or_else(|_| {
		panic!("failed to load genesis tx at path {}", genesis_tx_path.display())
	})
}

#[test]
fn check_all_chainspec_integrity() {
	*midnight_node_res::CFG_ROOT.lock().unwrap() = Some("../".to_string());
	for name in midnight_node_res::list_configs() {
		let config_str = midnight_node_res::get_config(&name)
			.unwrap_or_else(|| panic!("get_config error ({name})"));
		let config = config_str
			.parse::<toml::Table>()
			.unwrap_or_else(|_| panic!("failed to parse config as toml ({name})"));
		let chainspec_path = config.get("chain");
		if chainspec_path.is_none() {
			continue;
		}

		let chain_spec: serde_json::Value = serde_json::from_str(
			&std::fs::read_to_string(std::path::Path::new("../").join(
				chainspec_path.unwrap().as_str().unwrap_or_else(|| panic!("'chain' not string")),
			))
			.unwrap(),
		)
		.unwrap();

		let chainspec_genesis_tx = decode_genesis_tx(
			chain_spec
				.pointer("/properties/genesis_tx")
				.unwrap_or_else(|| panic!("genesis_tx not found in chain spec ({name})"))
				.as_str()
				.unwrap_or_else(|| panic!("genesis_tx not a string ({name})")),
		);
		let genesis_tx = load_genesis_tx_file(
			&std::path::Path::new("../").join(
				config
					.get("chainspec_genesis_tx")
					.unwrap_or_else(|| panic!("failed to find chainspec_genesis_tx ({name})"))
					.as_str()
					.unwrap(),
			),
		);

		// We compare them directly, instead of using assert_eq, because otherwise
		// assert_eq will crash on trying to generate the diff strings
		assert!(chainspec_genesis_tx == genesis_tx, "genesis tx mismatch for config {name}");
	}
}

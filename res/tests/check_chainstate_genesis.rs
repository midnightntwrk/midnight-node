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

fn load_genesis_state_file(genesis_state_path: &std::path::PathBuf) -> Vec<u8> {
	std::fs::read(genesis_state_path).unwrap_or_else(|_| {
		panic!("failed to load genesis state at path {}", genesis_state_path.display())
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

		let chainspec_genesis_state = chain_spec
			.pointer("/properties/genesis_state")
			.unwrap_or_else(|| panic!("genesis_state not found in chain spec ({name})"))
			.as_str()
			.unwrap_or_else(|| panic!("genesis_state not a string ({name})"));

		let genesis_state = load_genesis_state_file(
			&std::path::Path::new("../").join(
				config
					.get("chainspec_genesis_state")
					.unwrap_or_else(|| panic!("failed to find chainspec_genesis_state ({name})"))
					.as_str()
					.unwrap(),
			),
		);
		let hexed_genesis_state = hex::encode(&genesis_state);

		// We compare them directly, instead of using assert_eq, because otherwise
		// assert_eq will crash on trying to generate the diff strings
		assert!(
			chainspec_genesis_state.contains(&hexed_genesis_state),
			"genesis state mismatch for config {name}"
		);
	}
}

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

//! Offline session-key generation from the runtime wasm blob, as used by the
//! `wizards rotate-keys --runtime-wasm` flow. Exercises the real midnight runtime:
//! the wasm is executed with the node keystore exposed through host functions, so
//! this also verifies end-to-end that `generate_session_keys` creates the cross-chain
//! identity key only once while consensus keys rotate.

use partner_chains_cli::runtime_wasm::generate_session_keys_from_wasm;

const AURA_PREFIX: &str = "61757261";
const GRANDPA_PREFIX: &str = "6772616e";
const CROSS_CHAIN_PREFIX: &str = "63726368";

fn count_keys_with_prefix(keystore_path: &std::path::Path, prefix: &str) -> usize {
	std::fs::read_dir(keystore_path)
		.expect("keystore directory exists")
		.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
		.filter(|name| name.starts_with(prefix))
		.count()
}

#[test]
fn generates_and_rotates_session_keys_from_runtime_wasm() {
	let wasm = midnight_node_runtime::WASM_BINARY.expect("runtime wasm binary is built");
	let tmp = tempfile::tempdir().expect("temp dir is created");
	let wasm_path = tmp.path().join("runtime.wasm");
	std::fs::write(&wasm_path, wasm).expect("wasm file is written");
	let keystore_path = tmp.path().join("keystore");

	let first = generate_session_keys_from_wasm(
		wasm_path.to_str().unwrap(),
		keystore_path.to_str().unwrap(),
	)
	.expect("offline session key generation succeeds");

	assert_eq!(first.code_hash, sp_crypto_hashing::blake2_256(wasm));
	let ids: Vec<[u8; 4]> = first.keys.iter().map(|(id, _)| id.0).collect();
	assert_eq!(ids, vec![*b"aura", *b"gran"], "runtime declares the session key set");
	assert!(!first.opaque.is_empty());

	assert_eq!(count_keys_with_prefix(&keystore_path, AURA_PREFIX), 1);
	assert_eq!(count_keys_with_prefix(&keystore_path, GRANDPA_PREFIX), 1);
	assert_eq!(count_keys_with_prefix(&keystore_path, CROSS_CHAIN_PREFIX), 1);

	let second = generate_session_keys_from_wasm(
		wasm_path.to_str().unwrap(),
		keystore_path.to_str().unwrap(),
	)
	.expect("repeated generation succeeds");
	assert_ne!(first.opaque, second.opaque, "consensus session keys rotate");

	// consensus keys accumulate, while the cross-chain identity key is generated only once
	assert_eq!(count_keys_with_prefix(&keystore_path, AURA_PREFIX), 2);
	assert_eq!(count_keys_with_prefix(&keystore_path, GRANDPA_PREFIX), 2);
	assert_eq!(count_keys_with_prefix(&keystore_path, CROSS_CHAIN_PREFIX), 1);
}

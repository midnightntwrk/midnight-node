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

//! `midnight-node generate-session-keys`: generates a fresh session key set into the
//! keystore of the node on this machine and prints its public keys for the Cardano
//! registration.

#![allow(clippy::result_large_err)]

use clap::Parser;
use sc_cli::KeystoreParams;
use sc_service::config::KeystoreConfig;
use sp_core::{crypto::KeyTypeId, storage::well_known_keys};
use std::path::PathBuf;

use super::generate::{GeneratedKeys, NodeKeystore};

#[derive(Debug, Clone, Parser)]
#[command(mut_arg("keystore_path", |arg| arg.required(true)))]
pub struct GenerateSessionKeysCmd {
	/// Runtime wasm, compressed or not, that defines the session keys. Defaults to the
	/// chain's current runtime, read from the node's RPC on 127.0.0.1.
	#[arg(long)]
	pub runtime_wasm: Option<PathBuf>,

	/// RPC port of the node on this machine. Required without `--runtime-wasm`, rejected with it.
	#[arg(long, required_unless_present = "runtime_wasm", conflicts_with = "runtime_wasm")]
	pub rpc_port: Option<u16>,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub keystore_params: KeystoreParams,
}

impl GenerateSessionKeysCmd {
	pub fn run(&self) -> sc_cli::Result<()> {
		let KeystoreConfig::Path { path, password } =
			self.keystore_params.keystore_config(std::path::Path::new(""))?
		else {
			return Err(sc_cli::Error::Input("in-memory keystore".into()));
		};
		let keystore = NodeKeystore::open(&path, password).map_err(|e| e.to_string())?;
		eprintln!(
			"Keystore: {}\nCross-chain key: 0x{}",
			path.canonicalize().unwrap_or_else(|_| path.clone()).display(),
			hex::encode(keystore.cross_chain())
		);
		let code = match &self.runtime_wasm {
			Some(path) => {
				std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?
			},
			None => current_code(self.rpc_port.ok_or("--rpc-port is required")?)?,
		};
		let generated = keystore.generate(&code).map_err(|e| e.to_string())?;
		println!("{}", output(&generated));
		eprintln!(
			"Keep the old session keys in the keystore until the new registration is active \
			 (two Cardano epochs)."
		);
		Ok(())
	}
}

/// The chain's current runtime code, read through the RPC of the node on this machine.
fn current_code(rpc_port: u16) -> Result<Vec<u8>, String> {
	// Loopback only: the node's RPC must not be reached over the network.
	let rpc_url = format!("http://127.0.0.1:{rpc_port}");
	get_storage(&rpc_url, well_known_keys::CODE)?
		.ok_or_else(|| "the node returned no runtime code".into())
}

/// `state_getStorage` at the best block.
fn get_storage(rpc_url: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
	let request = serde_json::json!({
		"jsonrpc": "2.0",
		"id": 1,
		"method": "state_getStorage",
		"params": [format!("0x{}", hex::encode(key))],
	});
	// A proxy or a redirect would let another host serve the code that runs with the keystore.
	let client = reqwest::blocking::Client::builder()
		.no_proxy()
		.redirect(reqwest::redirect::Policy::none())
		.build()
		.map_err(|e| format!("RPC client: {e}"))?;
	let response: serde_json::Value = client
		.post(rpc_url)
		.json(&request)
		.send()
		.and_then(|response| response.json())
		.map_err(|e| format!("state_getStorage via {rpc_url}: {e}"))?;
	if let Some(error) = response.get("error") {
		return Err(format!("state_getStorage via {rpc_url}: {error}"));
	}
	response["result"]
		.as_str()
		.map(|value| hex::decode(value.trim_start_matches("0x")))
		.transpose()
		.map_err(|e| format!("state_getStorage via {rpc_url}: {e}"))
}

fn output(generated: &GeneratedKeys) -> serde_json::Value {
	let hex = |bytes: &[u8]| format!("0x{}", hex::encode(bytes));
	// In the runtime's key order, the order the signatures join into an ownership proof.
	let by_id = |entries: &[(KeyTypeId, Vec<u8>)]| -> serde_json::Map<String, serde_json::Value> {
		entries
			.iter()
			.map(|(key_type, bytes)| {
				(String::from_utf8_lossy(&key_type.0).into_owned(), hex(bytes).into())
			})
			.collect()
	};
	serde_json::json!({
		"sidechain_pub_key": hex(generated.cross_chain.as_ref()),
		"keys": by_id(&generated.keys),
		"signatures": by_id(&generated.signatures),
		"runtime_code_hash": hex(&generated.code_hash),
	})
}

#[cfg(test)]
mod tests {
	use super::{super::generate::generate, *};
	use jsonrpsee::{RpcModule, server::Server};
	use midnight_node_runtime::WASM_BINARY;
	use sc_keystore::LocalKeystore;
	use sidechain_domain::CROSS_CHAIN_KEY_TYPE_ID;
	use sp_keystore::Keystore;
	use sp_runtime::traits::OpaqueKeys;
	use tempfile::TempDir;

	fn wasm() -> &'static [u8] {
		WASM_BINARY.expect("tests need the runtime wasm; build without SKIP_WASM_BUILD")
	}

	/// Serves `state_getStorage` from `storage` on a loopback port.
	async fn mock_node(storage: Vec<(Vec<u8>, Vec<u8>)>) -> (u16, jsonrpsee::server::ServerHandle) {
		let mut module = RpcModule::new(storage);
		module
			.register_method("state_getStorage", |params, storage, _| {
				let key: String = params.one().unwrap();
				let key = hex::decode(key.trim_start_matches("0x")).unwrap();
				storage
					.iter()
					.find(|(k, _)| *k == key)
					.map(|(_, value)| format!("0x{}", hex::encode(value)))
			})
			.unwrap();
		// Debug builds of the runtime are uncompressed and exceed the default response limit.
		let server = Server::builder()
			.max_response_body_size(u32::MAX)
			.build("127.0.0.1:0")
			.await
			.unwrap();
		let port = server.local_addr().unwrap().port();
		(port, server.start(module))
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn defaults_to_the_code_the_node_serves() {
		let (port, _server) =
			mock_node(vec![(well_known_keys::CODE.to_vec(), wasm().to_vec())]).await;

		let code = tokio::task::spawn_blocking(move || current_code(port)).await.unwrap().unwrap();

		assert_eq!(code, wasm());
	}

	#[test]
	fn requires_an_explicit_keystore_path() {
		let err = GenerateSessionKeysCmd::try_parse_from([
			"generate-session-keys",
			"--runtime-wasm",
			"runtime.wasm",
		])
		.unwrap_err();

		assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
		assert!(err.to_string().contains("--keystore-path"), "{err}");
	}

	#[test]
	fn requires_the_rpc_port_only_without_a_runtime_wasm() {
		let parse = |args: &[&str]| {
			GenerateSessionKeysCmd::try_parse_from(
				[&["generate-session-keys", "--keystore-path", "keystore"], args].concat(),
			)
		};

		assert!(parse(&[]).is_err());
		assert_eq!(parse(&["--rpc-port", "9933"]).unwrap().rpc_port, Some(9933));
		assert_eq!(parse(&["--runtime-wasm", "runtime.wasm"]).unwrap().rpc_port, None);
		assert!(parse(&["--runtime-wasm", "runtime.wasm", "--rpc-port", "9933"]).is_err());
	}

	#[test]
	fn output_lists_the_generated_keys() {
		let dir = TempDir::new().unwrap();
		let crch = LocalKeystore::open(dir.path(), None)
			.unwrap()
			.ecdsa_generate_new(CROSS_CHAIN_KEY_TYPE_ID, None)
			.unwrap();
		let generated = generate(dir.path(), None, wasm()).unwrap();

		let output = output(&generated);

		let hex = |bytes: &[u8]| format!("0x{}", hex::encode(bytes));
		let by_id = |entries: &[(KeyTypeId, Vec<u8>)]| -> serde_json::Map<_, _> {
			entries
				.iter()
				.map(|(key_type, bytes)| {
					(String::from_utf8(key_type.0.to_vec()).unwrap(), hex(bytes).into())
				})
				.collect()
		};
		assert_eq!(
			output,
			serde_json::json!({
				"sidechain_pub_key": hex(&crch.0),
				"keys": by_id(&generated.keys),
				"signatures": by_id(&generated.signatures),
				"runtime_code_hash": hex(&sp_crypto_hashing::blake2_256(wasm())),
			})
		);
		for field in ["keys", "signatures"] {
			let names: Vec<_> = output[field].as_object().unwrap().keys().cloned().collect();
			let key_ids: Vec<_> = midnight_node_runtime::opaque::SessionKeys::key_ids()
				.iter()
				.map(|key_type| String::from_utf8(key_type.0.to_vec()).unwrap())
				.collect();
			assert_eq!(names, key_ids);
		}
	}
}

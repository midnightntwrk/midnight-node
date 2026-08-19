// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod common;

use clap::Parser;
use common::{test_image, wait_for_node::wait_for_finalized_block};
use midnight_node_toolkit::cli::{Cli, run_command};
use std::{process::Command, time::Duration};
use subxt::rpcs::{RpcClient, rpc_params};
use testcontainers::{
	GenericImage, ImageExt,
	core::{ContainerPort, WaitFor},
	runners::AsyncRunner,
};

/// Generate a chain-spec JSON string by running `build-spec` in the fork-from node container.
fn generate_chainspec(image: &str, tag: &str) -> String {
	let output = Command::new("docker")
		.args(["run", "--rm", "-e", "CFG_PRESET=dev", &format!("{image}:{tag}"), "build-spec"])
		.output()
		.expect("docker run build-spec failed");
	assert!(
		output.status.success(),
		"build-spec failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).expect("invalid utf8 chain-spec")
}

/// Run a toolkit CLI command.
async fn run_cli(args: &[&str]) {
	let full_args: Vec<&str> =
		std::iter::once("midnight-node-toolkit").chain(args.iter().copied()).collect();
	eprintln!("[hardfork_e2e] running CLI: {full_args:?}");
	let cli = Cli::parse_from(full_args);
	if let Err(e) = run_command(cli.command).await {
		eprintln!("[hardfork_e2e] CLI command failed: {e}");
		eprintln!("[hardfork_e2e] error debug: {e:?}");
		panic!("CLI command failed: {e}");
	}
	eprintln!("[hardfork_e2e] CLI command succeeded");
}

/// Hash of the block at `height`, hex-encoded.
async fn block_hash_at(rpc: &RpcClient, height: u64) -> String {
	let hash: serde_json::Value = rpc
		.request("chain_getBlockHash", rpc_params![height])
		.await
		.unwrap_or_else(|e| panic!("chain_getBlockHash({height}) failed: {e}"));
	hash.as_str()
		.unwrap_or_else(|| panic!("no block at height {height}"))
		.to_owned()
}

/// The runtime `specVersion` *stored at* `hash`.
///
/// Raw `state_getRuntimeVersion` rather than subxt's typed metadata on purpose:
/// across a runtime upgrade the client's metadata follows the new runtime, so
/// anything the old runtime encoded decodes unreliably. See
/// `midnight_node_toolkit::commands::runtime_upgrade`.
async fn spec_version_at(rpc: &RpcClient, hash: &str) -> u64 {
	let version: serde_json::Value = rpc
		.request("state_getRuntimeVersion", rpc_params![hash])
		.await
		.unwrap_or_else(|e| panic!("state_getRuntimeVersion({hash}) failed: {e}"));
	version
		.get("specVersion")
		.and_then(|v| v.as_u64())
		.unwrap_or_else(|| panic!("no specVersion in runtime version at {hash}"))
}

async fn finalized_height(rpc: &RpcClient) -> u64 {
	let hash: serde_json::Value = rpc
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.expect("chain_getFinalizedHead failed");
	let header: serde_json::Value = rpc
		.request("chain_getHeader", rpc_params![hash])
		.await
		.expect("chain_getHeader failed");
	header
		.get("number")
		.and_then(|n| n.as_str())
		.and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
		.expect("no number in finalized header")
}

/// Locate the block that applied the new runtime code — the one whose committed
/// state pairs the *new* `:code` with the *old* ledger version's `StateKey`.
///
/// `frame_system` overwrites `:code` inside that block (the pre-fork runtime ships
/// `system_version: 1`, so the code is not staged in `:pending_code`), while
/// pallet-midnight's v8->v9 state translation only runs in the next block's
/// `initialize_block`. Executing a read at that hash therefore runs ledger-9 WASM
/// against a ledger-8 arena root, which is what GH #1959 reports.
///
/// `state_getRuntimeVersion` reports the code stored at a block, so the first
/// height reporting the new spec is exactly that block. spec_version is monotonic
/// along the chain, so binary-search for it.
async fn find_code_applied_block(rpc: &RpcClient, head: u64, old_spec: u64) -> u64 {
	let (mut lo, mut hi) = (1u64, head);
	while lo < hi {
		let mid = lo + (hi - lo) / 2;
		let hash = block_hash_at(rpc, mid).await;
		if spec_version_at(rpc, &hash).await > old_spec {
			hi = mid;
		} else {
			lo = mid + 1;
		}
	}
	lo
}

/// Every way of reading the ledger state must answer at `height`.
///
/// Both the `midnight_*` RPCs and a raw `state_call`: the fix lives in the ledger-9
/// host function, so the runtime API has to work at the skew block too — that is
/// the path subxt-based tooling (`chain-indexer`, GH #1969) takes, and it does not
/// go anywhere near the node's own RPC layer.
async fn assert_ledger_state_readable(rpc: &RpcClient, height: u64, label: &str) {
	let hash = block_hash_at(rpc, height).await;

	for method in ["midnight_zswapStateRoot", "midnight_ledgerStateRoot"] {
		let root: Vec<u8> = rpc
			.request(method, rpc_params![&hash])
			.await
			.unwrap_or_else(|e| panic!("{method} failed at {label} (#{height}, {hash}): {e}"));
		assert!(!root.is_empty(), "{method} returned an empty root at {label} (#{height})");
	}

	// `Result<Vec<u8>, LedgerApiError>` SCALE-encoded: a leading 0x00 is `Ok`, and
	// anything else is the pallet reporting a ledger error (0x01 plus the variant).
	for api in
		["MidnightRuntimeApi_get_ledger_state_root", "MidnightRuntimeApi_get_ledger_parameters"]
	{
		let encoded: String = rpc
			.request("state_call", rpc_params![api, "0x", &hash])
			.await
			.unwrap_or_else(|e| panic!("{api} failed at {label} (#{height}, {hash}): {e}"));
		assert!(
			encoded.starts_with("0x00"),
			"{api} returned an error at {label} (#{height}): {encoded}"
		);
	}

	eprintln!("[hardfork_e2e] ledger state readable at {label} (#{height})");
}

#[test_log::test(tokio::test)]
async fn hardfork_single_tx() {
	// 1. Generate chain-spec from fork-from node
	let (old_name, old_tag) = test_image("midnight-node-fork-from");
	let chainspec_json = generate_chainspec(&old_name, &old_tag);

	let tempdir = tempfile::tempdir().expect("failed to create tempdir");

	// 2. Start new node with fork-from chain-spec
	let (name, tag) = test_image("midnight-node");
	let node_image = format!("{name}:{tag}");
	let container = GenericImage::new(name, tag)
		.with_wait_for(WaitFor::message_on_stderr("Running JSON-RPC server"))
		.with_exposed_port(ContainerPort::Tcp(9944))
		.with_env_var("CFG_PRESET", "dev")
		.with_env_var("CHAIN", "/chainspec/chainspec.json")
		.with_copy_to("/chainspec/chainspec.json", chainspec_json.into_bytes())
		.start()
		.await
		.expect("failed to start midnight-node container");

	let port = container.get_host_port_ipv4(9944).await.expect("failed to get node RPC port");
	let url = format!("ws://127.0.0.1:{port}");

	// Wait for finality. The toolkit CLI calls get_block_one_hash on
	// transaction-generating commands, which fails with OnlyGenesisFinalized
	// until finalized height >= 1.
	wait_for_finalized_block(&url, 1, Duration::from_secs(60)).await;

	// 3. Pre-fork: run single-tx to verify the new node works with the fork-from chain-spec
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"single-tx",
		"--source-seed",
		"0000000000000000000000000000000000000000000000000000000000000001",
		"--unshielded-amount",
		"10",
		"--destination-address",
		"mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r",
		"--destination-address",
		"mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm",
		"--destination-address",
		"mn_addr_undeployed12vv6yst6exn50pkjjq54tkmtjpyggmr2p07jwpk6pxd088resqzqszfgak",
		"-s",
		&url,
		"-d",
		&url,
	])
	.await;

	// 4. Runtime upgrade: extract WASM from new node image and apply it
	let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
	let wasm_path_in_image =
		format!("/artifacts-{arch}/midnight_node_runtime.compact.compressed.wasm");
	let wasm_output = Command::new("docker")
		.args(["run", "--rm", "--entrypoint", "cat", &node_image, &wasm_path_in_image])
		.output()
		.expect("docker run cat wasm failed");
	assert!(
		wasm_output.status.success(),
		"failed to extract wasm: {}",
		String::from_utf8_lossy(&wasm_output.stderr)
	);
	let wasm_path = tempdir.path().join("runtime.wasm");
	std::fs::write(&wasm_path, &wasm_output.stdout).expect("write wasm");

	run_cli(&[
		"runtime-upgrade",
		"--wasm-file",
		wasm_path.to_str().unwrap(),
		"-c",
		"//Dave",
		"-c",
		"//Eve",
		"-t",
		"//Alice",
		"-t",
		"//Bob",
		"--rpc-url",
		&url,
		"--signer-key",
		"//Alice",
	])
	.await;

	// 5. GH #1959: the whole fork boundary must stay readable. The block that
	//    applied the new code carries a ledger-8 `StateKey` under ledger-9 `:code`,
	//    so the ledger-9 host API has to detect that and serve the read from the
	//    ledger-8 bridge; the block after it is already translated to v9 and must
	//    keep taking the ordinary ledger-9 path.
	let rpc = RpcClient::from_insecure_url(&url).await.expect("failed to open raw RPC client");
	let pre_fork_spec = {
		let hash = block_hash_at(&rpc, 1).await;
		spec_version_at(&rpc, &hash).await
	};
	let head = finalized_height(&rpc).await;
	let applied = find_code_applied_block(&rpc, head, pre_fork_spec).await;
	eprintln!(
		"[hardfork_e2e] new runtime code applied at #{applied} \
		 (pre-fork spec {pre_fork_spec}, finalized head #{head})"
	);
	assert!(applied > 1, "expected the code-applying block to be past #1, got #{applied}");
	assert!(applied < head, "expected the code-applying block to be below the finalized head");

	// `runtime-upgrade` already waits for finality to pass `applied`, but the
	// assertion below needs `applied + 1` to exist regardless.
	wait_for_finalized_block(&url, applied + 1, Duration::from_secs(60)).await;

	assert_ledger_state_readable(&rpc, applied - 1, "pre-fork").await;
	assert_ledger_state_readable(&rpc, applied, "code-applied block").await;
	assert_ledger_state_readable(&rpc, applied + 1, "post-migration").await;

	// 6. Post-fork: run single-tx again to verify the node still works after the (future) upgrade
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"single-tx",
		"--source-seed",
		"0000000000000000000000000000000000000000000000000000000000000001",
		"--unshielded-amount",
		"10",
		"--destination-address",
		"mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r",
		"--destination-address",
		"mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm",
		"--destination-address",
		"mn_addr_undeployed12vv6yst6exn50pkjjq54tkmtjpyggmr2p07jwpk6pxd088resqzqszfgak",
		"-s",
		&url,
		"-d",
		&url,
	])
	.await;
}

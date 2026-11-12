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

//! Fork-boundary e2e: build a chain-spec from the previous release, run the
//! current node on it, upgrade the runtime, and check the boundary.
//!
//! The previous release is runtime 2.1.0 (`midnight-node-fork-from` in
//! `test-images.docker-compose.yml`): AURA + GRANDPA session keys, committee
//! storage v1, and delayed runtime upgrades (`system_version` 3, so the new code
//! is staged in `:pending_code` and first executes one block after
//! `apply_authorized_upgrade`). The upgrade to the current runtime has to run the
//! committee/session-key migration (BABE and BEEFY keys) and the BEEFY genesis
//! reset, after which the chain must keep producing and finalizing blocks and
//! accepting transactions.
//!
//! Set `NODE_BINARY=target/release/midnight-node` to run the node under test as a
//! local process instead of the `midnight-node` docker image, which skips the image
//! build while iterating. The *fork-from* chain-spec still comes from a docker
//! image — that runtime is a past release. The runtime WASM to upgrade to is then
//! taken from next to the binary; `RUNTIME_WASM` overrides where to look.

mod common;

use clap::Parser;
use common::{test_image, wait_for_node::wait_for_finalized_block};
use midnight_node_toolkit::cli::{Cli, run_command};
use std::{
	net::TcpListener,
	path::{Path, PathBuf},
	process::{Child, Command},
	time::Duration,
};
use subxt::rpcs::{RpcClient, rpc_params};
use testcontainers::{
	ContainerAsync, GenericImage, ImageExt,
	core::{ContainerPort, WaitFor},
	runners::AsyncRunner,
};

/// Genesis-funded dev wallet the test transacts from.
const SOURCE_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000001";

/// SCALE encoding of a pallet's `StorageVersion` (a `u16`).
fn storage_version(v: u16) -> Vec<u8> {
	v.to_le_bytes().to_vec()
}

/// The compiled runtime blob, under whichever directory holds it.
const RUNTIME_WASM_FILE: &str = "midnight_node_runtime.compact.compressed.wasm";

/// The repo root. The test's own CWD is the toolkit crate, but `dev`'s preset
/// resolves its `res/…` paths against the CWD, and `NODE_BINARY` is documented
/// relative to the root — so both the node's working directory and the binary
/// path hang off this.
const REPO_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// `NODE_BINARY` as an absolute path, so the binary and the runtime WASM beside
/// it resolve the same way no matter what the test's CWD is.
fn node_binary_path(binary: &str) -> PathBuf {
	let path = Path::new(binary);
	if path.is_absolute() { path.to_owned() } else { Path::new(REPO_ROOT).join(path) }
}

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

/// The running node under test. Held for the duration of the test — dropping it
/// stops the node either way.
enum NodeUnderTest {
	Container { _container: ContainerAsync<GenericImage> },
	Local(Child),
}

impl Drop for NodeUnderTest {
	fn drop(&mut self) {
		if let NodeUnderTest::Local(child) = self {
			let _ = child.kill();
			let _ = child.wait();
		}
	}
}

/// In-process CLI calls leave websocket connections open, so the node's default cap of 100 runs
/// out during the post-fork steps (HTTP 429).
const RPC_MAX_CONNECTIONS_ARG: &str = "--rpc-max-connections 1000";

/// An unused localhost port, so a local node does not collide with whatever else
/// the developer has running.
fn free_port() -> u16 {
	TcpListener::bind("127.0.0.1:0")
		.expect("failed to bind an ephemeral port")
		.local_addr()
		.expect("ephemeral socket has no local address")
		.port()
}

/// Start the node under test on `chainspec` and return it with its RPC url.
async fn start_node(
	binary: Option<&str>,
	chainspec: String,
	tempdir: &Path,
) -> (NodeUnderTest, String) {
	let Some(binary) = binary else {
		let (name, tag) = test_image("midnight-node");
		let container = GenericImage::new(name, tag)
			.with_wait_for(WaitFor::message_on_stderr("Running JSON-RPC server"))
			.with_exposed_port(ContainerPort::Tcp(9944))
			.with_env_var("CFG_PRESET", "dev")
			.with_env_var("CHAIN", "/chainspec/chainspec.json")
			.with_env_var("APPEND_ARGS", RPC_MAX_CONNECTIONS_ARG)
			.with_copy_to("/chainspec/chainspec.json", chainspec.into_bytes())
			.start()
			.await
			.expect("failed to start midnight-node container");
		let port = container.get_host_port_ipv4(9944).await.expect("failed to get node RPC port");
		return (
			NodeUnderTest::Container { _container: container },
			format!("ws://127.0.0.1:{port}"),
		);
	};

	// The node takes its run arguments from the cfg rather than from argv — passing
	// any argument would *replace* the preset's `args` — so the ports go through
	// `APPEND_ARGS`. `dev`'s preset resolves its `res/…` paths against the CWD,
	// hence the repo root.
	let chainspec_path = tempdir.join("chainspec.json");
	std::fs::write(&chainspec_path, chainspec).expect("failed to write chainspec");
	let rpc_port = free_port();
	let child = Command::new(node_binary_path(binary))
		.current_dir(REPO_ROOT)
		.env("CFG_PRESET", "dev")
		.env("CHAIN", &chainspec_path)
		.env("BASE_PATH", tempdir.join("chain"))
		.env(
			"APPEND_ARGS",
			format!("--rpc-port {rpc_port} --port 0 --no-prometheus {RPC_MAX_CONNECTIONS_ARG}"),
		)
		.spawn()
		.unwrap_or_else(|e| panic!("failed to spawn NODE_BINARY {binary}: {e}"));
	eprintln!(
		"[hardfork_e2e] started local node {binary} (pid {}) with RPC on {rpc_port}",
		child.id()
	);
	(NodeUnderTest::Local(child), format!("ws://127.0.0.1:{rpc_port}"))
}

/// The runtime WASM the upgrade applies: built alongside `NODE_BINARY` when
/// running locally, otherwise copied out of the node image.
fn runtime_wasm(binary: Option<&str>) -> Vec<u8> {
	let Some(binary) = binary else {
		let (name, tag) = test_image("midnight-node");
		let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
		let path_in_image = format!("/artifacts-{arch}/{RUNTIME_WASM_FILE}");
		let output = Command::new("docker")
			.args(["run", "--rm", "--entrypoint", "cat", &format!("{name}:{tag}"), &path_in_image])
			.output()
			.expect("docker run cat wasm failed");
		assert!(
			output.status.success(),
			"failed to extract wasm: {}",
			String::from_utf8_lossy(&output.stderr)
		);
		return output.stdout;
	};

	let resolved = node_binary_path(binary);
	let dir = resolved.parent().expect("NODE_BINARY has no parent directory");
	// `cargo build`'s layout first, then the flat one of CI's binaries artifact.
	let candidates = [
		dir.join("wbuild/midnight-node-runtime").join(RUNTIME_WASM_FILE),
		dir.join(RUNTIME_WASM_FILE),
	];
	let path = match std::env::var("RUNTIME_WASM") {
		Ok(explicit) => PathBuf::from(explicit),
		Err(_) => candidates.iter().find(|p| p.exists()).cloned().unwrap_or_else(|| {
			panic!("no runtime WASM next to {binary} (tried {candidates:?}); set RUNTIME_WASM")
		}),
	};
	eprintln!("[hardfork_e2e] upgrading to runtime WASM at {}", path.display());
	std::fs::read(&path)
		.unwrap_or_else(|e| panic!("failed to read runtime WASM {}: {e}", path.display()))
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

/// Locate the first block whose committed state holds the *new* `:code`.
///
/// The pre-fork runtime ships `system_version: 3`: `apply_authorized_upgrade` in
/// block N only stages the code in `:pending_code`, block N+1 is built and executed
/// by the staged runtime (its `initialize_block` runs the upgrade's migrations), and
/// `:code` is swapped at the end of N+1. So the block found here is the first one the
/// new runtime executed, with its migrations applied, and the block before it is the
/// last one the old runtime executed.
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

/// A plain (non-map) storage value at `hash`, or `None` if unset.
async fn storage_at(rpc: &RpcClient, pallet: &[u8], item: &[u8], hash: &str) -> Option<Vec<u8>> {
	let key = format!(
		"0x{}{}",
		hex::encode(sp_crypto_hashing::twox_128(pallet)),
		hex::encode(sp_crypto_hashing::twox_128(item)),
	);
	let value: Option<String> = rpc
		.request("state_getStorage", rpc_params![&key, hash])
		.await
		.unwrap_or_else(|e| panic!("state_getStorage({key}) failed at {hash}: {e}"));
	value.map(|v| hex::decode(v.trim_start_matches("0x")).expect("hex-encoded storage value"))
}

/// Every way of reading the ledger state must answer at `height`.
///
/// Both the `midnight_*` RPCs and a raw `state_call`: the latter is the path
/// subxt-based tooling (`chain-indexer`, GH #1969) takes, and it does not go
/// anywhere near the node's own RPC layer. Across the fork boundary the state is
/// read with whichever runtime is stored at the block, so both sides of the
/// boundary and the boundary block itself are checked.
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
	let node_binary = std::env::var("NODE_BINARY").ok();
	let (_node, url) = start_node(node_binary.as_deref(), chainspec_json, tempdir.path()).await;

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
		SOURCE_SEED,
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

	// 4. Runtime upgrade: take the new WASM from the node under test and apply it
	let wasm_path = tempdir.path().join("runtime.wasm");
	std::fs::write(&wasm_path, runtime_wasm(node_binary.as_deref())).expect("write wasm");

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

	// 5. Locate the fork boundary: `applied` is the first block the new runtime
	//    executed (see `find_code_applied_block`), `applied - 1` the last one the old
	//    runtime executed.
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
	// assertions below need `applied + 1` to exist regardless.
	wait_for_finalized_block(&url, applied + 1, Duration::from_secs(60)).await;

	let pre_fork_hash = block_hash_at(&rpc, applied - 1).await;
	let applied_hash = block_hash_at(&rpc, applied).await;

	// 5a. The fork-from release must already be on ledger 9 (pallet-midnight storage
	//     version 2): the runtime no longer carries the ledger 8->9 translation, so a
	//     ledger-8 `FORK_FROM_NODE_IMAGE` would fail later in a far less obvious way
	//     (`apply_post_block_update` panicking on the untranslated state key).
	assert_eq!(
		storage_at(&rpc, b"Midnight", b":__STORAGE_VERSION__:", &pre_fork_hash).await,
		Some(storage_version(2)),
		"the fork-from release must be on ledger 9 (pallet-midnight storage version 2) \
		 before the upgrade; the runtime carries no ledger 8->9 migration anymore",
	);

	// 5b. The 3.0.0 migrations ran in the boundary block: the committee pallet moved to
	//     storage version 2 (BABE and BEEFY keys added to the session keys, queued
	//     committee initialised) and the BEEFY genesis was reset to `None` (SCALE `0x00`
	//     under a `ValueQuery`), which keeps BEEFY disabled until governance re-enables it.
	assert_eq!(
		storage_at(&rpc, b"SessionCommitteeManagement", b":__STORAGE_VERSION__:", &pre_fork_hash)
			.await,
		Some(storage_version(1)),
		"the fork-from release must still be on committee storage version 1",
	);
	assert_eq!(
		storage_at(&rpc, b"SessionCommitteeManagement", b":__STORAGE_VERSION__:", &applied_hash)
			.await,
		Some(storage_version(2)),
		"the committee/session-key migration must have run in the boundary block #{applied}",
	);
	assert_eq!(
		storage_at(&rpc, b"Beefy", b"GenesisBlock", &applied_hash).await,
		Some(vec![0]),
		"the BEEFY genesis must be reset to None in the boundary block #{applied}",
	);
	eprintln!("[hardfork_e2e] committee storage v1 -> v2 and BEEFY genesis reset at #{applied}");

	// 5c. The whole fork boundary must stay readable.
	assert_ledger_state_readable(&rpc, applied - 1, "pre-fork").await;
	assert_ledger_state_readable(&rpc, applied, "code-applied block").await;
	assert_ledger_state_readable(&rpc, applied + 1, "post-migration").await;

	// 5d. The chain keeps going on the migrated state: the authority set the migration
	//     rewrote must still produce and finalize blocks.
	wait_for_finalized_block(&url, applied + 3, Duration::from_secs(60)).await;

	// 6. Post-fork: run single-tx again to verify the node still works after the upgrade
	run_cli(&[
		"generate-txs",
		"--fetch-cache",
		"inmemory",
		"single-tx",
		"--source-seed",
		SOURCE_SEED,
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

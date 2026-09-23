// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

// Only built when the toolkit is compiled with the `indexer-client` feature (the default).
#![cfg(feature = "indexer-client")]

//! End-to-end tests for the indexer backend (`--indexer-url`, issue #1186).
//!
//! Each test spawns a dev `midnight-node` and an `indexer-standalone` on a shared Docker network
//! (so the indexer reaches the node at `ws://<node>:9944`) and waits for the node to finalize and
//! the indexer to catch up. `show-wallet` then runs for a funded genesis seed and must report
//! non-empty shielded coins, unshielded UTXOs and dust UTXOs; `generate-txs single-tx` builds from
//! indexer state and its transfer must show up in the destination's `show-wallet`.
//!
//! An uncached run is also the oracle for the incremental wallet cache: a cache-resumed sync must
//! reproduce it exactly, and an entry filed under another chain must never be served.
//!
//! Like the other container tests in this crate it needs Docker plus the pinned node/indexer
//! images (resolved via `test-images.docker-compose.yml`), so it only runs where those are
//! available (CI / a local Docker host).

mod common;

use common::{test_image, wait_for_node::wait_for_finalized_block};
use midnight_ledger_unsafe_helpers::IndexerClient;
use midnight_ledger_unsafe_helpers::ledger_9::{
	BuilderContext, DefaultDB, IndexerContext, IntoWalletAddress, UnshieldedWallet, WalletSeed,
};
use midnight_node_toolkit::client::MidnightNodeClient;
use parity_scale_codec::Decode;
use std::num::NonZeroUsize;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use subxt::rpcs::methods::legacy::BlockNumber;
use subxt::utils::H256;
use testcontainers::{
	ContainerAsync, GenericImage, ImageExt,
	core::{ContainerPort, WaitFor},
	runners::AsyncRunner,
};

/// Genesis seed funded with NIGHT/DUST in the `dev` (undeployed) preset — the same seed the
/// `show_wallet` unit tests assert is funded.
const FUNDED_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000001";
/// Unfunded in the `dev` preset, so any UTXO it shows came from the test's transfer.
const UNFUNDED_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000005";
const NETWORK: &str = "undeployed";
/// 32-byte (64 hex char) secret the indexer uses to encrypt its wallet session store. Any valid
/// hex works for a throwaway standalone instance, but it MUST contain a non-digit hex char: the
/// indexer's config loader does `try_parsing` on `APP__` env vars, so an all-numeric value is
/// coerced to an integer and rejected ("expected a string for key INFRA.SECRET").
const INDEXER_SECRET: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// A dev node plus an indexer that has caught up to the node's finalized tip. The containers stop
/// when this is dropped.
struct Env {
	_node: ContainerAsync<GenericImage>,
	_indexer: ContainerAsync<GenericImage>,
	node_ws: String,
	indexer_url: String,
}

/// Opt-in via `MN_RUN_INDEXER_E2E=1`: the `indexer-standalone` image is not yet published and
/// pinned in CI (issue #1186 follow-up).
fn e2e_enabled(test: &str) -> bool {
	let enabled = std::env::var_os("MN_RUN_INDEXER_E2E").is_some();
	if !enabled {
		eprintln!(
			"skipping {test}: set MN_RUN_INDEXER_E2E=1 to run \
			 (requires Docker plus the midnight-node and indexer-standalone images)"
		);
	}
	enabled
}

/// `tag` keeps container names distinct between tests of this process run in parallel.
async fn start_env(tag: &str) -> Env {
	// Unique names so concurrent runs don't collide on the shared network / container names.
	let suffix = format!("{}-{tag}", std::process::id());
	let network = format!("mn-indexer-e2e-{suffix}");
	let node_name = format!("mn-node-{suffix}");

	// --- node ---------------------------------------------------------------------------------
	let (node_image, node_tag) = test_image("midnight-node");
	let node = GenericImage::new(node_image, node_tag)
		.with_wait_for(WaitFor::message_on_stderr("Running JSON-RPC server"))
		.with_exposed_port(ContainerPort::Tcp(9944))
		.with_env_var("CFG_PRESET", "dev")
		.with_network(&network)
		.with_container_name(&node_name)
		.start()
		.await
		.expect("failed to start midnight-node container");

	let node_rpc_port = node.get_host_port_ipv4(9944).await.expect("failed to get node RPC port");
	let node_ws = format!("ws://127.0.0.1:{node_rpc_port}");
	wait_for_finalized_block(&node_ws, 1, Duration::from_secs(90)).await;
	let node_height = node_finalized_height(&node_ws).await;

	// --- indexer ------------------------------------------------------------------------------
	// The indexer reaches the node over the shared network by its container name.
	let node_url = format!("ws://{node_name}:9944");
	let (indexer_image, indexer_tag) = test_image("indexer-standalone");
	let indexer = GenericImage::new(indexer_image, indexer_tag)
		.with_exposed_port(ContainerPort::Tcp(8088))
		.with_network(&network)
		.with_env_var("APP__INFRA__SECRET", INDEXER_SECRET)
		.with_env_var("APP__INFRA__NODE__URL", &node_url)
		.with_env_var("APP__INFRA__SPO_NODE__URL", &node_url)
		.with_env_var("APP__INFRA__SPO_NODE__BLOCKFROST_ID", "dummy-not-using-spo")
		.start()
		.await
		.expect("failed to start indexer-standalone container");

	let indexer_port = indexer.get_host_port_ipv4(8088).await.expect("failed to get indexer port");
	let indexer_url = format!("http://127.0.0.1:{indexer_port}/api/v4");

	// Wait until the indexer has caught up to the node's finalized tip, else balances read short.
	// At least height 2: genesis has no `Timestamp::Now` in storage for the block-context check
	// to compare a parent against.
	wait_for_indexer_height(&indexer_url, node_height.max(2), Duration::from_secs(180)).await;

	Env { _node: node, _indexer: indexer, node_ws, indexer_url }
}

#[tokio::test]
async fn indexer_show_wallet_reports_genesis_balances() {
	if !e2e_enabled("indexer_show_wallet_reports_genesis_balances") {
		return;
	}
	let env = start_env("show").await;
	let (node_ws, indexer_url) = (env.node_ws.as_str(), env.indexer_url.as_str());

	assert_context_reads_match_node(indexer_url, node_ws).await;

	// --- run show-wallet against the indexer --------------------------------------------------
	// Baseline: caching disabled, so every stream drains from the origin.
	let baseline = show_wallet(indexer_url, FUNDED_SEED, None);

	let utxos = baseline["utxos"].as_array().expect("`utxos` should be an array");
	let coins = baseline["coins"].as_object().expect("`coins` should be an object");
	let dust = baseline["dust_utxos"].as_array().expect("`dust_utxos` should be an array");

	assert!(!utxos.is_empty(), "expected non-empty unshielded UTXOs for funded seed");
	assert!(!coins.is_empty(), "expected non-empty shielded coins for funded seed");
	assert!(!dust.is_empty(), "expected non-empty dust UTXOs for funded seed");

	// --- incremental wallet cache ---------------------------------------------------------------
	let cache = tempfile::tempdir().expect("failed to create cache dir");
	assert_eq!(
		show_wallet(indexer_url, FUNDED_SEED, Some(cache.path())),
		baseline,
		"enabling the cache must not change the first (still cold) run's answer",
	);
	assert_eq!(
		show_wallet(indexer_url, FUNDED_SEED, Some(cache.path())),
		baseline,
		"a cache-resumed sync must reproduce the full drain exactly",
	);

	// Entries are namespaced by block 1's hash, so one chain's state can never be served against
	// another's indexer. Renaming that directory away must therefore force a full drain.
	let chain_id = IndexerClient::new(indexer_url)
		.expect("failed to build indexer client")
		.block_hash_at(1)
		.await
		.expect("block 1 query failed")
		.expect("indexer has not indexed block 1");
	let chain_dir = cache.path().join(hex::encode(chain_id));
	assert!(chain_dir.is_dir(), "cache must be namespaced by block 1's hash, found: {chain_dir:?}");
	std::fs::rename(&chain_dir, cache.path().join("ff".repeat(32)))
		.expect("failed to rename chain-id directory");

	assert_eq!(
		show_wallet(indexer_url, FUNDED_SEED, Some(cache.path())),
		baseline,
		"a cache entry under another chain id must be ignored, not served",
	);
	assert!(chain_dir.is_dir(), "the full-drain fallback must re-file its state under this chain");
}

#[tokio::test]
async fn indexer_generate_txs_single_tx_reaches_destination() {
	if !e2e_enabled("indexer_generate_txs_single_tx_reaches_destination") {
		return;
	}
	const AMOUNT: u64 = 1_000_000;
	let env = start_env("gen").await;

	let unshielded_values = |wallet: serde_json::Value| -> Vec<u64> {
		wallet["utxos"]
			.as_array()
			.expect("`utxos` should be an array")
			.iter()
			.map(|u| u["value"].as_u64().expect("utxo `value` should be a u64"))
			.collect()
	};
	assert!(
		unshielded_values(show_wallet(&env.indexer_url, UNFUNDED_SEED, None)).is_empty(),
		"the destination seed must start unfunded"
	);

	let destination =
		UnshieldedWallet::default(WalletSeed::try_from_hex_str(UNFUNDED_SEED).unwrap())
			.address(NETWORK)
			.to_bech32();
	let amount = AMOUNT.to_string();
	run_toolkit(&[
		"generate-txs",
		"--indexer-url",
		&env.indexer_url,
		"--network",
		NETWORK,
		"--fetch-cache",
		"inmemory",
		"--dest-url",
		&env.node_ws,
		"single-tx",
		"--source-seed",
		FUNDED_SEED,
		"--unshielded-amount",
		&amount,
		"--destination-address",
		&destination,
	]);

	// The indexer serves finalized blocks only, so poll until the transfer is indexed.
	let start = Instant::now();
	loop {
		let values = unshielded_values(show_wallet(&env.indexer_url, UNFUNDED_SEED, None));
		if values.contains(&AMOUNT) {
			break;
		}
		assert!(
			start.elapsed() < Duration::from_secs(180),
			"destination never received the {AMOUNT} transfer; its UTXOs: {values:?}"
		);
		tokio::time::sleep(Duration::from_secs(3)).await;
	}
}

/// Run the toolkit binary with `args`, panicking with its output on failure; returns stdout.
fn run_toolkit(args: &[&str]) -> String {
	let output = Command::new(env!("CARGO_BIN_EXE_midnight-node-toolkit"))
		.args(args)
		.output()
		.expect("failed to run midnight-node-toolkit");
	assert!(
		output.status.success(),
		"midnight-node-toolkit {args:?} failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
		output.status.code(),
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Run `show-wallet --indexer-url …` for `seed` and return its parsed JSON.
///
/// `cache_dir` enables the wallet cache under that directory; `None` disables it, which is what
/// `--fetch-cache inmemory` means to the toolkit.
fn show_wallet(indexer_url: &str, seed: &str, cache_dir: Option<&Path>) -> serde_json::Value {
	let mut args = vec![
		"show-wallet".to_string(),
		"--indexer-url".to_string(),
		indexer_url.to_string(),
		"--network".to_string(),
		NETWORK.to_string(),
		"--seed".to_string(),
		seed.to_string(),
	];
	match cache_dir {
		Some(dir) => args.extend([
			"--ledger-state-db".to_string(),
			dir.display().to_string(),
			"--fetch-cache".to_string(),
			format!("redb:{}", dir.join("fetch_cache.db").display()),
		]),
		None => args.extend(["--fetch-cache".to_string(), "inmemory".to_string()]),
	}

	let args: Vec<&str> = args.iter().map(String::as_str).collect();
	let stdout = run_toolkit(&args);
	serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("failed to parse show-wallet JSON ({e}):\n{stdout}"))
}

/// `IndexerContext::ledger_parameters` / `latest_block_context` must agree with the node. Uses
/// the ledger-9 context: the pinned node image tracks main.
///
/// The block context is checked against the node block its own `parent_block_hash` names, so the
/// chain advancing between the two reads cannot race the comparison.
async fn assert_context_reads_match_node(indexer_url: &str, node_ws: &str) {
	let ctx = IndexerContext::<DefaultDB>::new(indexer_url, NETWORK, NonZeroUsize::MIN)
		.expect("failed to build indexer context");
	let node = MidnightNodeClient::new(node_ws, Some(Duration::from_secs(30)))
		.await
		.expect("failed to connect to node");

	assert_eq!(
		ctx.ledger_parameters().await,
		node.get_ledger_parameters().await.expect("node ledger parameters"),
		"indexer ledger parameters must match the node's",
	);

	let block_ctx = ctx.latest_block_context().await;
	let parent_hash = H256(block_ctx.parent_block_hash.0);
	let parent = node
		.rpc
		.chain_get_header(Some(parent_hash))
		.await
		.expect("parent header query")
		.expect("node does not know the indexer's parent block");
	let child_hash = node
		.rpc
		.chain_get_block_hash(Some(BlockNumber::Number(parent.number + 1)))
		.await
		.expect("block hash query")
		.expect("node has no block after the indexer's parent");

	assert_eq!(block_ctx.last_block_time.to_secs(), node_timestamp_secs(&node, parent_hash).await);
	assert_eq!(block_ctx.tblock.to_secs(), node_timestamp_secs(&node, child_hash).await);
}

/// `Timestamp::Now` at `hash`, read raw so it holds across runtime versions.
async fn node_timestamp_secs(node: &MidnightNodeClient, hash: H256) -> u64 {
	let key =
		[sp_crypto_hashing::twox_128(b"Timestamp"), sp_crypto_hashing::twox_128(b"Now")].concat();
	let raw = node
		.api
		.at_block(hash)
		.await
		.expect("node at_block")
		.storage()
		.fetch_raw(key)
		.await
		.expect("Timestamp::Now");
	u64::decode(&mut &raw[..]).expect("decode Timestamp::Now") / 1000
}

/// Read the node's current finalized height (used as the indexer catch-up target).
async fn node_finalized_height(ws_url: &str) -> u64 {
	let client = MidnightNodeClient::new(ws_url, Some(Duration::from_secs(30)))
		.await
		.unwrap_or_else(|e| panic!("failed to connect to node {ws_url}: {e}"));
	client
		.get_finalized_height()
		.await
		.expect("failed to read node finalized height")
}

/// Poll the indexer's latest block until its height reaches `target`, or panic on timeout.
async fn wait_for_indexer_height(indexer_url: &str, target: u64, timeout: Duration) {
	let client = IndexerClient::new(indexer_url).expect("failed to build indexer client");
	let start = Instant::now();
	loop {
		match client.latest_block().await {
			Ok(block) if block.height >= target => {
				eprintln!(
					"[indexer] caught up: height {} >= target {target} ({:.1}s)",
					block.height,
					start.elapsed().as_secs_f32()
				);
				return;
			},
			Ok(block) => eprintln!(
				"[indexer] height {} < target {target} ({:.1}s)",
				block.height,
				start.elapsed().as_secs_f32()
			),
			Err(e) => eprintln!("[indexer] block query not ready yet: {e}"),
		}
		if start.elapsed() >= timeout {
			panic!("indexer did not reach height {target} within {timeout:?}");
		}
		tokio::time::sleep(Duration::from_secs(2)).await;
	}
}

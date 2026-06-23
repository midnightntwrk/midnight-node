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

//! End-to-end test for indexer-backed `show-wallet` (issue #1186).
//!
//! Spawns a dev `midnight-node` and an `indexer-standalone` on a shared Docker network (so the
//! indexer reaches the node at `ws://<node>:9944`), waits for the node to finalize and the indexer
//! to catch up, then runs `show-wallet --indexer-url …` for a funded genesis seed and asserts the
//! reconstructed wallet reports non-empty shielded coins, unshielded UTXOs and dust UTXOs.
//!
//! Like the other container tests in this crate it needs Docker plus the pinned node/indexer
//! images (resolved via `test-images.docker-compose.yml`), so it only runs where those are
//! available (CI / a local Docker host).

mod common;

use common::{test_image, wait_for_node::wait_for_finalized_block};
use midnight_node_ledger_helpers::IndexerClient;
use midnight_node_toolkit::client::MidnightNodeClient;
use std::process::Command;
use std::time::{Duration, Instant};
use testcontainers::{
	GenericImage, ImageExt,
	core::{ContainerPort, WaitFor},
	runners::AsyncRunner,
};

/// Genesis seed funded with NIGHT/DUST in the `dev` (undeployed) preset — the same seed the
/// `show_wallet` unit tests assert is funded.
const FUNDED_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const NETWORK: &str = "undeployed";
/// 32-byte hex secret the indexer uses to encrypt its wallet session store. Any value works for a
/// throwaway standalone instance.
const INDEXER_SECRET: &str = "0000000000000000000000000000000000000000000000000000000000000001";

#[tokio::test]
async fn indexer_show_wallet_reports_genesis_balances() {
	// Opt-in: the `indexer-standalone` image is not yet published/pinned in CI (issue #1186
	// follow-up), so this test only runs when explicitly enabled. Set `MN_RUN_INDEXER_E2E=1`
	// (and ensure Docker + the node/indexer images are available) to run it.
	if std::env::var_os("MN_RUN_INDEXER_E2E").is_none() {
		eprintln!(
			"skipping indexer_show_wallet_e2e: set MN_RUN_INDEXER_E2E=1 to run \
			 (requires Docker plus the midnight-node and indexer-standalone images)"
		);
		return;
	}

	// Unique names so concurrent runs don't collide on the shared network / container names.
	let suffix = std::process::id();
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
	wait_for_indexer_height(&indexer_url, node_height, Duration::from_secs(180)).await;

	// --- run show-wallet against the indexer --------------------------------------------------
	let bin = env!("CARGO_BIN_EXE_midnight-node-toolkit");
	let output = Command::new(bin)
		.args([
			"show-wallet",
			"--indexer-url",
			&indexer_url,
			"--network",
			NETWORK,
			"--seed",
			FUNDED_SEED,
		])
		.output()
		.expect("failed to run midnight-node-toolkit");

	assert!(
		output.status.success(),
		"show-wallet --indexer-url failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
		output.status.code(),
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("failed to parse show-wallet JSON ({e}):\n{stdout}"));

	let utxos = json["utxos"].as_array().expect("`utxos` should be an array");
	let coins = json["coins"].as_object().expect("`coins` should be an object");
	let dust = json["dust_utxos"].as_array().expect("`dust_utxos` should be an array");

	assert!(!utxos.is_empty(), "expected non-empty unshielded UTXOs for funded seed");
	assert!(!coins.is_empty(), "expected non-empty shielded coins for funded seed");
	assert!(!dust.is_empty(), "expected non-empty dust UTXOs for funded seed");
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

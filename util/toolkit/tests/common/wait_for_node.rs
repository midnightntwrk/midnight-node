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

//! Wait until the node's **finalized** (GRANDPA-confirmed) block height reaches
//! a target. The toolkit CLI calls `get_block_one_hash` on transaction-generating
//! commands, which fails with `OnlyGenesisFinalized` until finality has reached
//! block 1, so finality (not best-block) is what tests need to wait for.

use midnight_node_ledger_helpers::fork::raw_block_data::LedgerVersion;
use midnight_node_toolkit::client::MidnightNodeClient;
use std::time::{Duration, Instant};

/// Wait until the finalized ledger state has been translated to ledger 9.
///
/// The ledger 8 -> 9 state translation is a multi-block migration, so for the
/// blocks it spans the chain still carries a ledger-8 state root even though the
/// ledger-9 runtime is live. A client that syncs to such a head would build
/// ledger-8 transactions; those blocks admit inherents only, so nothing is lost
/// by waiting for the migration to land first.
pub async fn wait_for_ledger_9_state(ws_url: &str, timeout: Duration) {
	let client = connect(ws_url, timeout).await;
	let start = Instant::now();
	loop {
		match client.get_state_root_at(None).await {
			Ok(Some(root)) => match LedgerVersion::from_state_root(&root) {
				Some(LedgerVersion::Ledger9) => {
					eprintln!(
						"[wait_for_ledger_9] finalized state is ledger 9 (elapsed: {:.1}s)",
						start.elapsed().as_secs_f32()
					);
					return;
				},
				other => eprintln!(
					"[wait_for_ledger_9] finalized state is {other:?}, migration still pending (elapsed: {:.1}s)",
					start.elapsed().as_secs_f32()
				),
			},
			Ok(None) => eprintln!("[wait_for_ledger_9] no StateKey in storage yet"),
			Err(e) => eprintln!("[wait_for_ledger_9] rpc error fetching StateKey: {e}"),
		}
		if start.elapsed() >= timeout {
			panic!(
				"timed out after {:?} waiting for the ledger 8->9 migration to complete on {ws_url}",
				start.elapsed()
			);
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
}

pub async fn wait_for_finalized_block(ws_url: &str, target_block: u64, timeout: Duration) {
	let client = connect(ws_url, timeout).await;
	let start = Instant::now();
	loop {
		match client.get_finalized_height().await {
			Ok(h) if h >= target_block => {
				eprintln!(
					"[wait_for_block] reached finalized block {h} (target {target_block}, elapsed: {:.1}s)",
					start.elapsed().as_secs_f32()
				);
				return;
			},
			Ok(h) => eprintln!(
				"[wait_for_block] finalized block {h} < target {target_block} (elapsed: {:.1}s)",
				start.elapsed().as_secs_f32()
			),
			Err(e) => eprintln!("[wait_for_block] rpc error fetching finalized height: {e}"),
		}
		bail_or_sleep(start, timeout, "finalized", target_block, ws_url).await;
	}
}

/// Wait until finality advances at least one block past the current finalized height.
///
/// Retroactive DUST accrues with block time, so a wallet funded in the latest block has
/// accrued nothing yet (`dt = 0`); one more block is enough for a self-funded registration.
// Shared across test binaries; not every binary uses it.
#[allow(dead_code)]
pub async fn wait_for_next_finalized_block(ws_url: &str, timeout: Duration) {
	let client = connect(ws_url, timeout).await;
	let current = client
		.get_finalized_height()
		.await
		.unwrap_or_else(|e| panic!("failed to fetch finalized height from {ws_url}: {e}"));
	wait_for_finalized_block(ws_url, current + 1, timeout).await;
}

async fn connect(ws_url: &str, timeout: Duration) -> MidnightNodeClient {
	let connect_timeout = timeout.min(Duration::from_secs(60));
	MidnightNodeClient::new(ws_url, Some(connect_timeout))
		.await
		.unwrap_or_else(|e| panic!("failed to connect to {ws_url}: {e}"))
}

async fn bail_or_sleep(
	start: Instant,
	timeout: Duration,
	label: &str,
	target_block: u64,
	ws_url: &str,
) {
	if start.elapsed() >= timeout {
		panic!(
			"timed out after {:?} waiting for {label} block >= {target_block} on {ws_url}",
			start.elapsed()
		);
	}
	tokio::time::sleep(Duration::from_secs(1)).await;
}

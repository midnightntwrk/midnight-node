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

//! Two flavours of "is the node ready" wait, picked deliberately at the call
//! site:
//!
//! * [`wait_for_unfinalized_block`] — best-chain head; fast, sufficient for
//!   "node is alive and producing".
//! * [`wait_for_finalized_block`] — GRANDPA-finalized; required when the test
//!   asserts on something only meaningful after finality (e.g. startup smoke
//!   checks that finality is advancing).

use midnight_node_toolkit::client::MidnightNodeClient;
use std::time::{Duration, Instant};

/// Wait until the node's **best** (head-of-chain) block height reaches `target`.
/// Best ≠ finalized: a best block can still be reorged. Use this when you only
/// need "the node is producing".
pub async fn wait_for_unfinalized_block(ws_url: &str, target_block: u64, timeout: Duration) {
	let client = connect(ws_url, timeout).await;
	let start = Instant::now();
	loop {
		match client.get_best_height().await {
			Ok(h) if h >= target_block => {
				eprintln!(
					"[wait_for_block] reached best block {h} (target {target_block}, elapsed: {:.1}s)",
					start.elapsed().as_secs_f32()
				);
				return;
			},
			Ok(h) => eprintln!(
				"[wait_for_block] best block {h} < target {target_block} (elapsed: {:.1}s)",
				start.elapsed().as_secs_f32()
			),
			Err(e) => eprintln!("[wait_for_block] rpc error fetching best height: {e}"),
		}
		bail_or_sleep(start, timeout, "best", target_block, ws_url).await;
	}
}

/// Wait until the node's **finalized** (GRANDPA-confirmed) block height
/// reaches `target`. Slower but stable — use when finality matters.
///
/// Currently no Rust test calls this; kept as part of the deliberate API pair
/// so future callers don't have to add it back from scratch.
#[allow(dead_code)]
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

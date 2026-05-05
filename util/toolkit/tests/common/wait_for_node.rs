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

use midnight_node_toolkit::client::MidnightNodeClient;
use std::time::{Duration, Instant};

pub async fn wait_for_block(ws_url: &str, target_block: u64, timeout: Duration) {
	let connect_timeout = timeout.min(Duration::from_secs(60));
	let client = MidnightNodeClient::new(ws_url, Some(connect_timeout))
		.await
		.unwrap_or_else(|e| panic!("failed to connect to {ws_url}: {e}"));

	let start = Instant::now();
	let poll = Duration::from_secs(1);
	loop {
		match client.get_finalized_height().await {
			Ok(h) if h >= target_block => {
				eprintln!(
					"[wait_for_block] reached block {h} (target {target_block}, elapsed: {:.1}s)",
					start.elapsed().as_secs_f32()
				);
				return;
			},
			Ok(h) => eprintln!(
				"[wait_for_block] block {h} < target {target_block} (elapsed: {:.1}s)",
				start.elapsed().as_secs_f32()
			),
			Err(e) => eprintln!("[wait_for_block] rpc error: {e}"),
		}
		if start.elapsed() >= timeout {
			panic!(
				"timed out after {:?} waiting for block >= {target_block} on {ws_url}",
				start.elapsed()
			);
		}
		tokio::time::sleep(poll).await;
	}
}

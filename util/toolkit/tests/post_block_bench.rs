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

//! Where a block's `LedgerState::post_block_update` spends its time, measured on a real
//! ledger snapshot. The replay runs it once per block, empty or not, and the phase
//! timers show it dominating long replays, so this attributes it to the ledger's own
//! sub-steps. Ignored by default; run with a snapshot written by `--ledger-state-db`:
//!
//! ```text
//! MN_BENCH_SNAPSHOT=<ledger_state_db>/<chain id>/ledger/<height>.zstd \
//!   cargo test --release -p midnight-node-toolkit --test post_block_bench -- --ignored --nocapture
//! ```

use midnight_ledger_unsafe_helpers::ledger_8::{
	DefaultDB, Sp, SyntheticCost, Timestamp, clamp_and_normalize, compute_overall_fullness,
};
use midnight_node_toolkit::fetcher::wallet_state_cache::{
	LedgerSnapshot, restore_context_from_ledger_snapshot_8,
};
use std::time::Instant;

const ITERATIONS: u32 = 200;

fn timed(label: &str, mut step: impl FnMut(u64)) {
	let t = Instant::now();
	for i in 0..ITERATIONS {
		step(i as u64);
	}
	println!("{label:>40}: {:>10.1?} per block", t.elapsed() / ITERATIONS);
}

#[test]
#[ignore]
fn post_block_update_breakdown_ledger8() {
	let path = std::env::var("MN_BENCH_SNAPSHOT")
		.expect("set MN_BENCH_SNAPSHOT to a ledger-8 snapshot file (<height>.zstd)");
	let height: u64 = std::path::Path::new(&path)
		.file_stem()
		.and_then(|s| s.to_str())
		.and_then(|s| s.parse().ok())
		.expect("snapshot file name must be the block height");
	let t = Instant::now();
	let bytes = std::fs::read(&path).expect("read snapshot");
	let snapshot = LedgerSnapshot::from_value_bytes(&bytes, height).expect("decode snapshot");
	let (ctx, state, _) = restore_context_from_ledger_snapshot_8(&snapshot).expect("restore");
	println!(
		"snapshot at block {height}: {} bytes compressed, {} bytes of ledger state, restored in {:?}",
		bytes.len(),
		snapshot.ledger_state_bytes.len(),
		t.elapsed()
	);

	let base = ctx.latest_block_context().tblock.to_secs();
	let at = |i: u64| Timestamp::from_secs(base + 6 * (i + 1));
	let limits = state.parameters.limits.block_limits;
	let fullness = || {
		let normalized = clamp_and_normalize(&SyntheticCost::ZERO, &limits, "bench");
		(normalized, compute_overall_fullness(&normalized))
	};

	// A freshly restored state pays one-off costs on its first blocks (Merkle trees
	// rehashed from scratch); replay a few blocks first so the steps below see the
	// steady state a long replay runs in.
	const WARM_UP: u64 = 20;
	let mut state = state;
	let t = Instant::now();
	for i in 0..WARM_UP {
		let (normalized, overall) = fullness();
		state = state.post_block_update(at(i), normalized, overall).expect("warm-up");
	}
	println!(
		"{:>40}: {:>10.1?} per block",
		"post_block_update, first blocks after restore",
		t.elapsed() / WARM_UP as u32
	);
	let at = |i: u64| at(WARM_UP + i);

	timed("LedgerState::clone", |_| {
		let _ = state.clone();
	});
	timed("Sp::new(parameters.clone())", |_| {
		let _: Sp<_, DefaultDB> = Sp::new((*state.parameters).clone());
	});
	timed("replay_protection.post_block_update + Sp", |i| {
		let _: Sp<_, DefaultDB> = Sp::new(state.replay_protection.post_block_update(at(i)));
	});
	timed("zswap.post_block_update + Sp", |i| {
		let _: Sp<_, DefaultDB> = Sp::new(state.zswap.post_block_update(at(i)));
	});
	timed("LedgerState::post_block_update (same base)", |i| {
		let (normalized, overall) = fullness();
		let _ = state.post_block_update(at(i), normalized, overall).expect("post block update");
	});
	timed("LedgerState::post_block_update (chained)", |i| {
		let (normalized, overall) = fullness();
		state = state.post_block_update(at(i), normalized, overall).expect("post block update");
	});
}

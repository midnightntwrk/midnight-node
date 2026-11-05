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

//! Root bookkeeping of the on-disk ledger arena. A binary of its own, so the arena's
//! roots are only this test's.

use midnight_ledger_unsafe_helpers::{WalletSeed, arena_db};
use midnight_node_ledger_helpers::fork::raw_block_data::{LedgerVersion, RawBlockData};
use midnight_node_toolkit::{
	fetcher::{
		fetch_storage::{WalletStateCaching, file_backend::FileBackend},
		ledger_arena,
		wallet_state_cache::{self, serialize_ledger_state_fast},
	},
	serde_def::SourceTransactions,
	tx_generator::builder::{build_fork_aware_context_cached, build_fork_aware_context_raw},
};
use std::time::Duration;

/// `l8` empty ledger-8 blocks followed by `l9` empty ledger-9 blocks.
fn synthetic_source(l8: u64, l9: u64) -> SourceTransactions {
	let block = |number: u64, version: LedgerVersion| RawBlockData {
		hash: {
			let mut h = [0u8; 32];
			h[..8].copy_from_slice(&number.to_le_bytes());
			h
		},
		parent_hash: [0; 32],
		number,
		ledger_version: version,
		transactions: vec![],
		tblock_secs: 1_700_000_000 + number * 6,
		tblock_err: 30,
		parent_block_hash: [0; 32],
		last_block_time_secs: 1_700_000_000 + number.saturating_sub(1) * 6,
		state_root: None,
		state: None,
	};
	let blocks = (0..l8)
		.map(|n| block(n, LedgerVersion::Ledger8))
		.chain((l8..l8 + l9).map(|n| block(n, LedgerVersion::Ledger9)))
		.collect();
	SourceTransactions::new(blocks, "undeployed")
}

fn root_count() -> usize {
	arena_db::try_default_storage()
		.unwrap()
		.arena
		.with_backend(|b| b.get_roots().len())
}

/// Every snapshot file on disk owns one root; roots of deleted or replaced snapshots are
/// released, and a GC keeps everything the remaining roots reach.
#[tokio::test]
async fn snapshot_roots_survive_gc_and_released_roots_do_not_leak() {
	let arena = tempfile::TempDir::new().unwrap();
	ledger_arena::open(arena.path()).unwrap();

	let source = synthetic_source(6, 4);
	let chain_id = source.chain_id().unwrap();
	let seeds = vec![WalletSeed::try_from_hex_str(&format!("{:0>64}", "01")).unwrap()];
	let raw = build_fork_aware_context_raw(&source, &seeds).into_ledger9().unwrap();
	let raw_bytes = serialize_ledger_state_fast(&raw.ledger_state.lock().unwrap()).unwrap();

	let files = tempfile::TempDir::new().unwrap();
	let backend = FileBackend::new(files.path());
	// A checkpoint every 2 blocks, across the 8 -> 9 fork.
	let _ = build_fork_aware_context_cached(&seeds, &source, Some(&backend), 2).await;
	let snapshots = std::fs::read_dir(files.path().join(hex::encode(chain_id.0)).join("ledger"))
		.unwrap()
		.count();
	assert!(snapshots >= 2, "checkpoints and the tip each leave a snapshot");
	assert_eq!(root_count(), snapshots, "one arena root per snapshot file, none left by the fork");

	// Saving the tip again replaces its snapshot, and its root, in place.
	let _ = build_fork_aware_context_cached(&seeds, &source, Some(&backend), 0).await;
	assert_eq!(root_count(), snapshots);

	// Nothing but the roots matters to a GC; what they reach must survive it.
	let removed = arena_db::try_default_storage()
		.unwrap()
		.arena
		.with_backend(|b| b.gc(Duration::from_secs(3600)));
	let warm = build_fork_aware_context_cached(&seeds, &source, Some(&backend), 0)
		.await
		.into_ledger9()
		.unwrap();
	let warm_bytes = serialize_ledger_state_fast(&warm.ledger_state.lock().unwrap()).unwrap();
	assert_eq!(warm_bytes, raw_bytes, "state restored after a GC that removed {removed} nodes");

	// Deleting a snapshot file hands back its root, which the caller releases. Files
	// younger than the backend's grace period are kept, so this may release nothing.
	let tip = source.blocks.last().unwrap().number;
	let released = backend.gc_ledger_snapshots(chain_id, &[tip]).await;
	for snapshot in &released {
		wallet_state_cache::release_snapshot_root(snapshot);
	}
	ledger_arena::flush();
	assert_eq!(root_count(), snapshots - released.len());
}

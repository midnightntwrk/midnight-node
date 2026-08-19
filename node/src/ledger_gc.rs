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

//! Ledger GC worker: `release_tagged` wrappers whose block *number* has left
//! the Substrate pruning window, then run incremental arena `gc`.
//!
//! Wrappers are persisted in the same `on_finalize` flush as the tip itself
//! (`persist_tagged(block_number)`). This worker only decides *when* they
//! may go. Reclaim is staged (no flush). Arena mark/sweep still skips a dirty
//! cache. Under `ArchiveAll` and `ArchiveCanonical`, tip reclaim is skipped
//! (history wrappers stay; stale forks at a shared height leak until — or,
//! under archive, forever) but the arena GC loop still runs.

use std::{collections::HashSet, sync::Arc, time::Duration};

use futures::{FutureExt, StreamExt};
use log::{debug, error, info, warn};
use midnight_node_runtime::opaque::Block;
use sc_client_api::{Backend, BlockchainEvents};
use sc_client_db::PruningMode;
use sc_service::SpawnTaskHandle;
use sp_blockchain::HeaderBackend;
use sp_runtime::{SaturatedConversion, traits::NumberFor};

use crate::service::{FullBackend, FullClient};

const LOG_TARGET: &str = "midnight::ledger_gc";
/// Soft time bound for one arena mark/sweep slice after finality. Storage-core
/// GC is incremental across calls, so backlog continues on later notifications
/// instead of looping under the arena mutex in a single tick.
const GC_SLICE: Duration = Duration::from_millis(25);

/// Unpersist lag = the effective `--state-pruning` window.
///
/// `None` means tip reclaim is off (`ArchiveAll` / `ArchiveCanonical`) —
/// Anchored history wrappers must stay — but the arena GC loop still runs to
/// cull zero-ref transient/intermediate nodes unpersisted during block
/// execution. Number tags cannot distinguish a canonical tip from a stale
/// fork at the same height, so `ArchiveCanonical` does not reclaim forks.
///
/// A `None` *config* does not mean archive: the CLI's `[default: 256]` is
/// help text only, so a node started without the flag reaches here with
/// `None` while sc-client-db prunes at `PruningMode::default()` (constrained,
/// 256). Map it through that same default. If the DB was created with a
/// larger window and the flag later dropped, `have_state_at` (checked past
/// the lag) still keeps wrappers live until actual pruning, so the mismatch
/// is leak-only. (`None` resolving to a stored *archive* mode on an existing
/// DB is diverted in `try_spawn` via the DB-effective `requires_full_sync()`
/// before this mapping is consulted.)
fn discovery_depth(state_pruning: &Option<PruningMode>) -> Option<u32> {
	match state_pruning.clone().unwrap_or_default() {
		PruningMode::Constrained(c) => Some(c.max_blocks.unwrap_or(0)),
		PruningMode::ArchiveCanonical | PruningMode::ArchiveAll => None,
	}
}

fn number_from_tag(tag: &[u8]) -> Option<u32> {
	midnight_node_ledger::latest::block_number_from_persist_tag(tag)
}

/// Keep the wrapper while inside pruning lag or while canonical state at that
/// height is still present. Missing canonical hash → reclaimable leftover.
/// Transient `hash` errors → keep.
fn height_still_live(
	client: &FullClient,
	backend: &FullBackend,
	n: u32,
	lag: NumberFor<Block>,
) -> bool {
	let number: NumberFor<Block> = n.saturated_into();
	let finalized = client.info().finalized_number;
	if finalized.saturating_sub(number) < lag {
		return true;
	}
	match client.hash(number) {
		Ok(Some(hash)) => backend.have_state_at(hash, number),
		Ok(None) => false,
		Err(e) => {
			debug!(target: LOG_TARGET, "⏸️  hash({n}) failed ({e}); keeping wrapper");
			true
		},
	}
}

fn reclaim_slice(
	client: &FullClient,
	backend: &FullBackend,
	lag: NumberFor<Block>,
	prev_pruned: HashSet<u32>,
) -> HashSet<u32> {
	let mut candidates = HashSet::new();
	let mut to_release = Vec::new();

	for tag in midnight_node_ledger::gc::tagged_root_tags() {
		let Some(n) = number_from_tag(&tag) else {
			continue;
		};
		if height_still_live(client, backend, n, lag) {
			continue;
		}
		candidates.insert(n);
		if prev_pruned.contains(&n) {
			to_release.push(tag);
		}
	}

	if !to_release.is_empty() {
		let released = midnight_node_ledger::gc::release_tagged_tips(&to_release);
		if released > 0 {
			info!(
				target: LOG_TARGET,
				"🧹 Released {released} tagged wrapper(s) from {} pruned height(s)",
				to_release.len()
			);
		}
	}

	run_arena_gc_slice();

	candidates
}

/// One incremental arena GC slice (zero-ref Transient/intermediate cull).
/// Paced on finality — no inner mutex-yield loop.
fn run_arena_gc_slice() {
	let started = std::time::Instant::now();
	match midnight_node_ledger::gc::collect_garbage(GC_SLICE) {
		Ok(0) => {},
		Ok(n) => {
			info!(
				target: LOG_TARGET,
				"🗑️  Arena GC slice: culled={n} elapsed_ms={}",
				started.elapsed().as_millis()
			);
		},
		Err(e) => {
			debug!(target: LOG_TARGET, "⏸️  Arena GC deferred: {e}");
		},
	}
}

async fn run(client: Arc<FullClient>, backend: Arc<FullBackend>, depth: u32) {
	info!(target: LOG_TARGET, "♻️  Ledger GC started (depth={depth})");

	let mut finality = client.finality_notification_stream();
	let lag: NumberFor<Block> = depth.saturated_into();
	let mut pruned_candidates: HashSet<u32> = HashSet::new();

	pruned_candidates = run_reclaim_slice(&client, &backend, lag, pruned_candidates).await;

	loop {
		let Some(_note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger GC");
			return;
		};
		while let Some(Some(_)) = finality.next().now_or_never() {}

		pruned_candidates = run_reclaim_slice(&client, &backend, lag, pruned_candidates).await;
	}
}

async fn run_reclaim_slice(
	client: &Arc<FullClient>,
	backend: &Arc<FullBackend>,
	lag: NumberFor<Block>,
	prev: HashSet<u32>,
) -> HashSet<u32> {
	let client_gc = client.clone();
	let backend_gc = backend.clone();
	match tokio::task::spawn_blocking(move || reclaim_slice(&client_gc, &backend_gc, lag, prev))
		.await
	{
		Ok(candidates) => candidates,
		Err(e) => {
			error!(target: LOG_TARGET, "💥 GC slice task failed: {e}");
			HashSet::new()
		},
	}
}

/// Arena-GC-only loop: never releases Anchored history wrappers, but still
/// culls zero-ref Transient/intermediate garbage that block execution already
/// unpersisted. Paced on finality.
async fn run_arena_only(client: Arc<FullClient>) {
	info!(
		target: LOG_TARGET,
		"♻️  Ledger arena GC started (tip reclaim disabled; history wrappers kept)"
	);

	let mut finality = client.finality_notification_stream();
	let _ = tokio::task::spawn_blocking(run_arena_gc_slice).await;

	loop {
		let Some(_note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger arena GC");
			return;
		};
		while let Some(Some(_)) = finality.next().now_or_never() {}

		if let Err(e) = tokio::task::spawn_blocking(run_arena_gc_slice).await {
			error!(target: LOG_TARGET, "💥 Arena GC slice task failed: {e}");
		}
	}
}

/// Spawn the ledger GC worker for the configured pruning mode.
pub fn try_spawn(
	spawn: &SpawnTaskHandle,
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	state_pruning: &Option<PruningMode>,
) {
	// `StateDb::open` reuses the DB-stored pruning mode when the CLI flag is
	// omitted (`(false, Some(stored), None) => stored`), so a `None` config
	// only means "constrained 256" on a fresh DB. If the flag is omitted and
	// the backend reports archive pruning, the stored mode is `ArchiveAll` or
	// `ArchiveCanonical` — indistinguishable through the public API. Fall
	// back to arena-only GC.
	if state_pruning.is_none() && backend.requires_full_sync() {
		info!(
			target: LOG_TARGET,
			"ℹ️  Archive DB with --state-pruning omitted; ledger tip reclaim disabled \
			 (arena GC only)"
		);
		spawn.spawn("ledger-gc", Some("ledger"), run_arena_only(client));
		return;
	}

	let Some(depth) = discovery_depth(state_pruning) else {
		info!(
			target: LOG_TARGET,
			"ℹ️  Ledger tip reclaim disabled (archive pruning); arena GC still runs for \
			 transient/intermediate garbage"
		);
		spawn.spawn("ledger-gc", Some("ledger"), run_arena_only(client));
		return;
	};

	spawn.spawn("ledger-gc", Some("ledger"), run(client, backend, depth));
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn discovery_depth_matches_effective_pruning() {
		assert_eq!(discovery_depth(&None), Some(256));
		assert_eq!(discovery_depth(&Some(PruningMode::default())), Some(256));
		assert_eq!(discovery_depth(&Some(PruningMode::blocks_pruning(1024))), Some(1024));
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveAll)), None);
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveCanonical)), None);
	}
}

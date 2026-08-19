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

//! Ledger GC worker: `release_tagged` wrappers whose block hash has left the
//! Substrate pruning window, then run incremental arena `gc`.
//!
//! Wrappers are staged at every executed import (`ledger_root_tag`, including
//! initial sync) and become durable on the next `on_finalize` flush; this
//! worker only decides *when* they may go. Under
//! `ArchiveAll`, tip reclaim is skipped (history wrappers stay) but the arena
//! GC loop still runs.

use std::{collections::HashSet, sync::Arc, time::Duration};

use futures::{FutureExt, StreamExt};
use log::{debug, error, info, warn};
use midnight_node_runtime::opaque::Block;
use sc_client_api::{Backend, BlockchainEvents};
use sc_client_db::PruningMode;
use sc_service::SpawnTaskHandle;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	SaturatedConversion,
	traits::{Block as BlockT, NumberFor},
};

use crate::service::{FullBackend, FullClient};

const LOG_TARGET: &str = "midnight::ledger_gc";
/// Soft time bound for one arena mark/sweep slice after finality. Storage-core
/// GC is incremental across calls, so backlog continues on later notifications
/// instead of looping under the arena mutex in a single tick.
const GC_SLICE: Duration = Duration::from_millis(25);

type BlockHash = <Block as BlockT>::Hash;

/// Unpersist lag = the effective `--state-pruning` window.
///
/// `None` means tip reclaim is off (`ArchiveAll` only) — Anchored history
/// wrappers must stay — but the arena GC loop still runs to cull zero-ref
/// transient/intermediate nodes unpersisted during block execution.
///
/// A `None` *config* does not mean archive: the CLI's `[default: 256]` is
/// help text only, so a node started without the flag reaches here with
/// `None` while sc-client-db prunes at `PruningMode::default()` (constrained,
/// 256). Map it through that same default — otherwise default-configured
/// nodes silently skip tip reclaim and `ledger_storage` re-grows without
/// bound (the bug this feature fixes). If the DB was created with a larger
/// window and the flag later dropped, `have_state_at` (checked past the lag)
/// still keeps wrappers live until actual pruning, so the mismatch is leak-only.
/// (`None` resolving to a stored *archive* mode on an existing DB is diverted
/// in `try_spawn` via the DB-effective `requires_full_sync()` before this
/// mapping is consulted.)
///
/// `ArchiveCanonical` is zero-lag: Substrate still drops non-canonical state,
/// so abandoned-fork wrappers would otherwise leak forever. Canonical history
/// wrappers are never released in that mode.
fn discovery_depth(state_pruning: &Option<PruningMode>) -> Option<u32> {
	match state_pruning.clone().unwrap_or_default() {
		PruningMode::Constrained(c) => Some(c.max_blocks.unwrap_or(0)),
		PruningMode::ArchiveCanonical => Some(0),
		PruningMode::ArchiveAll => None,
	}
}

fn hash_from_tag(tag: &[u8]) -> Option<BlockHash> {
	let raw = <[u8; 32]>::try_from(tag).ok()?;
	Some(BlockHash::from(raw))
}

/// Keep the wrapper while inside pruning lag or while state is still present.
/// Unknown header → reclaimable leftover. Transient `number` errors → keep.
fn state_still_live(
	client: &FullClient,
	backend: &FullBackend,
	hash: BlockHash,
	lag: NumberFor<Block>,
) -> bool {
	match client.number(hash) {
		Ok(Some(n)) => {
			let finalized = client.info().finalized_number;
			if finalized.saturating_sub(n) < lag {
				return true;
			}
			backend.have_state_at(hash, n)
		},
		Ok(None) => false,
		Err(e) => {
			debug!(target: LOG_TARGET, "⏸️  number({hash:?}) failed ({e}); keeping wrapper");
			true
		},
	}
}

/// Liveness for stale-fork wrappers under `ArchiveCanonical`.
///
/// `have_state_at` on archive backends is a state-root-*presence* probe, and
/// a stale fork can share its state root with a canonical sibling — the
/// canonical copy keeps that trie root present forever, so the probe would
/// never release the fork's wrapper. Use block liveness instead: a fork is
/// reclaimable once finality has passed its height and it is not the
/// canonical block there. Unknown header → displaced leftover → reclaimable.
/// Transient errors → keep (fail-safe). Canonical hashes always stay live.
fn stale_fork_still_live(client: &FullClient, hash: BlockHash) -> bool {
	match client.number(hash) {
		Ok(Some(n)) => {
			if n > client.info().finalized_number {
				return true;
			}
			match client.hash(n) {
				Ok(Some(canonical)) => canonical == hash,
				Ok(None) | Err(_) => true,
			}
		},
		Ok(None) => false,
		Err(e) => {
			debug!(target: LOG_TARGET, "⏸️  number({hash:?}) failed ({e}); keeping wrapper");
			true
		},
	}
}

fn reclaim_slice(
	client: &FullClient,
	backend: &FullBackend,
	lag: NumberFor<Block>,
	prev_pruned: HashSet<BlockHash>,
	archive_canonical: bool,
) -> HashSet<BlockHash> {
	let mut candidates = HashSet::new();
	let mut to_release = Vec::new();

	for tag in midnight_node_ledger::gc::tagged_root_tags() {
		let Some(hash) = hash_from_tag(&tag) else {
			continue;
		};
		let live = if archive_canonical {
			stale_fork_still_live(client, hash)
		} else {
			state_still_live(client, backend, hash, lag)
		};
		if live {
			continue;
		}
		candidates.insert(hash);
		if prev_pruned.contains(&hash) {
			to_release.push(tag);
		}
	}

	if !to_release.is_empty() {
		if !midnight_node_ledger::gc::ledger_quiescent() {
			debug!(
				target: LOG_TARGET,
				"⏸️  Ledger write cache busy; deferring reclaim of {} wrapper(s)",
				to_release.len()
			);
			return candidates;
		}

		match midnight_node_ledger::gc::release_tagged_tips(&to_release) {
			Ok(n) => {
				if n > 0 {
					info!(
						target: LOG_TARGET,
						"🧹 Released {n} tagged wrapper(s) from {} pruned block(s)",
						to_release.len()
					);
				}
			},
			Err(e) => {
				debug!(target: LOG_TARGET, "⏸️  Tagged-root release deferred: {e}");
				return candidates;
			},
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

async fn run(
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	depth: u32,
	archive_canonical: bool,
) {
	info!(
		target: LOG_TARGET,
		"♻️  Ledger GC started (depth={depth}, archive_canonical={archive_canonical})"
	);

	let mut finality = client.finality_notification_stream();
	let lag: NumberFor<Block> = depth.saturated_into();
	let mut pruned_candidates: HashSet<BlockHash> = HashSet::new();

	pruned_candidates =
		run_reclaim_slice(&client, &backend, lag, pruned_candidates, archive_canonical).await;

	loop {
		let Some(_note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger GC");
			return;
		};
		while let Some(Some(_)) = finality.next().now_or_never() {}

		pruned_candidates =
			run_reclaim_slice(&client, &backend, lag, pruned_candidates, archive_canonical).await;
	}
}

async fn run_reclaim_slice(
	client: &Arc<FullClient>,
	backend: &Arc<FullBackend>,
	lag: NumberFor<Block>,
	prev: HashSet<BlockHash>,
	archive_canonical: bool,
) -> HashSet<BlockHash> {
	let client_gc = client.clone();
	let backend_gc = backend.clone();
	match tokio::task::spawn_blocking(move || {
		reclaim_slice(&client_gc, &backend_gc, lag, prev, archive_canonical)
	})
	.await
	{
		Ok(candidates) => candidates,
		Err(e) => {
			error!(target: LOG_TARGET, "💥 GC slice task failed: {e}");
			HashSet::new()
		},
	}
}

/// Arena-GC-only loop for `ArchiveAll`: never releases Anchored history
/// wrappers, but still culls zero-ref Transient/intermediate garbage that
/// block execution already unpersisted. Paced on finality.
async fn run_arena_only(client: Arc<FullClient>) {
	info!(
		target: LOG_TARGET,
		"♻️  Ledger arena GC started (ArchiveAll; tip reclaim disabled, history wrappers kept)"
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
	// back to arena-only GC: correct for `ArchiveAll`, and for a stored
	// `ArchiveCanonical` it merely forgoes stale-fork reclaim (leak-only,
	// fork-rate bounded). Running the constrained path here instead would
	// release canonical wrappers that archive state keeps live forever.
	if state_pruning.is_none() && backend.requires_full_sync() {
		info!(
			target: LOG_TARGET,
			"ℹ️  Archive DB with --state-pruning omitted; ledger tip reclaim disabled \
			 (arena GC only). Pass --state-pruning archive-canonical explicitly to \
			 enable stale-fork reclaim"
		);
		spawn.spawn("ledger-gc", Some("ledger"), run_arena_only(client));
		return;
	}

	let archive_canonical =
		matches!(state_pruning.clone().unwrap_or_default(), PruningMode::ArchiveCanonical);
	let Some(depth) = discovery_depth(state_pruning) else {
		info!(
			target: LOG_TARGET,
			"ℹ️  Ledger tip reclaim disabled (ArchiveAll); arena GC still runs for \
			 transient/intermediate garbage"
		);
		spawn.spawn("ledger-gc", Some("ledger"), run_arena_only(client));
		return;
	};

	if archive_canonical {
		info!(
			target: LOG_TARGET,
			"ℹ️  Ledger GC enabled (ArchiveCanonical, zero-lag); reclaiming stale-fork \
			 wrappers once finality passes the fork height"
		);
	}

	spawn.spawn("ledger-gc", Some("ledger"), run(client, backend, depth, archive_canonical));
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
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveCanonical)), Some(0));
	}
}

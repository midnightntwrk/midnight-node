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

//! Ledger GC worker: index Anchored tips from Substrate `StateKey` at finality,
//! then `unpersist` them once Substrate state pruning drops the block; also
//! runs incremental arena `gc` to cull zero-ref transient/intermediate nodes.
//!
//! AuxStore holds `(block_hash → tip_bytes)`. Under `ArchiveAll`, tip reclaim
//! is skipped (history tips stay) but the arena GC loop still runs.

use std::{collections::HashSet, sync::Arc, time::Duration};

use futures::{FutureExt, StreamExt};
use log::{debug, error, info, warn};
use midnight_node_ledger::types::LedgerStateKey;
use midnight_node_runtime::{Runtime, opaque::Block};
use midnight_primitives_ledger::{LedgerGcDurability, LedgerGcIndex, LedgerGcState};
use parity_scale_codec::{DecodeAll, Encode};
use sc_client_api::{Backend, BlockchainEvents, StorageProvider, backend::AuxStore};
use sc_client_db::PruningMode;
use sc_service::SpawnTaskHandle;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_core::storage::StorageKey;
use sp_runtime::{
	SaturatedConversion,
	traits::{Block as BlockT, NumberFor},
};

use crate::service::{FullBackend, FullClient};

const LOG_TARGET: &str = "midnight::ledger_gc";
const GC_BUDGET: Duration = Duration::from_millis(250);
const GC_CHUNK: Duration = Duration::from_millis(25);

/// AuxStore key for SCALE [`LedgerGcState`].
const AUX_GC_KEY: &[u8] = b"midnight/ledger_gc";

type BlockHash = <Block as BlockT>::Hash;

/// AuxStore-backed durability for the GC index blob.
pub struct AuxGcStore {
	client: Arc<FullClient>,
}

impl AuxGcStore {
	pub fn new(client: Arc<FullClient>) -> Arc<Self> {
		Arc::new(Self { client })
	}

	fn read_blob(&self) -> Result<LedgerGcState, String> {
		match self.client.get_aux(AUX_GC_KEY) {
			Ok(Some(bytes)) => match LedgerGcState::decode_all(&mut &bytes[..]) {
				Ok(state) => Ok(state),
				Err(e) => {
					warn!(
						target: LOG_TARGET,
						"⚠️  Corrupt ledger-GC AuxStore blob ({e:?}); rebuilding from empty"
					);
					Ok(LedgerGcState::default())
				},
			},
			Ok(None) => Ok(LedgerGcState::default()),
			Err(e) => Err(format!("get_aux(ledger_gc) failed: {e}")),
		}
	}

	fn write_blob(&self, state: &LedgerGcState) -> Result<(), String> {
		let bytes = state.encode();
		self.client
			.insert_aux(&[(AUX_GC_KEY, bytes.as_slice())], &[])
			.map_err(|e| e.to_string())
	}
}

impl LedgerGcDurability for AuxGcStore {
	fn update(&self, f: &mut dyn FnMut(&mut LedgerGcState)) -> Result<(), String> {
		let mut state = self.read_blob()?;
		f(&mut state);
		self.write_blob(&state)
	}

	fn load(&self) -> Result<LedgerGcState, String> {
		self.read_blob()
	}
}

/// Unpersist lag = the effective `--state-pruning` window.
///
/// `None` means tip reclaim is off (`ArchiveAll` only) — Anchored history
/// tips must stay — but the arena GC loop still runs to cull zero-ref
/// transient/intermediate nodes unpersisted during block execution.
///
/// A `None` *config* does not mean archive: the CLI's `[default: 256]` is
/// help text only, so a node started without the flag reaches here with
/// `None` while sc-client-db prunes at `PruningMode::default()` (constrained,
/// 256). Map it through that same default — otherwise default-configured
/// nodes silently skip tip reclaim and `ledger_storage` re-grows without
/// bound (the bug this feature fixes). If the DB was created with a larger
/// window and the flag later dropped, `have_state_at` (checked past the lag)
/// still keeps tips live until actual pruning, so the mismatch is leak-only.
/// (`None` resolving to a stored *archive* mode on an existing DB is diverted
/// in `try_spawn` via the DB-effective `requires_full_sync()` before this
/// mapping is consulted.)
///
/// `ArchiveCanonical` is zero-lag: Substrate still drops non-canonical state,
/// so abandoned-fork `post_block_update` roots would otherwise leak forever.
/// With lag `0`, `state_still_live` relies on `have_state_at` alone — canonical
/// archive history stays live; stale forks become reclaimable once their
/// state is gone.
fn discovery_depth(state_pruning: &Option<PruningMode>) -> Option<u32> {
	match state_pruning.clone().unwrap_or_default() {
		PruningMode::Constrained(c) => Some(c.max_blocks.unwrap_or(0)),
		PruningMode::ArchiveCanonical => Some(0),
		PruningMode::ArchiveAll => None,
	}
}

fn midnight_state_key() -> StorageKey {
	StorageKey(pallet_midnight::StateKey::<Runtime>::hashed_key().to_vec())
}

fn hash_bytes(hash: &BlockHash) -> Vec<u8> {
	hash.as_ref().to_vec()
}

fn read_anchored_tip(client: &FullClient, hash: BlockHash) -> Option<Vec<u8>> {
	let data = client.storage(hash, &midnight_state_key()).ok().flatten()?;
	let tip = match LedgerStateKey::decode_all(&mut &data.0[..]) {
		Ok(LedgerStateKey::Anchored(bytes)) if !bytes.is_empty() => bytes,
		Ok(LedgerStateKey::Anchored(_)) | Ok(LedgerStateKey::Transient(_)) => return None,
		// Pre-v3 `StateKey` was a raw tip `Vec<u8>` (every post-block root was a
		// retained tip). Needed so full-sync across pre-migration history still
		// indexes those roots for reclaim after pruning.
		Err(_) => match Vec::<u8>::decode_all(&mut &data.0[..]) {
			Ok(bytes) if !bytes.is_empty() => bytes,
			_ => return None,
		},
	};
	// Only index tips this binary can reclaim (ledger-8/9). Pre-ledger-8
	// arena roots would otherwise enter AuxStore, get retired, then be
	// skipped at unpersist — a pointless binding with the same leak.
	if !midnight_node_ledger::gc::is_reclaimable_tip(&tip) {
		debug!(
			target: LOG_TARGET,
			"⏭️  Not indexing undecodable tip at {hash:?} ({} bytes)",
			tip.len()
		);
		return None;
	}
	Some(tip)
}

/// Record tips from newly finalized / abandoned-fork blocks (StateKey @ hash).
///
/// `bind_canonical` is false under `ArchiveCanonical`: canonical state is
/// never dropped there, so canonical bindings could never be reclaimed —
/// they would sit in the AuxStore blob forever, growing it by one entry per
/// block and re-writing the whole blob every finality. Only stale forks are
/// reclaimable in that mode. Fork capture is best-effort either way:
/// non-canonical state is discarded in the same commit that emits this
/// notification, so a tip unreadable by now leaks (bounded by fork rate).
fn on_finality(
	client: &FullClient,
	index: &LedgerGcIndex,
	note: &sc_client_api::FinalityNotification<Block>,
	bind_canonical: bool,
) {
	let mut binds = Vec::new();
	if bind_canonical {
		for hash in note.tree_route.iter().copied().chain(std::iter::once(note.hash)) {
			if let Some(tip) = read_anchored_tip(client, hash) {
				binds.push((hash_bytes(&hash), tip));
			}
		}
	}
	for stale in note.stale_blocks.iter() {
		if let Some(tip) = read_anchored_tip(client, stale.hash) {
			binds.push((hash_bytes(&stale.hash), tip));
		}
	}
	index.bind_finalized(&binds);
}

/// Keep tip while inside pruning lag or while state is still present.
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
			debug!(target: LOG_TARGET, "⏸️  number({hash:?}) failed ({e}); keeping tip");
			true
		},
	}
}

/// Liveness for stale-fork bindings under `ArchiveCanonical`.
///
/// `have_state_at` on archive backends is a state-root-*presence* probe, and
/// a stale fork can share its state root with a canonical sibling (an empty
/// or equivocation sibling at the same height) — the canonical copy keeps
/// that trie root present forever, so the probe would never release the
/// fork's binding even though the fork block's own ledger persist must be
/// reclaimed. Use block liveness instead: a fork is reclaimable once
/// finality has passed its height and it is not the canonical block there
/// (non-canonical state is discarded at canonicalization; a shared trie root
/// surviving via the sibling is irrelevant to the fork's own tip count).
/// Unknown header → displaced leftover → reclaimable. Transient errors →
/// keep (fail-safe).
fn stale_fork_still_live(client: &FullClient, hash: BlockHash) -> bool {
	match client.number(hash) {
		Ok(Some(n)) => {
			if n > client.info().finalized_number {
				// Finality has not passed this height — the fork is still
				// contestable and its state may still be live.
				return true;
			}
			// Defensive: stale bindings should never be canonical, but if one
			// is, keep it (its state is retained forever in this mode).
			match client.hash(n) {
				Ok(Some(canonical)) => canonical == hash,
				Ok(None) | Err(_) => true,
			}
		},
		Ok(None) => false,
		Err(e) => {
			debug!(target: LOG_TARGET, "⏸️  number({hash:?}) failed ({e}); keeping tip");
			true
		},
	}
}

fn reclaim_slice(
	client: &FullClient,
	backend: &FullBackend,
	index: &LedgerGcIndex,
	lag: NumberFor<Block>,
	prev_pruned: HashSet<BlockHash>,
	run_gc: bool,
	archive_canonical: bool,
) -> HashSet<BlockHash> {
	let snapshot = index.bound_snapshot();
	let mut candidates = HashSet::new();
	let mut to_remove = Vec::new();
	let mut tips_to_zero = Vec::new();

	for (hash_bytes, tip) in &snapshot {
		let Ok(raw) = <[u8; 32]>::try_from(hash_bytes.as_slice()) else {
			warn!(
				target: LOG_TARGET,
				"⚠️  Skipping tip with non-hash key ({} bytes)",
				hash_bytes.len()
			);
			continue;
		};
		let hash = BlockHash::from(raw);
		// ArchiveCanonical bindings are stale forks only, and `have_state_at`
		// is a root-presence probe there — use block liveness instead (see
		// `stale_fork_still_live`).
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
			// One unpersist per removed binding (each executed block persisted once).
			to_remove.push(hash_bytes.clone());
			tips_to_zero.push(tip.clone());
		}
	}

	if !to_remove.is_empty() {
		// At-most-once ordering: `unpersist_tips` is a blind, NON-idempotent
		// decrement — repeating it for the same tip (crash retry, failed
		// removal retry) would decrement the root twice and drive its count
		// negative (fail-deadly). So durably drop the binding FIRST and only
		// decrement once the removal is committed; any failure after that
		// point costs at most a leak. `unpersist_tips` flushes the ledger
		// write cache before Ok, so a shutdown before the next block flush
		// cannot drop staged root-count decrements while bindings are gone.
		// On unpersist failure, re-bind best-effort so the tips retry next
		// slice (re-bind failure = leak, still fail-safe).
		let removed_binds: Vec<(Vec<u8>, Vec<u8>)> =
			snapshot.iter().filter(|(h, _)| to_remove.contains(h)).cloned().collect();
		if !index.remove_bound(&to_remove) {
			return candidates;
		}
		match midnight_node_ledger::gc::unpersist_tips(&tips_to_zero) {
			Ok(n) => {
				info!(
					target: LOG_TARGET,
					"🧹 Unpersisted {n} arena tip root(s) from {} block binding(s)",
					to_remove.len()
				);
			},
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"⚠️  Tip unpersist deferred: {e}; re-binding {} block(s) for retry",
					removed_binds.len()
				);
				index.bind_finalized(&removed_binds);
				return candidates;
			},
		}
	}

	if run_gc {
		run_arena_gc_budget();
	} else {
		debug!(target: LOG_TARGET, "💤 Unpersist-only slice (tips={})", index.len());
	}

	candidates
}

/// Incremental arena GC for up to [`GC_BUDGET`], in [`GC_CHUNK`] slices.
/// Culled zero-ref nodes (including Transient intermediates already
/// unpersisted during block execution).
fn run_arena_gc_budget() {
	let started = std::time::Instant::now();
	let mut culled_total = 0usize;
	while started.elapsed() < GC_BUDGET {
		match midnight_node_ledger::gc::collect_garbage(GC_CHUNK) {
			Ok(0) => break,
			Ok(n) => culled_total += n,
			Err(e) => {
				debug!(target: LOG_TARGET, "⏸️  Arena GC deferred: {e}");
				break;
			},
		}
		std::thread::sleep(Duration::from_millis(2));
	}
	if culled_total > 0 {
		info!(
			target: LOG_TARGET,
			"🗑️  Culled {culled_total} arena nodes in {}ms",
			started.elapsed().as_millis()
		);
	}
}

async fn run<S>(
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	sync: Arc<S>,
	index: LedgerGcIndex,
	depth: u32,
	archive_canonical: bool,
) where
	S: SyncOracle + Send + Sync + 'static,
{
	let bind_canonical = !archive_canonical;
	info!(
		target: LOG_TARGET,
		"♻️  Ledger GC started (depth={depth}, archive_canonical={archive_canonical}, \
		 bind_canonical={bind_canonical})"
	);
	info!(target: LOG_TARGET, "📦 GC index (tips={})", index.len());

	let mut finality = client.finality_notification_stream();
	let lag: NumberFor<Block> = depth.saturated_into();
	let mut pruned_candidates: HashSet<BlockHash> = HashSet::new();

	// Reclaim any tips already past the pruning window before waiting on finality.
	pruned_candidates = run_reclaim_slice(
		&client,
		&backend,
		&index,
		lag,
		pruned_candidates,
		!sync.is_major_syncing(),
		archive_canonical,
	)
	.await;

	loop {
		let Some(note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger GC");
			return;
		};
		on_finality(&client, &index, &note, bind_canonical);
		while let Some(Some(note)) = finality.next().now_or_never() {
			on_finality(&client, &index, &note, bind_canonical);
		}

		let run_gc = if sync.is_major_syncing() {
			debug!(target: LOG_TARGET, "⏳ Major sync; unpersist-only slice");
			false
		} else {
			true
		};

		pruned_candidates = run_reclaim_slice(
			&client,
			&backend,
			&index,
			lag,
			pruned_candidates,
			run_gc,
			archive_canonical,
		)
		.await;
	}
}

async fn run_reclaim_slice(
	client: &Arc<FullClient>,
	backend: &Arc<FullBackend>,
	index: &LedgerGcIndex,
	lag: NumberFor<Block>,
	prev: HashSet<BlockHash>,
	run_gc: bool,
	archive_canonical: bool,
) -> HashSet<BlockHash> {
	let client_gc = client.clone();
	let backend_gc = backend.clone();
	let index_gc = index.clone();
	match tokio::task::spawn_blocking(move || {
		reclaim_slice(&client_gc, &backend_gc, &index_gc, lag, prev, run_gc, archive_canonical)
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

/// Arena-GC-only loop for `ArchiveAll`: never unpersists Anchored history
/// tips, but still culls zero-ref Transient/intermediate garbage that block
/// execution already unpersisted. Paced on finality (same as tip reclaim).
async fn run_arena_only<S>(client: Arc<FullClient>, sync: Arc<S>)
where
	S: SyncOracle + Send + Sync + 'static,
{
	info!(
		target: LOG_TARGET,
		"♻️  Ledger arena GC started (ArchiveAll; tip reclaim disabled, history tips kept)"
	);

	let mut finality = client.finality_notification_stream();
	// Catch up any backlog before waiting on the next notification.
	if !sync.is_major_syncing() {
		let _ = tokio::task::spawn_blocking(run_arena_gc_budget).await;
	}

	loop {
		let Some(_note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger arena GC");
			return;
		};
		while let Some(Some(_)) = finality.next().now_or_never() {}

		if sync.is_major_syncing() {
			debug!(target: LOG_TARGET, "⏳ Major sync; skipping arena GC slice");
			continue;
		}

		if let Err(e) = tokio::task::spawn_blocking(run_arena_gc_budget).await {
			error!(target: LOG_TARGET, "💥 Arena GC slice task failed: {e}");
		}
	}
}

/// Spawn the ledger GC worker for the configured pruning mode.
pub fn try_spawn<S>(
	spawn: &SpawnTaskHandle,
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	sync: Arc<S>,
	index: LedgerGcIndex,
	state_pruning: &Option<PruningMode>,
) where
	S: SyncOracle + Send + Sync + 'static,
{
	// `StateDb::open` reuses the DB-stored pruning mode when the CLI flag is
	// omitted (`(false, Some(stored), None) => stored`), so a `None` config
	// only means "constrained 256" on a fresh DB. If the flag is omitted and
	// the backend reports archive pruning, the stored mode is `ArchiveAll` or
	// `ArchiveCanonical` — indistinguishable through the public API (upstream
	// ask: expose the effective mode). Fall back to arena-only GC: correct
	// for `ArchiveAll`, and for a stored `ArchiveCanonical` it merely forgoes
	// stale-fork reclaim (leak-only, fork-rate bounded). Running the
	// constrained path here instead would bind canonical tips that archive
	// state keeps live forever, growing the AuxStore blob by one immortal
	// entry per finalized block. With the flag set explicitly, `StateDb::open`
	// errors on a variant mismatch, so the config variant below is
	// authoritative for the DB.
	if state_pruning.is_none() && backend.requires_full_sync() {
		info!(
			target: LOG_TARGET,
			"ℹ️  Archive DB with --state-pruning omitted; ledger tip reclaim disabled \
			 (arena GC only). Pass --state-pruning archive-canonical explicitly to \
			 enable stale-fork reclaim"
		);
		spawn.spawn("ledger-gc", Some("ledger"), run_arena_only(client, sync));
		return;
	}

	// Derive the mode explicitly: `depth == 0` alone cannot distinguish
	// `ArchiveCanonical` from `Constrained(max_blocks: None)` (which prunes
	// canonical state immediately and so must bind canonical blocks and get
	// the sync warning below).
	let archive_canonical =
		matches!(state_pruning.clone().unwrap_or_default(), PruningMode::ArchiveCanonical);
	let Some(depth) = discovery_depth(state_pruning) else {
		// ArchiveAll: keep every Anchored tip, but still run arena GC so
		// zero-ref Transient intermediates do not accumulate forever.
		info!(
			target: LOG_TARGET,
			"ℹ️  Ledger tip reclaim disabled (ArchiveAll); arena GC still runs for \
			 transient/intermediate garbage"
		);
		spawn.spawn("ledger-gc", Some("ledger"), run_arena_only(client, sync));
		return;
	};

	if archive_canonical {
		info!(
			target: LOG_TARGET,
			"ℹ️  Ledger GC enabled (ArchiveCanonical, zero-lag); reclaiming stale-fork \
			 tips once Substrate drops non-canonical state"
		);
	}

	// Tips are captured by reading `StateKey` at finality. During full sync,
	// GRANDPA finalizes in justification-period batches and state pruning runs
	// in the same commit — blocks more than `depth` behind each batch head are
	// already unreadable when the notification arrives, so their tips can never
	// be captured and leak permanently. A window >= the justification period
	// closes that gap. ArchiveCanonical keeps canonical state forever, so the
	// constrained-window gap does not apply there.
	if !archive_canonical && depth < crate::service::GRANDPA_JUSTIFICATION_PERIOD {
		warn!(
			target: LOG_TARGET,
			"⚠️  --state-pruning {depth} is smaller than the GRANDPA justification period \
			 ({}); a full sync from genesis will permanently leak ledger tips for blocks \
			 pruned before their finality notification. Use --state-pruning >= {} for \
			 full-sync nodes",
			crate::service::GRANDPA_JUSTIFICATION_PERIOD,
			crate::service::GRANDPA_JUSTIFICATION_PERIOD,
		);
	}

	spawn.spawn(
		"ledger-gc",
		Some("ledger"),
		run(client, backend, sync, index, depth, archive_canonical),
	);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn discovery_depth_matches_effective_pruning() {
		// Flag omitted: sc-client-db prunes at the default (constrained 256) —
		// the worker must run, not silently disable on default configs.
		assert_eq!(discovery_depth(&None), Some(256));
		assert_eq!(discovery_depth(&Some(PruningMode::default())), Some(256));
		assert_eq!(discovery_depth(&Some(PruningMode::blocks_pruning(1024))), Some(1024));
		// ArchiveAll: tip reclaim off (`None`); arena-GC-only worker still spawns.
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveAll)), None);
		// ArchiveCanonical still drops non-canonical state — zero-lag worker.
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveCanonical)), Some(0));
	}
}

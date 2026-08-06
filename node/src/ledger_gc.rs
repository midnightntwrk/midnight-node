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
//! then `unpersist` them once Substrate state pruning drops the block.
//!
//! AuxStore holds `(block_hash → tip_bytes)`.

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

/// Unpersist lag = the effective `--state-pruning` window; `None` disables
/// the worker (archive modes only).
///
/// A `None` *config* does not mean archive: the CLI's `[default: 256]` is
/// help text only, so a node started without the flag reaches here with
/// `None` while sc-client-db prunes at `PruningMode::default()` (constrained,
/// 256). Map it through that same default — otherwise default-configured
/// nodes silently skip the worker and `ledger_storage` re-grows without
/// bound (the bug this feature fixes). If the DB was created with a larger
/// window and the flag later dropped, `have_state_at` (checked past the lag)
/// still keeps tips live until actual pruning, so the mismatch is leak-only.
fn discovery_depth(state_pruning: &Option<PruningMode>) -> Option<u32> {
	match state_pruning.clone().unwrap_or_default() {
		PruningMode::Constrained(c) => Some(c.max_blocks.unwrap_or(0)),
		PruningMode::ArchiveAll | PruningMode::ArchiveCanonical => None,
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
	match LedgerStateKey::decode_all(&mut &data.0[..]) {
		Ok(LedgerStateKey::Anchored(bytes)) if !bytes.is_empty() => Some(bytes),
		_ => None,
	}
}

/// Record tips from newly finalized / abandoned-fork blocks (StateKey @ hash).
fn on_finality(
	client: &FullClient,
	index: &LedgerGcIndex,
	note: &sc_client_api::FinalityNotification<Block>,
) {
	let mut binds = Vec::new();
	for hash in note.tree_route.iter().copied().chain(std::iter::once(note.hash)) {
		if let Some(tip) = read_anchored_tip(client, hash) {
			binds.push((hash_bytes(&hash), tip));
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

fn reclaim_slice(
	client: &FullClient,
	backend: &FullBackend,
	index: &LedgerGcIndex,
	lag: NumberFor<Block>,
	prev_pruned: HashSet<BlockHash>,
	run_gc: bool,
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
		if state_still_live(client, backend, hash, lag) {
			continue;
		}
		candidates.insert(hash);
		if prev_pruned.contains(&hash) {
			to_remove.push(hash_bytes.clone());
			if !tips_to_zero.iter().any(|t| t == tip) {
				tips_to_zero.push(tip.clone());
			}
		}
	}

	if !to_remove.is_empty() {
		// At-most-once ordering: `unpersist_tips` is a blind, NON-idempotent
		// decrement — repeating it for the same tip (crash retry, failed
		// removal retry) would decrement the root twice and drive its count
		// negative (fail-deadly). So durably drop the binding FIRST and only
		// decrement once the removal is committed; any failure after that
		// point costs at most a leak. On unpersist failure, re-bind
		// best-effort so the tips retry next slice (re-bind failure = leak,
		// still fail-safe).
		let removed_binds: Vec<(Vec<u8>, Vec<u8>)> =
			snapshot.iter().filter(|(h, _)| to_remove.contains(h)).cloned().collect();
		if !index.remove_bound(&to_remove) {
			return candidates;
		}
		match midnight_node_ledger::gc::unpersist_tips(&tips_to_zero) {
			Ok(()) => {
				info!(
					target: LOG_TARGET,
					"🧹 Unpersisted {} arena tip(s) ({} block(s))",
					tips_to_zero.len(),
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
	} else {
		debug!(target: LOG_TARGET, "💤 Unpersist-only slice (tips={})", index.len());
	}

	candidates
}

async fn run<S>(
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	sync: Arc<S>,
	index: LedgerGcIndex,
	depth: u32,
) where
	S: SyncOracle + Send + Sync + 'static,
{
	info!(
		target: LOG_TARGET,
		"♻️  Ledger GC started (depth={depth})"
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
	)
	.await;

	loop {
		let Some(note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger GC");
			return;
		};
		on_finality(&client, &index, &note);
		while let Some(Some(note)) = finality.next().now_or_never() {
			on_finality(&client, &index, &note);
		}

		let run_gc = if sync.is_major_syncing() {
			debug!(target: LOG_TARGET, "⏳ Major sync; unpersist-only slice");
			false
		} else {
			true
		};

		pruned_candidates =
			run_reclaim_slice(&client, &backend, &index, lag, pruned_candidates, run_gc).await;
	}
}

async fn run_reclaim_slice(
	client: &Arc<FullClient>,
	backend: &Arc<FullBackend>,
	index: &LedgerGcIndex,
	lag: NumberFor<Block>,
	prev: HashSet<BlockHash>,
	run_gc: bool,
) -> HashSet<BlockHash> {
	let client_gc = client.clone();
	let backend_gc = backend.clone();
	let index_gc = index.clone();
	match tokio::task::spawn_blocking(move || {
		reclaim_slice(&client_gc, &backend_gc, &index_gc, lag, prev, run_gc)
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

/// Spawn the GC worker when state pruning is constrained.
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
	let Some(depth) = discovery_depth(state_pruning) else {
		info!(
			target: LOG_TARGET,
			"ℹ️  Ledger GC disabled (archive pruning); tips retain full history"
		);
		return;
	};

	// Tips are captured by reading `StateKey` at finality. During full sync,
	// GRANDPA finalizes in justification-period batches and state pruning runs
	// in the same commit — blocks more than `depth` behind each batch head are
	// already unreadable when the notification arrives, so their tips can never
	// be captured and leak permanently. A window >= the justification period
	// closes that gap (warp sync is unaffected: skipped history is never
	// executed, so nothing is persisted).
	if depth < crate::service::GRANDPA_JUSTIFICATION_PERIOD {
		warn!(
			target: LOG_TARGET,
			"⚠️  --state-pruning {depth} is smaller than the GRANDPA justification period \
			 ({}); a full sync from genesis will permanently leak ledger tips for blocks \
			 pruned before their finality notification. Use --state-pruning >= {} (or warp \
			 sync) for full-sync nodes",
			crate::service::GRANDPA_JUSTIFICATION_PERIOD,
			crate::service::GRANDPA_JUSTIFICATION_PERIOD,
		);
	}

	spawn.spawn("ledger-gc", Some("ledger"), run(client, backend, sync, index, depth));
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
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveAll)), None);
		assert_eq!(discovery_depth(&Some(PruningMode::ArchiveCanonical)), None);
	}
}

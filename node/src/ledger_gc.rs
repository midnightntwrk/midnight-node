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

//! Background worker that keeps `ledger_storage` aligned with Substrate state pruning.
//!
//! Every ledger host call that mutates state persists a new arena root. Without a
//! matching cleanup, those roots (and their exclusively-owned nodes) accumulate for
//! every block forever — see
//! <https://github.com/midnightntwrk/midnight-node/issues/1983>.
//!
//! This worker periodically:
//! 1. Collects the `Midnight::StateKey` values for every block whose Substrate
//!    state is still retained (canonical pruning window + non-finalized forks).
//! 2. Runs one time-budgeted slice of
//!    [`midnight_node_ledger::gc::collect_garbage`], which treats that set as the
//!    complete live root set and deletes everything else.
//!
//! Archive nodes (`--state-pruning archive` / `archive-canonical`) skip the worker:
//! reconstructing the full historical root set would require reading every block
//! since genesis, and archive operators intentionally keep that history.

use std::{collections::HashSet, sync::Arc, time::Duration};

use midnight_node_runtime::opaque::Block;
use parity_scale_codec::Decode;
use sc_client_api::{Backend as _, StorageProvider};
use sc_client_db::PruningMode;
use sc_service::SpawnTaskHandle;
use sp_blockchain::{Backend as BlockchainBackend, HeaderBackend};
use sp_core::storage::StorageKey;
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor, One};

use crate::service::{FullBackend, FullClient};

const LOG_TARGET: &str = "midnight::ledger_gc";

/// How often to run a GC slice. Kept modest so disk reclamation progresses on
/// large historical databases without monopolising the backend lock.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

/// Soft time budget per GC slice. The collector is incremental and resumes on
/// the next tick; the backend lock is held for roughly this long.
const DEFAULT_BUDGET: Duration = Duration::from_millis(250);

/// Storage key for `Midnight::StateKey` (`twox128("Midnight") ++ twox128("StateKey")`).
fn midnight_state_key() -> StorageKey {
	StorageKey([twox_128(b"Midnight"), twox_128(b"StateKey")].concat())
}

/// Derive the ledger-GC retention window from Substrate's state-pruning config.
///
/// Returns `None` for archive modes (worker should not run).
fn retention_window(state_pruning: &Option<PruningMode>) -> Option<u32> {
	match state_pruning {
		Some(PruningMode::ArchiveAll) | Some(PruningMode::ArchiveCanonical) => None,
		Some(PruningMode::Constrained(constraints)) => Some(constraints.max_blocks.unwrap_or(256)),
		// Substrate default when the flag is omitted.
		None => Some(256),
	}
}

/// Collect serialized `Midnight::StateKey` values for every block whose state we
/// still expect Substrate to retain.
fn collect_live_state_keys(
	client: &FullClient,
	backend: &FullBackend,
	window: u32,
) -> Vec<Vec<u8>> {
	let info = client.info();
	let best = info.best_number;
	let finalized = info.finalized_number;
	let storage_key = midnight_state_key();

	let mut hashes: HashSet<<Block as BlockT>::Hash> = HashSet::new();

	// Canonical chain covering the pruning window (and the non-finalized tip).
	let window_start = finalized.saturating_sub(window);
	let mut number: NumberFor<Block> = window_start;
	while number <= best {
		if let Ok(Some(hash)) = client.hash(number) {
			hashes.insert(hash);
		}
		if number == best {
			break;
		}
		number += <NumberFor<Block> as One>::one();
	}

	// Non-finalized forks: walk every leaf back to (and including) the finalized
	// block so a reorg within the unfinalized window cannot see its roots culled.
	if let Ok(leaves) = backend.blockchain().leaves() {
		for mut hash in leaves {
			loop {
				if !hashes.insert(hash) {
					break;
				}
				let Ok(Some(header)) = client.header(hash) else {
					break;
				};
				if *header.number() <= finalized {
					break;
				}
				hash = *header.parent_hash();
				if hash == Default::default() {
					break;
				}
			}
		}
	}

	let mut keys = Vec::with_capacity(hashes.len());
	let mut missing = 0u32;
	for hash in hashes {
		match client.storage(hash, &storage_key) {
			Ok(Some(data)) => match Vec::<u8>::decode(&mut &data.0[..]) {
				Ok(state_key) if !state_key.is_empty() => keys.push(state_key),
				Ok(_) => {
					log::debug!(
						target: LOG_TARGET,
						"Empty StateKey at {:?}; skipping",
						hash
					);
				},
				Err(e) => {
					log::warn!(
						target: LOG_TARGET,
						"Failed to SCALE-decode StateKey at {:?}: {e:?}",
						hash
					);
					missing += 1;
				},
			},
			Ok(None) => {
				// State already pruned for this hash — expected at the trailing
				// edge of the window / for ancient leaves.
				missing += 1;
			},
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"State unavailable at {:?}: {e:?}",
					hash
				);
				missing += 1;
			},
		}
	}

	log::debug!(
		target: LOG_TARGET,
		"Collected {} live StateKeys (window={window}, best={}, finalized={}, unavailable={missing})",
		keys.len(),
		best,
		finalized,
	);
	keys
}

async fn run(client: Arc<FullClient>, backend: Arc<FullBackend>, window: u32) {
	log::info!(
		target: LOG_TARGET,
		"Ledger GC worker started (window={window} blocks, interval={DEFAULT_INTERVAL:?}, budget={DEFAULT_BUDGET:?})"
	);

	loop {
		tokio::time::sleep(DEFAULT_INTERVAL).await;

		let client = client.clone();
		let backend = backend.clone();
		let result = tokio::task::spawn_blocking(move || {
			let keys = collect_live_state_keys(&client, &backend, window);
			if keys.is_empty() {
				// An empty root set would unpersist everything. Skip until the
				// chain has produced at least one readable StateKey.
				log::debug!(target: LOG_TARGET, "No live StateKeys yet; skipping GC slice");
				return Ok(None);
			}
			midnight_node_ledger::gc::collect_garbage(DEFAULT_BUDGET, &keys).map(Some)
		})
		.await;

		match result {
			Ok(Ok(Some(outcome))) => {
				if outcome.culled > 0 {
					log::info!(
						target: LOG_TARGET,
						"GC slice culled {} arena nodes (live_roots={})",
						outcome.culled,
						outcome.live_roots,
					);
				} else {
					log::trace!(
						target: LOG_TARGET,
						"GC slice made no deletions (live_roots={})",
						outcome.live_roots,
					);
				}
			},
			Ok(Ok(None)) => {},
			Ok(Err(midnight_node_ledger::gc::GcError::UnrecognizedStateKey)) => {
				// Expected while syncing through the pre-ledger-8 era: those
				// state keys carry older tags, and the legacy ledger-7 storage
				// crate has no GC support. Retried on the next tick; the
				// window clears once sync passes the ledger-8 hardfork.
				log::debug!(
					target: LOG_TARGET,
					"GC slice deferred: live window contains pre-ledger-8 state keys"
				);
			},
			Ok(Err(e)) => {
				log::warn!(target: LOG_TARGET, "GC slice aborted: {e}");
			},
			Err(e) => {
				log::error!(target: LOG_TARGET, "GC worker task failed: {e}");
			},
		}
	}
}

/// Spawn the ledger-storage GC worker, or no-op on archive nodes.
pub fn try_spawn(
	state_pruning: &Option<PruningMode>,
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	spawn_handle: &SpawnTaskHandle,
) {
	let Some(window) = retention_window(state_pruning) else {
		log::info!(
			target: LOG_TARGET,
			"Ledger GC worker disabled (state pruning is archive); ledger_storage retains full history"
		);
		return;
	};

	spawn_handle.spawn("ledger-storage-gc", None, run(client, backend, window));
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn retention_window_skips_archive() {
		assert_eq!(retention_window(&Some(PruningMode::ArchiveAll)), None);
		assert_eq!(retention_window(&Some(PruningMode::ArchiveCanonical)), None);
	}

	#[test]
	fn retention_window_reads_constrained() {
		assert_eq!(retention_window(&Some(PruningMode::blocks_pruning(128))), Some(128));
		assert_eq!(retention_window(&None), Some(256));
	}

	#[test]
	fn state_key_storage_key_is_stable() {
		// Must match toolkit / metadata: twox128("Midnight") ++ twox128("StateKey").
		assert_eq!(midnight_state_key().0, [twox_128(b"Midnight"), twox_128(b"StateKey")].concat());
	}
}

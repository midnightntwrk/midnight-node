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

//! Node side of snapshot ledger storage (`ledger_storage_mode = "snapshot"`): when to open,
//! replay, evict and snapshot. The mechanism is `midnight_node_ledger::snapshot`; see
//! `docs/ledger-snapshot-mode.md`.

pub(crate) mod chain;
mod driver;

use crate::{
	cfg::midnight_cfg::{LedgerStorageMode, MidnightCfg},
	service::StorageInit,
};
use chain::Chain;
use midnight_node_ledger::snapshot::{self, Marker, SnapshotRequest};
use sc_service::{BlocksPruning, Configuration, PruningMode};
use sp_core::H256;
use std::{
	path::{Path, PathBuf},
	time::Duration,
};

pub(crate) const LOG_TARGET: &str = "midnight::ledger-snapshot";

/// Blocks of Substrate state kept beyond twice the block cap: finality arrives in jumps of up
/// to 512 blocks during sync, and blocks keep being finalized while a snapshot is written.
const PRUNING_MARGIN: u32 = 4096;

#[derive(Debug, Clone)]
pub struct SnapshotSettings {
	root: PathBuf,
	interval: Duration,
	block_cap: u32,
	query_window: u32,
}

impl SnapshotSettings {
	pub fn from_cfg(cfg: &MidnightCfg, base_path: &Path) -> Option<Self> {
		(cfg.ledger_storage_mode == LedgerStorageMode::Snapshot).then(|| Self {
			root: base_path.join("ledger_storage_snapshot"),
			interval: Duration::from_secs(cfg.ledger_snapshot_interval_secs),
			block_cap: cfg.ledger_snapshot_block_cap,
			query_window: cfg.ledger_query_window,
		})
	}

	/// A restart replays every block above the snapshot block on its parent's Substrate state,
	/// so state and bodies must reach back that far: up to the block cap while a snapshot is
	/// not yet due, twice that if one falls due while the previous is still being written.
	/// Returns the number of blocks whose state is kept.
	fn check_pruning(
		&self,
		state: Option<&PruningMode>,
		blocks: &BlocksPruning,
	) -> Result<u32, String> {
		let min = self.block_cap.saturating_mul(2).saturating_add(PRUNING_MARGIN);
		let state_blocks = match state {
			Some(PruningMode::Constrained(c)) => c.max_blocks.filter(|n| *n >= min),
			_ => None,
		};
		let Some(state_blocks) = state_blocks else {
			return Err(format!(
				"ledger_storage_mode = \"snapshot\" needs --state-pruning <N> with N >= {min} \
				 (2 * ledger_snapshot_block_cap + {PRUNING_MARGIN}), got {state:?}"
			));
		};
		if let BlocksPruning::Some(n) = blocks
			&& *n < min
		{
			return Err(format!("--blocks-pruning must keep at least {min} blocks, got {n}"));
		}
		Ok(state_blocks)
	}
}

/// Before the chain is opened: check the pruning settings and open or create the snapshot.
/// Returns the blocks of Substrate state kept, and the marker: `None` if a persistent-layout
/// database waits for conversion in [`start`].
pub(crate) fn init(
	settings: &SnapshotSettings,
	config: &Configuration,
	storage: &StorageInit,
) -> Result<(u32, Option<Marker>), String> {
	let state_pruning =
		settings.check_pruning(config.state_pruning.as_ref(), &config.blocks_pruning)?;
	let marker = snapshot::init(
		&settings.root,
		&storage.genesis_state,
		Some(&storage.db_path),
		storage.cache_size,
	)?;
	Ok((state_pruning, marker))
}

/// Once the chain is readable: convert a persistent-layout database if that is pending, check the
/// snapshot against the chain, and re-execute every block above it. Returns the driver that then
/// evicts and snapshots.
pub(crate) fn start<C: Chain>(
	settings: &SnapshotSettings,
	state_pruning: u32,
	marker: Option<Marker>,
	legacy: &Path,
	chain: C,
	registry: Option<&prometheus_endpoint::Registry>,
) -> Result<driver::Driver<C>, String> {
	let base = startup(&chain, marker, legacy)?;
	let mut driver = driver::Driver::new(chain, settings.clone(), state_pruning, registry)?;
	if let Some((number, hash)) = base {
		driver.replay(number, hash)?;
	}
	Ok(driver)
}

/// Convert a persistent-layout database if that is pending, then check the snapshot against the
/// chain's `StateKey` at its block. Returns the block to replay from: none on a warp-synced node,
/// which has no Substrate state at genesis and recovers the ledger by warp sync.
fn startup(
	chain: &impl Chain,
	marker: Option<Marker>,
	legacy: &Path,
) -> Result<Option<(u64, H256)>, String> {
	let marker = match marker {
		Some(marker) => marker,
		None => {
			let (number, hash) = chain.finalized();
			let request = SnapshotRequest {
				block_number: number,
				block_hash: Some(hash.as_bytes().to_vec()),
				state_key: chain.state_key(hash)?.ok_or("no StateKey at the finalized block")?,
			};
			snapshot::convert_legacy(legacy, &request)?
		},
	};
	let hash = match marker.block_hash_bytes()? {
		Some(bytes) => H256::from_slice(&bytes),
		None => chain.genesis(),
	};
	if chain.header(hash)?.is_none_or(|(number, _)| number != marker.block_number) {
		return Err(format!(
			"snapshot block #{} {hash:?} is not in the chain. If the chain database was purged, \
			 remove the ledger_storage_snapshot directory too",
			marker.block_number
		));
	}
	let on_chain = match chain.state_key(hash)? {
		Some(key) => key,
		None if marker.block_hash.is_none() => {
			log::warn!(target: LOG_TARGET, "No Substrate state at genesis: warp sync recovers the ledger");
			return Ok(None);
		},
		None => {
			return Err(format!(
				"the Substrate state of snapshot block #{} {hash:?} is pruned, so the ledger cannot \
				 be rebuilt from the snapshot (did snapshots fail for longer than --state-pruning \
				 allows?). Remove the ledger_storage_snapshot directory and the chain database \
				 (purge-chain), then sync again",
				marker.block_number
			));
		},
	};
	if on_chain != marker.state_key_bytes()? {
		return Err(format!(
			"snapshot {} does not match StateKey {} at #{}",
			marker.dir,
			hex::encode(&on_chain),
			marker.block_number
		));
	}
	Ok(Some((marker.block_number, hash)))
}

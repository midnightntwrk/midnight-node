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

//! Replay on startup, then once a second: evict the window and take snapshots when due.
//!
//! Finality is polled rather than followed, as Substrate sends no finality notifications for
//! blocks finalized during initial sync.

use super::{LOG_TARGET, SnapshotSettings, chain::Chain};
use midnight_node_ledger::snapshot::{self, Marker, SnapshotRequest, window};
use prometheus_endpoint::{Counter, Gauge, Registry, U64, register};
use sp_core::H256;
use std::{
	collections::{BTreeSet, HashMap, HashSet},
	thread::JoinHandle,
	time::{Duration, Instant},
};

const RETRY_AFTER: Duration = Duration::from_secs(60);
const REPLAY_EVICT_EVERY: u64 = 1000;

struct Metrics {
	window_states: Gauge<U64>,
	snapshots: Counter<U64>,
	snapshot_block: Gauge<U64>,
}

pub(crate) struct Driver<C> {
	chain: C,
	settings: SnapshotSettings,
	/// Blocks whose Substrate state the node keeps (`--state-pruning`).
	state_pruning: u32,
	metrics: Option<Metrics>,
	keys: HashMap<H256, (u64, Vec<u8>)>,
	last_snapshot: Instant,
	snapshot: Option<JoinHandle<Result<Marker, String>>>,
	failed_at: Option<Instant>,
}

impl<C: Chain> Driver<C> {
	pub(super) fn new(
		chain: C,
		settings: SnapshotSettings,
		state_pruning: u32,
		registry: Option<&Registry>,
	) -> Result<Self, String> {
		let metrics = registry
			.map(|r| {
				Ok::<_, prometheus_endpoint::PrometheusError>(Metrics {
					window_states: register(
						Gauge::new(
							"midnight_ledger_window_states",
							"Ledger states held in memory",
						)?,
						r,
					)?,
					snapshots: register(
						Counter::new("midnight_ledger_snapshots_total", "Ledger snapshots taken")?,
						r,
					)?,
					snapshot_block: register(
						Gauge::new(
							"midnight_ledger_snapshot_block",
							"Block of the ledger snapshot on disk, which a restart replays from",
						)?,
						r,
					)?,
				})
			})
			.transpose()
			.map_err(|e| format!("ledger snapshot metrics: {e}"))?;
		Ok(Self {
			chain,
			settings,
			state_pruning,
			metrics,
			keys: HashMap::new(),
			last_snapshot: Instant::now(),
			snapshot: None,
			failed_at: None,
		})
	}

	pub(crate) async fn run(mut self) {
		loop {
			self.pass();
			tokio::time::sleep(Duration::from_secs(1)).await;
		}
	}

	/// A block's `StateKey`; `None` if the node has no Substrate state of the block.
	fn state_key(&mut self, hash: H256, number: u64) -> Result<Option<Vec<u8>>, String> {
		if let Some((_, key)) = self.keys.get(&hash) {
			return Ok(Some(key.clone()));
		}
		let key = self.chain.state_key(hash)?;
		if let Some(key) = &key {
			self.keys.insert(hash, (number, key.clone()));
		}
		Ok(key)
	}

	fn required_state_key(&mut self, hash: H256, number: u64) -> Result<Vec<u8>, String> {
		self.state_key(hash, number)?.ok_or_else(|| format!("no StateKey at {hash:?}"))
	}

	/// Re-execute every block above the snapshot block, on every live chain, checking that each
	/// puts its own `StateKey` into the window. Returns the number executed.
	pub(super) fn replay(&mut self, base_number: u64, base_hash: H256) -> Result<u64, String> {
		let started = Instant::now();
		let chain = &self.chain;
		let blocks = descendants(&chain.leaves()?, base_number, base_hash, |h| chain.header(h))?;
		log::info!(target: LOG_TARGET, "Replaying {} blocks above #{base_number}", blocks.len());
		let mut executed = 0;
		for (number, hash) in blocks {
			let key = self.required_state_key(hash, number)?;
			self.chain.execute(hash)?;
			if !window::contains(&key) {
				return Err(format!(
					"re-executing #{number} {hash:?} did not rebuild its StateKey"
				));
			}
			executed += 1;
			if executed % REPLAY_EVICT_EVERY == 0
				&& let Err(e) = self.evict()
			{
				log::warn!(target: LOG_TARGET, "Window not evicted: {e}");
			}
		}
		log::info!(target: LOG_TARGET, "Replayed {executed} blocks in {:?}", started.elapsed());
		Ok(executed)
	}

	/// Drop window states outside the query window and off live chains. Finalized blocks below
	/// the oldest one with Substrate state have no `StateKey` to keep, and no state anything can
	/// query: a warp-synced node has neither below its warp target. Any other block without a
	/// `StateKey` stops the eviction, as its state cannot be told.
	fn evict(&mut self) -> Result<usize, String> {
		let (finalized_number, finalized) = self.chain.finalized();
		let chain = &self.chain;
		let header = |h: H256| chain.header(h);
		let finalized_chain = ancestors(finalized, self.settings.query_window, header)?;
		let live = descendants(&chain.leaves()?, finalized_number, finalized, header)?;
		let mut keep = HashSet::new();
		let mut stateless = None;
		for (number, hash) in finalized_chain {
			match (self.state_key(hash, number)?, stateless) {
				(Some(key), None) => {
					keep.insert(key);
				},
				(Some(_), Some(above)) => return Err(format!("no StateKey at {above:?}")),
				(None, _) => stateless = stateless.or(Some(hash)),
			}
		}
		for (number, hash) in live {
			keep.insert(self.required_state_key(hash, number)?);
		}
		let horizon = finalized_number.saturating_sub(u64::from(self.settings.query_window));
		self.keys.retain(|_, (number, _)| *number >= horizon);
		Ok(window::evict(&keep))
	}

	fn pass(&mut self) {
		if self.snapshot.as_ref().is_some_and(|h| h.is_finished()) {
			match self.snapshot.take().map(JoinHandle::join) {
				Some(Ok(Ok(_))) => {
					self.last_snapshot = Instant::now();
					if let Some(m) = &self.metrics {
						m.snapshots.inc();
					}
				},
				outcome => self.failed(&format!("{outcome:?}")),
			}
		}
		if let Err(e) = self.evict() {
			log::warn!(target: LOG_TARGET, "Window not evicted: {e}");
		}
		if let Err(e) = self.maybe_snapshot() {
			self.failed(&e);
		}
		if let Some(m) = &self.metrics {
			m.window_states.set(window::len() as u64);
			m.snapshot_block.set(snapshot::current_marker().map_or(0, |c| c.block_number));
		}
	}

	/// A restart replays from the snapshot block on its Substrate state: say how long that lasts.
	fn failed(&mut self, error: &str) {
		self.failed_at = Some(Instant::now());
		let last = snapshot::current_marker().map_or(0, |m| m.block_number);
		let behind = self.chain.finalized().0.saturating_sub(last);
		let margin = i128::from(self.state_pruning) - i128::from(behind);
		log::error!(
			target: LOG_TARGET,
			"Ledger snapshot failed: {error}. The last one is at #{last}, {behind} blocks behind \
			 finality; the node can restart from it until Substrate prunes its state, in about \
			 {margin} blocks. Retrying in {RETRY_AFTER:?}."
		);
	}

	fn maybe_snapshot(&mut self) -> Result<(), String> {
		let Some(current) = snapshot::current_marker() else { return Ok(()) };
		let (number, hash) = self.chain.finalized();
		let due = snapshot::is_requested()
			|| self.last_snapshot.elapsed() >= self.settings.interval
			|| number.saturating_sub(current.block_number) >= u64::from(self.settings.block_cap);
		if !due
			|| self.snapshot.is_some()
			|| number <= current.block_number
			|| self.failed_at.is_some_and(|t| t.elapsed() < RETRY_AFTER)
		{
			return Ok(());
		}
		let request = SnapshotRequest {
			block_number: number,
			block_hash: Some(hash.as_bytes().to_vec()),
			state_key: self.required_state_key(hash, number)?,
		};
		self.snapshot = Some(std::thread::spawn(move || snapshot::take_snapshot(&request)));
		Ok(())
	}
}

/// The blocks on the paths from `leaves` down to the base block, by number, base excluded.
/// `header` gives a block's number and parent.
fn descendants(
	leaves: &[H256],
	base_number: u64,
	base_hash: H256,
	header: impl Fn(H256) -> Result<Option<(u64, H256)>, String>,
) -> Result<BTreeSet<(u64, H256)>, String> {
	let mut blocks = BTreeSet::new();
	for &leaf in leaves {
		let mut path = Vec::new();
		let mut hash = leaf;
		loop {
			let (number, parent) = header(hash)?.ok_or_else(|| format!("no header {hash:?}"))?;
			if number <= base_number || blocks.contains(&(number, hash)) {
				if blocks.contains(&(number, hash)) || hash == base_hash {
					blocks.extend(path);
				}
				break;
			}
			path.push((number, hash));
			hash = parent;
		}
	}
	Ok(blocks)
}

/// The last `count` finalized blocks, from the finalized one down.
fn ancestors(
	finalized: H256,
	count: u32,
	header: impl Fn(H256) -> Result<Option<(u64, H256)>, String>,
) -> Result<Vec<(u64, H256)>, String> {
	let mut blocks = Vec::new();
	let mut hash = finalized;
	for _ in 0..count {
		let Some((number, parent)) = header(hash)? else { break };
		blocks.push((number, hash));
		if number == 0 {
			break;
		}
		hash = parent;
	}
	Ok(blocks)
}

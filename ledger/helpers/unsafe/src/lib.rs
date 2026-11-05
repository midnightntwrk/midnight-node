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

//! Ledger transaction/wallet-building apparatus used exclusively by
//! `midnight-node-toolkit` (the CLI and transaction generator) and by test
//! tooling that borrows it as a dev-dependency (`tests/e2e`, `pallets/midnight`'s
//! and `midnight-node-ledger`'s own unit tests).
//!
//! This crate builds transactions and wallets by hand from raw inputs, which can
//! genuinely panic on malformed input — unlike the node's real block-execution
//! path in `midnight-node-ledger`. It sits alongside `midnight-node-ledger-helpers`
//! (the shared/production ledger glue) and depends on it for the version-independent
//! passthrough types (`BlockContext`, `WalletSeed`, `fork::raw_block_data`, etc.).

pub mod arena_db;
pub mod extract_tx_with_context;

/// Process-wide counters of replayed transactions that did not fully apply,
/// shared by every ledger generation's `LedgerContext`, plus the cumulative
/// wall-clock time each phase of block replay has consumed so far.
///
/// The phase counters are written on the replay thread (one `fetch_add` per
/// transaction or block, which is negligible next to the work being timed) and
/// read by the toolkit's replay progress logging to attribute replay time.
pub mod replay_stats {
	use std::{
		fmt,
		sync::atomic::{AtomicU64, Ordering::Relaxed},
		time::{Duration, Instant},
	};

	pub static PARTIALLY_FAILED_TXS: AtomicU64 = AtomicU64::new(0);
	pub static FAILED_TXS: AtomicU64 = AtomicU64::new(0);

	/// Blocks applied through `LedgerContext::update_from_block`.
	pub static BLOCKS_APPLIED: AtomicU64 = AtomicU64::new(0);
	/// Transactions (Midnight and system) applied to the ledger state.
	pub static TXS_APPLIED: AtomicU64 = AtomicU64::new(0);
	/// Decoding a block's raw transaction bytes into ledger transactions.
	pub static TX_DESERIALIZE_NS: AtomicU64 = AtomicU64::new(0);
	/// `Transaction::well_formed` (proof-erased during replay) and the fee cost model.
	pub static TX_WELL_FORMED_NS: AtomicU64 = AtomicU64::new(0);
	/// `LedgerState::apply` / `apply_system_tx`.
	pub static TX_APPLY_NS: AtomicU64 = AtomicU64::new(0);
	/// Feeding a transaction's successful shielded offers to every wallet.
	pub static WALLET_OFFERS_NS: AtomicU64 = AtomicU64::new(0);
	/// Allocating the post-transaction ledger state in the storage arena (`Sp::new`).
	pub static STATE_ALLOC_NS: AtomicU64 = AtomicU64::new(0);
	/// `LedgerState::post_block_update` plus its arena allocation.
	pub static POST_BLOCK_NS: AtomicU64 = AtomicU64::new(0);
	/// Computing the local state root for the on-chain comparison.
	pub static STATE_ROOT_NS: AtomicU64 = AtomicU64::new(0);
	/// Replaying batched dust events into every wallet.
	pub static DUST_EVENTS_NS: AtomicU64 = AtomicU64::new(0);

	/// Add the time elapsed since `since` to `counter`.
	pub fn record(counter: &AtomicU64, since: Instant) {
		counter.fetch_add(since.elapsed().as_nanos() as u64, Relaxed);
	}

	/// A point-in-time copy of the phase counters.
	#[derive(Clone, Copy, Debug, Default)]
	pub struct Phases {
		pub blocks: u64,
		pub txs: u64,
		pub tx_deserialize: Duration,
		pub tx_well_formed: Duration,
		pub tx_apply: Duration,
		pub wallet_offers: Duration,
		pub state_alloc: Duration,
		pub post_block: Duration,
		pub state_root: Duration,
		pub dust_events: Duration,
	}

	impl Phases {
		pub fn snapshot() -> Self {
			let d = |c: &AtomicU64| Duration::from_nanos(c.load(Relaxed));
			Self {
				blocks: BLOCKS_APPLIED.load(Relaxed),
				txs: TXS_APPLIED.load(Relaxed),
				tx_deserialize: d(&TX_DESERIALIZE_NS),
				tx_well_formed: d(&TX_WELL_FORMED_NS),
				tx_apply: d(&TX_APPLY_NS),
				wallet_offers: d(&WALLET_OFFERS_NS),
				state_alloc: d(&STATE_ALLOC_NS),
				post_block: d(&POST_BLOCK_NS),
				state_root: d(&STATE_ROOT_NS),
				dust_events: d(&DUST_EVENTS_NS),
			}
		}

		/// Sum of every timed phase.
		pub fn total(&self) -> Duration {
			self.tx_deserialize
				+ self.tx_well_formed
				+ self.tx_apply
				+ self.wallet_offers
				+ self.state_alloc
				+ self.post_block
				+ self.state_root
				+ self.dust_events
		}
	}

	impl fmt::Display for Phases {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(
				f,
				"{} blocks / {} txs: deserialize {:.1?} | well_formed {:.1?} | apply {:.1?} | \
				 wallet offers {:.1?} | state alloc {:.1?} | post-block {:.1?} | \
				 state-root {:.1?} | dust events {:.1?} (timed total {:.1?})",
				self.blocks,
				self.txs,
				self.tx_deserialize,
				self.tx_well_formed,
				self.tx_apply,
				self.wallet_offers,
				self.state_alloc,
				self.post_block,
				self.state_root,
				self.dust_events,
				self.total(),
			)
		}
	}

	/// Occupancy of the in-memory storage arena that backs the ledger state.
	#[derive(Clone, Copy, Debug, Default)]
	pub struct ArenaStats {
		/// Nodes tracked by the arena (every live hashed object).
		pub nodes: usize,
		/// Objects pending in the backend's unbounded write cache.
		pub write_cache_objs: usize,
		/// Serialized bytes held by those write-cache objects.
		pub write_cache_bytes: usize,
	}

	impl fmt::Display for ArenaStats {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(
				f,
				"{} nodes, write cache {} objs / {:.1} MiB",
				self.nodes,
				self.write_cache_objs,
				self.write_cache_bytes as f64 / (1024.0 * 1024.0)
			)
		}
	}

	pub fn arena_stats_8() -> ArenaStats {
		let storage = crate::ledger_8::default_storage::<crate::ledger_8::DefaultDB>();
		let nodes = storage.arena.size();
		let (write_cache_objs, write_cache_bytes) = storage
			.arena
			.with_backend(|b| (b.get_write_cache_len(), b.get_write_cache_obj_bytes()));
		ArenaStats { nodes, write_cache_objs, write_cache_bytes }
	}

	pub fn arena_stats_9() -> ArenaStats {
		let storage = crate::ledger_9::default_storage::<crate::ledger_9::DefaultDB>();
		let nodes = storage.arena.size();
		let (write_cache_objs, write_cache_bytes) = storage
			.arena
			.with_backend(|b| (b.get_write_cache_len(), b.get_write_cache_obj_bytes()));
		ArenaStats { nodes, write_cache_objs, write_cache_bytes }
	}
}

/// Strategy for ordering candidate coins/UTXOs during input selection.
///
/// Defined at the crate root (not inside `ledger_8`/`ledger_9`) so that all versions
/// see the same type, allowing it to flow through the toolkit's version-dispatched
/// builders unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoinSelectionStrategy {
	/// Use the largest coins/UTXOs first. Minimizes the number of inputs.
	#[default]
	LargestFirst,
	/// Use the smallest coins/UTXOs first. Consolidates dust.
	SmallestFirst,
}

/// Struct to store serialized verifying key bytes
/// To be deserialized when constructing ContractOperations
pub struct ContractVerifyingKeyBytes(pub Vec<u8>);

pub mod ledger_8;
pub mod ledger_9;

pub use ledger_9 as latest;

pub mod fork;

pub use latest::*;

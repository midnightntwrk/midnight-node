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

//! RPC endpoint for ledger-state collection sizes — principally the unshielded
//! UTXO set size, which nothing else on the chain exposes.
//!
//! The UTXO set lives in the midnight-ledger arena, a content-addressed blob
//! outside the Substrate trie, so no `state_getStorage` query can reach it. This
//! method resolves it the same way the warp ledger-sync server does: read the raw
//! `pallet_midnight::StateKey` from the trie at the requested block, then ask the
//! ledger crate to resolve that key in the node's already-open arena. That keeps
//! it **entirely node-side** — no runtime API, so shipping it is a binary swap
//! rather than a runtime upgrade.
//!
//! Every number served is an O(1) annotation read in state size, not a traversal
//! (see `midnight_node_ledger::ledger_stats`) — but it is *not* O(1) in disk
//! latency: resolving the `StateKey` loads the root nodes from the arena, so the
//! read runs on tokio's blocking pool rather than inline on a worker. The memo
//! below means repeated polling of the same block costs one hash comparison and
//! touches the arena — and therefore the arena's process-global locks, which block
//! execution also takes — exactly once, with concurrent callers for the same block
//! sharing a single read instead of piling on.

use jsonrpsee::{
	core::{RpcResult, async_trait},
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
};
use midnight_node_ledger::types::LedgerStats as InnerLedgerStats;
use sc_client_api::{Backend, StorageProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::HashMap,
	hash::Hash,
	marker::PhantomData,
	sync::{Arc, Mutex, MutexGuard, PoisonError},
};
use tokio::sync::OnceCell;

use crate::warp_ledger_sync::read_state_key;

/// Collection sizes held at the ledger-state root.
///
/// The `u64` counts serialize as JSON numbers: the UTXO set would have to reach
/// 9e15 entries to strain a double, which it cannot.
///
/// `unshieldedUtxoStars` is a `u128` and is serialized as a decimal string.
/// JSON itself imposes no bound on integer literals, so a number here would be
/// perfectly legal — but `JSON.parse` maps every number onto a double, and NIGHT
/// in Stars genuinely exceeds `Number.MAX_SAFE_INTEGER` (the 24e9 NIGHT max
/// supply is 2.4e16 Stars, against a 9.007e15 ceiling). A client would have to
/// opt into a BigInt-aware parser to read it losslessly, and one that did not
/// would be silently wrong rather than loudly broken. A string makes the
/// precision requirement explicit; `pattern` below keeps it machine-checkable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStats {
	/// Live unshielded UTXOs — the UTXO set size.
	pub unshielded_utxo_count: u64,
	/// NIGHT held across those UTXOs, in atomic Stars (1 NIGHT = 1e6 Stars), as a
	/// decimal string.
	#[schemars(regex(pattern = r"^[0-9]+$"))]
	pub unshielded_utxo_stars: String,
	/// Zswap note commitments ever created.
	pub zswap_commitment_count: u64,
	/// Zswap nullifiers, i.e. shielded notes ever spent.
	pub zswap_nullifier_count: u64,
	/// Live shielded notes: commitments minus nullifiers.
	pub zswap_live_note_count: u64,
	/// DUST commitments ever created.
	pub dust_commitment_count: u64,
	/// DUST nullifiers, i.e. DUST notes ever spent.
	pub dust_nullifier_count: u64,
	/// Live DUST notes: commitments minus nullifiers.
	pub dust_live_note_count: u64,
	/// Contracts currently deployed.
	pub contract_count: u64,
}

impl From<InnerLedgerStats> for LedgerStats {
	fn from(s: InnerLedgerStats) -> Self {
		LedgerStats {
			unshielded_utxo_count: s.unshielded_utxo_count,
			unshielded_utxo_stars: s.unshielded_utxo_stars.to_string(),
			zswap_commitment_count: s.zswap_commitment_count,
			zswap_nullifier_count: s.zswap_nullifier_count,
			zswap_live_note_count: s.zswap_commitment_count.saturating_sub(s.zswap_nullifier_count),
			dust_commitment_count: s.dust_commitment_count,
			dust_nullifier_count: s.dust_nullifier_count,
			dust_live_note_count: s.dust_commitment_count.saturating_sub(s.dust_nullifier_count),
			contract_count: s.contract_count,
		}
	}
}

#[derive(Debug)]
pub enum LedgerStatsError {
	/// The requested block hash is not known to this node.
	UnknownBlock,
	/// Reading `pallet_midnight::StateKey` from the trie failed. On a pruned node
	/// this is the expected outcome for a block outside the retained state window.
	StateKeyUnavailable(String),
	/// The pallet holds no `StateKey` at that block.
	NoStateKey,
	/// The arena could not resolve the state the `StateKey` points at.
	LedgerUnavailable(String),
}

impl std::fmt::Display for LedgerStatsError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			LedgerStatsError::UnknownBlock => write!(f, "requested block is unknown to this node"),
			LedgerStatsError::StateKeyUnavailable(reason) => write!(
				f,
				"unable to read ledger StateKey at the requested block \
				 (a pruned node has no state for older blocks): {reason}"
			),
			LedgerStatsError::NoStateKey => {
				write!(f, "pallet StateKey not present at the requested block")
			},
			LedgerStatsError::LedgerUnavailable(reason) => {
				write!(f, "unable to read ledger state: {reason}")
			},
		}
	}
}

impl std::error::Error for LedgerStatsError {}

impl From<LedgerStatsError> for ErrorObjectOwned {
	fn from(value: LedgerStatsError) -> Self {
		let code = match value {
			LedgerStatsError::UnknownBlock | LedgerStatsError::NoStateKey => INVALID_PARAMS_CODE,
			_ => INTERNAL_ERROR_CODE,
		};
		ErrorObject::owned(code, value.to_string(), None::<()>)
	}
}

#[rpc(client, server)]
pub trait LedgerStatsApi<BlockHash> {
	/// Returns the collection sizes held at the ledger-state root, most notably
	/// the unshielded UTXO set size.
	///
	/// Queries the best block unless `at` names one. Every value is read from a
	/// trie-root annotation, so the cost is independent of the size of the state.
	/// Note that a node running with default state pruning only retains trie state
	/// for recent blocks, so an older `at` will fail on such a node even though the
	/// arena itself still holds the data.
	#[method(name = "midnight_ledgerStats")]
	async fn ledger_stats(&self, at: Option<BlockHash>) -> RpcResult<LedgerStats>;
}

/// Memo of the last answer, plus per-key single-flight for fills in progress.
///
/// Both locks are plain `std` mutexes held only across a lookup or a store — never
/// across a fill — so a caller is parked for no longer than it takes another to
/// touch a `HashMap`. Concurrent callers for the *same* key share one fill through
/// a `tokio::sync::OnceCell`: the first to arrive runs it and the rest await the
/// cell as futures, yielding their worker rather than blocking it. Callers for
/// different keys fill independently; a failed fill is not memoised, and the next
/// waiter on that key runs its own.
struct SingleFlightMemo<K, V> {
	/// The last successful answer. One entry: the overwhelmingly common access
	/// pattern is polling the tip, and a bigger memo would only let a caller
	/// spraying distinct hashes grow it without bound.
	last: Mutex<Option<(K, V)>>,
	/// Fills in progress. An entry lives only for the duration of its fill, so the
	/// map is bounded by the number of concurrent misses, not by call history.
	inflight: Mutex<HashMap<K, Arc<OnceCell<V>>>>,
}

/// A panic while one of the memo's locks is held poisons it. Losing the memo is
/// survivable; refusing every later call is not, so poisoning is ignored.
fn lock_ignoring_poison<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
	m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Removes an in-flight entry when dropped, so it goes whether the fill
/// completed, failed, or never finished: jsonrpsee can drop a call's future
/// mid-flight (server shutdown, HTTP connection closed), and an entry that
/// outlived its fill would otherwise leak, one per abandoned hash.
///
/// Only *this* caller's cell is removed. A late caller could have found the
/// entry gone already (another finisher removed it) and registered a fresh one
/// that is mid-fill; that one must stay.
struct RetireOnDrop<'a, K: Hash + Eq, V> {
	inflight: &'a Mutex<HashMap<K, Arc<OnceCell<V>>>>,
	key: K,
	cell: Arc<OnceCell<V>>,
}

impl<K: Hash + Eq, V> Drop for RetireOnDrop<'_, K, V> {
	fn drop(&mut self) {
		let mut inflight = lock_ignoring_poison(self.inflight);
		if inflight.get(&self.key).is_some_and(|c| Arc::ptr_eq(c, &self.cell)) {
			inflight.remove(&self.key);
		}
	}
}

impl<K: Hash + Eq + Clone, V: Clone> SingleFlightMemo<K, V> {
	fn new() -> Self {
		Self { last: Mutex::new(None), inflight: Mutex::new(HashMap::new()) }
	}

	async fn get_or_fill<E, F, Fut>(&self, key: K, fill: F) -> Result<V, E>
	where
		F: FnOnce() -> Fut,
		Fut: Future<Output = Result<V, E>>,
	{
		if let Some((k, v)) = lock_ignoring_poison(&self.last).as_ref()
			&& *k == key
		{
			return Ok(v.clone());
		}

		let cell = lock_ignoring_poison(&self.inflight).entry(key.clone()).or_default().clone();
		let _retire =
			RetireOnDrop { inflight: &self.inflight, key: key.clone(), cell: cell.clone() };

		// If this caller is the one running `fill` and its future is dropped
		// part-way, the cell stays empty and the next waiter runs its own `fill`.
		let result = cell.get_or_try_init(fill).await.cloned();

		if let Ok(v) = &result {
			*lock_ignoring_poison(&self.last) = Some((key, v.clone()));
		}

		result
	}
}

pub struct LedgerStatsRpc<C, Block: BlockT, BE> {
	client: Arc<C>,
	/// Memo of the last answer, keyed by block hash. Repeated polling of the tip
	/// (the overwhelmingly common case) then never touches the arena.
	memo: SingleFlightMemo<Block::Hash, LedgerStats>,
	_phantom: PhantomData<(Block, BE)>,
}

impl<C, Block: BlockT, BE> LedgerStatsRpc<C, Block, BE> {
	pub fn new(client: Arc<C>) -> Self {
		Self { client, memo: SingleFlightMemo::new(), _phantom: PhantomData }
	}
}

impl<C, Block, BE> LedgerStatsRpc<C, Block, BE>
where
	Block: BlockT,
	BE: Backend<Block> + Send + Sync + 'static,
	C: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	/// Resolve the stats for `at` from the arena. Blocking: the header and trie
	/// reads hit the backend and the arena resolve loads root nodes from disk, so
	/// this is run on the blocking pool, never inline on an async worker.
	fn read_stats(client: &C, at: Block::Hash) -> Result<LedgerStats, LedgerStatsError> {
		if client
			.header(at)
			.map_err(|e| LedgerStatsError::StateKeyUnavailable(e.to_string()))?
			.is_none()
		{
			return Err(LedgerStatsError::UnknownBlock);
		}

		let state_key = read_state_key::<Block, C, BE>(client, at)
			.map_err(|e| LedgerStatsError::StateKeyUnavailable(e.to_string()))?
			.ok_or(LedgerStatsError::NoStateKey)?;

		Ok(midnight_node_ledger::ledger_stats(&state_key)
			.map_err(LedgerStatsError::LedgerUnavailable)?
			.into())
	}
}

#[async_trait]
impl<C, Block, BE> LedgerStatsApiServer<Block::Hash> for LedgerStatsRpc<C, Block, BE>
where
	Block: BlockT,
	BE: Backend<Block> + Send + Sync + 'static,
	C: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	async fn ledger_stats(&self, at: Option<Block::Hash>) -> RpcResult<LedgerStats> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		// On a miss the arena read is handed to the blocking pool. It is cheap in
		// state size but not in latency — it can wait on the arena's process-global
		// locks and load root nodes from disk — and a sync handler would run it inline
		// on a tokio worker the networking and consensus tasks share. The memo
		// single-flights concurrent misses for the same hash, so a polling burst on a
		// freshly produced block still reads the arena once; misses for *different*
		// hashes proceed in parallel, each costing what one arena read costs.
		let client = self.client.clone();
		let stats = self
			.memo
			.get_or_fill(at, || async move {
				tokio::task::spawn_blocking(move || Self::read_stats(&client, at))
					.await
					.map_err(|e| {
						LedgerStatsError::LedgerUnavailable(format!(
							"ledger stats read task failed: {e}"
						))
					})?
			})
			.await?;

		Ok(stats)
	}
}

#[cfg(test)]
mod tests {
	use super::SingleFlightMemo;
	use std::sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	};
	use tokio::sync::Semaphore;

	type Fills = Arc<AtomicUsize>;
	/// Fills block on this until the test hands out a permit, so callers spawned
	/// meanwhile queue on the cell instead of racing the fill to completion.
	type Gate = Arc<Semaphore>;

	fn gated_fill(
		fills: &Fills,
		gate: &Gate,
		value: Result<u32, &'static str>,
	) -> impl Future<Output = Result<u32, &'static str>> + use<> {
		let (fills, gate) = (fills.clone(), gate.clone());
		async move {
			fills.fetch_add(1, Ordering::SeqCst);
			gate.acquire().await.expect("gate is never closed").forget();
			value
		}
	}

	/// Let every task spawned so far run up to its next await point. The tests use
	/// the current-thread runtime, so spawned tasks only progress while this one
	/// yields.
	async fn settle() {
		for _ in 0..32 {
			tokio::task::yield_now().await;
		}
	}

	#[tokio::test]
	async fn concurrent_misses_for_one_key_share_a_single_fill() {
		let memo = Arc::new(SingleFlightMemo::<u8, u32>::new());
		let fills = Fills::default();
		let gate = Gate::new(Semaphore::new(0));

		let callers: Vec<_> = (0..8)
			.map(|_| {
				let (memo, fills, gate) = (memo.clone(), fills.clone(), gate.clone());
				tokio::spawn(async move {
					memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(42))).await
				})
			})
			.collect();
		settle().await;
		assert_eq!(fills.load(Ordering::SeqCst), 1, "one caller is filling, the rest wait");

		gate.add_permits(1);
		for c in callers {
			assert_eq!(c.await.unwrap(), Ok(42));
		}
		assert_eq!(fills.load(Ordering::SeqCst), 1);
		assert!(memo.inflight.lock().unwrap().is_empty(), "in-flight entry retired");
	}

	#[tokio::test]
	async fn hit_skips_the_fill_and_memo_holds_one_entry() {
		let memo = SingleFlightMemo::<u8, u32>::new();
		let fills = Fills::default();
		let gate = Gate::new(Semaphore::new(Semaphore::MAX_PERMITS));

		assert_eq!(memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(10))).await, Ok(10));
		assert_eq!(memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(99))).await, Ok(10));
		assert_eq!(fills.load(Ordering::SeqCst), 1, "second call is a hit");

		assert_eq!(memo.get_or_fill(2, || gated_fill(&fills, &gate, Ok(20))).await, Ok(20));
		assert_eq!(fills.load(Ordering::SeqCst), 2, "different key fills");

		assert_eq!(memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(11))).await, Ok(11));
		assert_eq!(fills.load(Ordering::SeqCst), 3, "key 1 was evicted by key 2");
		assert!(memo.inflight.lock().unwrap().is_empty());
	}

	#[tokio::test]
	async fn failed_fill_is_not_memoised_and_waiters_retry() {
		let memo = Arc::new(SingleFlightMemo::<u8, u32>::new());
		let fills = Fills::default();
		let gate = Gate::new(Semaphore::new(0));

		let first = {
			let (memo, fills, gate) = (memo.clone(), fills.clone(), gate.clone());
			tokio::spawn(async move {
				memo.get_or_fill(1, || gated_fill(&fills, &gate, Err("boom"))).await
			})
		};
		settle().await;
		let second = {
			let (memo, fills, gate) = (memo.clone(), fills.clone(), gate.clone());
			tokio::spawn(
				async move { memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(7))).await },
			)
		};
		settle().await;
		assert_eq!(fills.load(Ordering::SeqCst), 1, "second is queued behind first's fill");

		// First's fill fails; second then runs its own rather than inheriting the error.
		gate.add_permits(2);
		assert_eq!(first.await.unwrap(), Err("boom"));
		assert_eq!(second.await.unwrap(), Ok(7));
		assert_eq!(fills.load(Ordering::SeqCst), 2);
		assert!(memo.inflight.lock().unwrap().is_empty());

		assert_eq!(
			memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(8))).await,
			Ok(7),
			"the success, not the failure, is what got memoised"
		);
		assert_eq!(fills.load(Ordering::SeqCst), 2);
	}

	#[tokio::test]
	async fn cancelled_fill_retires_its_entry_and_a_waiter_takes_over() {
		let memo = Arc::new(SingleFlightMemo::<u8, u32>::new());
		let fills = Fills::default();
		let gate = Gate::new(Semaphore::new(0));

		let leader = {
			let (memo, fills, gate) = (memo.clone(), fills.clone(), gate.clone());
			tokio::spawn(
				async move { memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(1))).await },
			)
		};
		settle().await;
		assert_eq!(fills.load(Ordering::SeqCst), 1);

		// A sole caller abandoned mid-fill must not leave its entry behind.
		leader.abort();
		assert!(leader.await.unwrap_err().is_cancelled());
		assert!(memo.inflight.lock().unwrap().is_empty(), "entry retired on cancel");

		// With a waiter queued, the waiter runs its own fill once the leader is gone.
		let leader = {
			let (memo, fills, gate) = (memo.clone(), fills.clone(), gate.clone());
			tokio::spawn(
				async move { memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(2))).await },
			)
		};
		settle().await;
		let waiter = {
			let (memo, fills, gate) = (memo.clone(), fills.clone(), gate.clone());
			tokio::spawn(
				async move { memo.get_or_fill(1, || gated_fill(&fills, &gate, Ok(3))).await },
			)
		};
		settle().await;
		assert_eq!(fills.load(Ordering::SeqCst), 2, "waiter is queued, not filling");

		leader.abort();
		assert!(leader.await.unwrap_err().is_cancelled());
		gate.add_permits(1);
		assert_eq!(waiter.await.unwrap(), Ok(3));
		assert_eq!(fills.load(Ordering::SeqCst), 3);
		assert!(memo.inflight.lock().unwrap().is_empty());
	}
}

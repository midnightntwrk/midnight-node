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

//! Unpersist Anchored tip roots (one `unpersist` per tip).

use std::time::Duration;

use ledger_storage_ledger_8::{
	DefaultDB,
	arena::{self, ArenaHash, ArenaKey, TypedArenaKey},
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::{default_storage, try_get_default_storage},
};
use log::warn;
use midnight_primitives_ledger::LedgerStorageExt;
use midnight_serialize::tagged_deserialize;

const LOG_TARGET: &str = "midnight::ledger_gc";

type DbSeparate = ParityDb;
type DbUnified = ParityDb<sha2::Sha256, OwnedDb, { LedgerStorageExt::COLUMN_OFFSET }>;

/// Whether `tip` decodes to an arena root this process can reclaim via
/// [`unpersist_tips`] (ledger-8 or ledger-9 typed key under any supported DB
/// hasher).
///
/// Used at finality-index time so AuxStore never accumulates bindings we would
/// only skip later (pre-ledger-8 era roots: leak-by-not-indexing).
pub fn is_reclaimable_tip(tip: &[u8]) -> bool {
	tip_root::<DbSeparate>(tip).is_some()
		|| tip_root::<DbUnified>(tip).is_some()
		|| tip_root::<DefaultDB>(tip).is_some()
}

/// Advisory: whether the initialized ledger backend's write cache is empty.
///
/// The GC worker checks this *before* durably retiring AuxStore bindings, to
/// avoid the retire → busy-defer → re-bind churn when a block is mid-
/// execution. Purely advisory — the authoritative check runs under the
/// backend lock inside [`unpersist_tips`]. Returns `true` when no storage is
/// initialized (the reclaim path will then fail with its own error).
pub fn ledger_quiescent() -> bool {
	fn quiescent_in<D: DB + 'static>() -> Option<bool> {
		try_get_default_storage::<D>()
			.map(|storage| storage.with_backend(|backend| backend.get_write_cache_len() == 0))
	}
	quiescent_in::<DbSeparate>()
		.or_else(quiescent_in::<DbUnified>)
		.or_else(quiescent_in::<DefaultDB>)
		.unwrap_or(true)
}

/// Unpersist each tip once. Returns how many arena roots were decremented.
///
/// On success the root-count decrements are flushed to the ledger DB before
/// returning — `unpersist` alone only stages them in the write cache, and a
/// process exit before the next block's normal ledger flush would otherwise
/// drop the cache while the caller's AuxStore binding is already gone,
/// leaving the on-disk root permanently unreclaimable.
///
/// **Quiescence-gated**: the decrement + flush run only when the shared
/// ledger write cache is empty, so the durability flush writes *isolated GC
/// decrements* and nothing else. Flushing a dirty cache would land another
/// block execution's staged, not-yet-rooted nodes in the DB and make the
/// arena sweep's own quiescence check pass mid-execution — letting it cull
/// state the runtime still needs. A dirty cache returns `Err` ("busy");
/// callers should treat it as retryable and re-submit the tips later.
///
/// **Count-clamped**: the batch is coalesced by arena root and each root is
/// decremented at most its observed count — never per-tip, so duplicate
/// bindings exceeding the count (a state-synced block that never executed
/// locally sharing a tip with one executed block; a crash-replayed binding
/// across the two independent parity-db WALs) clamp to a warned no-op
/// instead of underflowing the flush-time `root_count >= 0` assert.
///
/// **Not idempotent** for shared roots: replaying a tip whose root is also
/// persisted by another still-live block (recurrence class) steals that
/// count. Callers must keep at-most-once delivery per tip: durably retire
/// the tip *before* calling this, and only re-submit a tip whose retirement
/// was rolled back (see the node GC worker's remove-then-unpersist ordering).
pub fn unpersist_tips(tips: &[Vec<u8>]) -> Result<usize, String> {
	if tips.is_empty() {
		return Ok(0);
	}
	let storage_ready = try_get_default_storage::<DbSeparate>().is_some()
		|| try_get_default_storage::<DbUnified>().is_some()
		|| try_get_default_storage::<DefaultDB>().is_some();
	if let Some(n) = unpersist_in::<DbSeparate>(tips)? {
		return Ok(n);
	}
	if let Some(n) = unpersist_in::<DbUnified>(tips)? {
		return Ok(n);
	}
	if let Some(n) = unpersist_in::<DefaultDB>(tips)? {
		return Ok(n);
	}
	// Initialized backend(s) existed but no tip decoded for their hasher —
	// fail-safe (caller already retired bindings). Distinct from "no storage".
	if storage_ready {
		warn!(
			target: LOG_TARGET,
			"⏭️  No tip roots unpersisted ({} tip(s) undecodable for initialized storage)",
			tips.len()
		);
		return Ok(0);
	}
	Err("ledger storage is not initialized".into())
}

/// Run incremental arena GC for up to `budget`. Returns culled node count.
///
/// Sweeps only when the ledger write cache is quiescent (empty): the
/// mark/sweep's reachability is DB-based and staged-but-unflushed state is
/// invisible to it, so sweeping concurrently with an executing block could
/// cull nodes the in-flight state still references. When the cache is dirty
/// this returns `0` and the caller should retry after the next block flush.
pub fn collect_garbage(budget: Duration) -> Result<usize, String> {
	if let Some(n) = gc_in::<DbSeparate>(budget)? {
		return Ok(n);
	}
	if let Some(n) = gc_in::<DbUnified>(budget)? {
		return Ok(n);
	}
	if let Some(n) = gc_in::<DefaultDB>(budget)? {
		return Ok(n);
	}
	Err("ledger storage is not initialized".into())
}

/// Arena root for a tip payload. Tries the ledger-9 then the ledger-8 typed
/// key (their `ledger-state[vN]` tags differ; both live in this storage
/// crate's arena). Only `key.hash()` was incremented by `persist()` — never
/// unpersist `refs()`, those are child hashes, not GC roots.
fn tip_root<D: DB>(tip: &[u8]) -> Option<ArenaHash<D::Hasher>> {
	if let Ok(key) = tagged_deserialize::<TypedArenaKey<crate::ledger_9::api::Ledger<D>, D::Hasher>>(
		&mut &tip[..],
	) {
		let key: ArenaKey<D::Hasher> = key.into();
		return Some(key.hash().clone());
	}
	if let Ok(key) = tagged_deserialize::<TypedArenaKey<crate::ledger_8::api::Ledger<D>, D::Hasher>>(
		&mut &tip[..],
	) {
		let key: ArenaKey<D::Hasher> = key.into();
		return Some(key.hash().clone());
	}
	None
}

fn unpersist_in<D: DB + 'static>(tips: &[Vec<u8>]) -> Result<Option<usize>, String> {
	if try_get_default_storage::<D>().is_none() {
		return Ok(None);
	}
	let mut roots = Vec::with_capacity(tips.len());
	let mut skipped = 0usize;
	for tip in tips {
		// Undecodable for *this* hasher → skip (do not fail the batch; caller
		// already retired bindings). Empty `roots` returns `None` so the
		// caller can try another initialized backend before giving up.
		match tip_root::<D>(tip) {
			Some(root) => roots.push(root),
			None => skipped += 1,
		}
	}
	if roots.is_empty() {
		return Ok(None);
	}
	if skipped > 0 {
		warn!(
			target: LOG_TARGET,
			"⏭️  Unpersisting {} tip root(s); skipped {skipped} not decodable for this DB",
			roots.len()
		);
	}

	// Count-clamp (pre-read; `test_helpers::get_root_count` takes the backend
	// RefCell itself, so it must not run inside `with_backend`): coalesce the
	// batch by root and decrement at most the observed count per root. A
	// binding can outlive or outnumber its persists — (a) crash replay:
	// AuxStore retirement and the flush below ride *independent* parity-db
	// WALs in separate-DB mode, so a crash can durably keep the decrement
	// while losing the removal; (b) state-synced (warp/fast) blocks: a
	// readable `StateKey` for a block that never executed locally contributes
	// a binding but no persist, so duplicate-tip batches can request more
	// decrements than the root holds (per-tip zero-checks would each see the
	// same nonzero count and all pass). Decrementing past the count would
	// underflow into the flush-time `root_count >= 0` assert — after the
	// cache was already cleared — discarding everything staged with it.
	// Counts can only rise between this read and the hold below (Bridge never
	// unpersists Anchored roots), which is safe.
	let storage = default_storage::<D>();
	let mut per_root: Vec<(ArenaHash<D::Hasher>, u32)> = Vec::new();
	for root in roots {
		match per_root.iter_mut().find(|(r, _)| *r == root) {
			Some((_, mult)) => *mult += 1,
			None => per_root.push((root, 1)),
		}
	}
	let mut to_zero: Vec<(ArenaHash<D::Hasher>, u32)> = Vec::new();
	let mut clamped = 0u32;
	for (root, mult) in per_root {
		let take = mult.min(arena::test_helpers::get_root_count(&storage.arena, &root));
		clamped += mult - take;
		if take > 0 {
			to_zero.push((root, take));
		}
	}
	if clamped > 0 {
		warn!(
			target: LOG_TARGET,
			"⏭️  Clamped {clamped} decrement(s) exceeding observed root count(s) \
			 (crash-replayed binding or state-synced block never executed locally)"
		);
	}
	if to_zero.is_empty() {
		return Ok(Some(0));
	}

	let flushed = storage.with_backend(|backend| {
		// Quiescence gate for the reclaim hold: the durability flush below
		// drains the WHOLE shared write cache. If another block execution has
		// staged writes (between host calls, before its `on_finalize` flush),
		// flushing here would land its unrooted in-flight nodes in the DB and
		// leave the cache empty — making the arena sweep's own quiescence
		// check pass mid-execution and cull state the runtime still needs.
		// Defer instead (caller re-binds and retries); with an empty cache at
		// this point, the flush writes ONLY our decrements.
		if backend.get_write_cache_len() > 0 {
			return false;
		}
		for (root, take) in &to_zero {
			for _ in 0..*take {
				backend.unpersist(root);
			}
		}
		// Durability: AuxStore bindings are already retired by the caller.
		// Without this flush, shutdown drops the write cache and the on-disk
		// root counts stay elevated with nothing left to retry the decrement.
		backend.flush_all_changes_to_db();
		true
	});
	if !flushed {
		return Err("ledger write cache busy; tip reclaim deferred".into());
	}
	Ok(Some(to_zero.iter().map(|(_, take)| *take as usize).sum()))
}

fn gc_in<D: DB + 'static>(budget: Duration) -> Result<Option<usize>, String> {
	if try_get_default_storage::<D>().is_none() {
		return Ok(None);
	}
	let culled = default_storage::<D>().with_backend(|backend| {
		// Quiescence gate: the incremental mark/sweep computes reachability
		// from DB roots (plus live inserts) and its write barrier re-arms only
		// on flushed root-count changes. Staged-but-unflushed state — a block
		// mid-execution after its `Sp`s dropped, or eviction-flushed interior
		// nodes whose root has not landed yet — is invisible to the mark set,
		// so a concurrent sweep could cull DB nodes that only the staged DAG
		// references and even evict their pending cache deltas, leaving the
		// next host call unable to load the state (fail-deadly). Only sweep
		// when the write cache is empty: everything is then in the DB, and
		// every root that changed since the last mark has forced a rescan.
		// Deferring costs at most reclaim latency (retried next slice).
		if backend.get_write_cache_len() > 0 {
			return 0;
		}
		backend.gc(budget)
	});
	Ok(Some(culled))
}

#[cfg(test)]
mod tests {
	use super::*;
	use ledger_storage_ledger_8::storage::default_storage;
	use midnight_serialize::tagged_serialize;

	/// Serialized typed key for a fresh ledger-9 state (allocated in the shared
	/// in-memory test arena), persisted `persists` times.
	fn ledger_9_tip(network_id: &str, persists: u32) -> Vec<u8> {
		use crate::ledger_9::api::Ledger;
		use mn_ledger_9::structure::LedgerState;

		let state: LedgerState<DefaultDB> = LedgerState::new(network_id);
		let mut sp = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		for _ in 0..persists {
			sp.persist();
		}
		default_storage::<DefaultDB>().with_backend(|b| b.flush_all_changes_to_db());
		let mut tip = vec![];
		tagged_serialize(&sp.as_typed_key(), &mut tip).expect("tip serializes");
		tip
	}

	fn ledger_8_tip(network_id: &str) -> Vec<u8> {
		use crate::ledger_8::api::Ledger;
		use mn_ledger_8::structure::LedgerState;

		let state: LedgerState<DefaultDB> = LedgerState::new(network_id);
		let sp = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		// Flush so the shared test arena's write cache stays quiescent —
		// `unpersist_tips` defers on a dirty cache.
		default_storage::<DefaultDB>().with_backend(|b| b.flush_all_changes_to_db());
		let mut tip = vec![];
		tagged_serialize(&sp.as_typed_key(), &mut tip).expect("tip serializes");
		tip
	}

	fn root_count(tip: &[u8]) -> u32 {
		let root = tip_root::<DefaultDB>(tip).expect("tip decodes");
		default_storage::<DefaultDB>()
			.with_backend(|b| b.get_roots().get(&root).copied().unwrap_or(0))
	}

	/// `unpersist_tips` with bounded retry on the quiescence deferral: tests
	/// share one global arena and run in parallel, so another test's staging
	/// can transiently dirty the write cache.
	fn unpersist_retrying(tips: &[Vec<u8>]) -> usize {
		for _ in 0..50 {
			match unpersist_tips(tips) {
				Ok(n) => return n,
				Err(e) if e.contains("busy") => {
					std::thread::sleep(Duration::from_millis(10));
				},
				Err(e) => panic!("unpersist_tips failed: {e}"),
			}
		}
		panic!("ledger write cache never became quiescent");
	}

	#[test]
	fn reclaimable_tip_detection() {
		// Both supported ledger versions decode; both tags are recognized.
		assert!(is_reclaimable_tip(&ledger_9_tip("gc-detect-9", 0)));
		assert!(is_reclaimable_tip(&ledger_8_tip("gc-detect-8")));
		// Arbitrary bytes (pre-ledger-8 / junk) are not reclaimable and must
		// be filtered out at index time, not error later.
		assert!(!is_reclaimable_tip(&[0u8; 40]));
		assert!(!is_reclaimable_tip(b"not a ledger tip"));
		assert!(!is_reclaimable_tip(&[]));
	}

	#[test]
	fn unpersist_decrements_once_per_tip() {
		let tip = ledger_9_tip("gc-once", 1);
		assert_eq!(root_count(&tip), 1);
		assert_eq!(unpersist_retrying(std::slice::from_ref(&tip)), 1);
		assert_eq!(root_count(&tip), 0);
	}

	#[test]
	fn duplicate_bindings_unpersist_full_multiplicity() {
		// Two blocks bound to the same tip (e.g. sibling empty forks in one
		// slot): each executed block persisted once, so reclaim must send the
		// tip once per binding — deduping would strand the surplus count with
		// no binding left to retry it.
		let tip = ledger_9_tip("gc-mult", 2);
		assert_eq!(root_count(&tip), 2);
		assert_eq!(unpersist_retrying(&[tip.clone(), tip.clone()]), 2);
		assert_eq!(root_count(&tip), 0);
	}

	#[test]
	fn zero_count_root_is_clamped_not_underflowed() {
		// A binding can outlive its persist count: crash-replayed removal
		// (independent WALs) or a state-synced block that never executed
		// locally. The decrement must clamp to a no-op, not underflow into
		// the flush-time `root_count >= 0` assert.
		let tip = ledger_9_tip("gc-clamp", 0);
		assert_eq!(root_count(&tip), 0);
		assert_eq!(unpersist_retrying(std::slice::from_ref(&tip)), 0);
		assert_eq!(root_count(&tip), 0);

		// And a replayed rc=1 reclaim: first call decrements, replay is a no-op.
		let tip = ledger_9_tip("gc-clamp-replay", 1);
		assert_eq!(unpersist_retrying(std::slice::from_ref(&tip)), 1);
		assert_eq!(unpersist_retrying(std::slice::from_ref(&tip)), 0);
		assert_eq!(root_count(&tip), 0);
	}

	#[test]
	fn duplicate_tips_clamp_to_observed_root_count() {
		// One locally executed block persisted the tip once, but TWO bindings
		// carry it (e.g. a state-synced sibling that never executed locally
		// shares the empty-block tip). Per-tip zero-checks would each see
		// count 1 and both pass — the batch must coalesce by root and
		// decrement at most the observed count, not underflow.
		let tip = ledger_9_tip("gc-dup-clamp", 1);
		assert_eq!(root_count(&tip), 1);
		assert_eq!(unpersist_retrying(&[tip.clone(), tip.clone()]), 1);
		assert_eq!(root_count(&tip), 0);
	}

	#[test]
	fn undecodable_batch_is_leak_not_error() {
		// Ensure the in-memory storage is initialized.
		let _ = default_storage::<DefaultDB>();
		// Undecodable tips with initialized storage must be a no-op (leak),
		// not an error — an error would make the worker re-bind and retry the
		// same batch forever (wedged reclaim).
		assert_eq!(unpersist_retrying(&[vec![1, 2, 3]]), 0);
	}
}

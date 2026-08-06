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
	arena::{ArenaHash, ArenaKey, TypedArenaKey},
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

/// Unpersist each tip once. Returns how many arena roots were decremented.
///
/// **Not idempotent**: each call blindly decrements the tip's arena root
/// count by one — calling twice for the same persist event drives the count
/// negative (storage-core treats negative root counts as a fatal invariant
/// violation) or steals a count from another tip sharing the root. Callers
/// must guarantee at-most-once delivery per tip: durably retire the tip
/// *before* calling this, and only re-submit a tip whose retirement was
/// rolled back (see the node GC worker's remove-then-unpersist ordering).
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
	default_storage::<D>().with_backend(|backend| {
		for root in &roots {
			backend.unpersist(root);
		}
	});
	Ok(Some(roots.len()))
}

fn gc_in<D: DB + 'static>(budget: Duration) -> Result<Option<usize>, String> {
	if try_get_default_storage::<D>().is_none() {
		return Ok(None);
	}
	let culled = default_storage::<D>().with_backend(|backend| backend.gc(budget));
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
		let mut tip = vec![];
		tagged_serialize(&sp.as_typed_key(), &mut tip).expect("tip serializes");
		tip
	}

	fn root_count(tip: &[u8]) -> u32 {
		let root = tip_root::<DefaultDB>(tip).expect("tip decodes");
		default_storage::<DefaultDB>()
			.with_backend(|b| b.get_roots().get(&root).copied().unwrap_or(0))
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
		assert_eq!(unpersist_tips(std::slice::from_ref(&tip)).unwrap(), 1);
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
		assert_eq!(unpersist_tips(&[tip.clone(), tip.clone()]).unwrap(), 2);
		assert_eq!(root_count(&tip), 0);
	}

	#[test]
	fn undecodable_batch_is_leak_not_error() {
		// Ensure the in-memory storage is initialized.
		let _ = default_storage::<DefaultDB>();
		// Undecodable tips with initialized storage must be a no-op (leak),
		// not an error — an error would make the worker re-bind and retry the
		// same batch forever (wedged reclaim).
		assert_eq!(unpersist_tips(&[vec![1, 2, 3]]).unwrap(), 0);
	}
}

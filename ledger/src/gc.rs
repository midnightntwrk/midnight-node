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

//! Reclaim hash-tagged Anchored wrappers and run incremental arena GC.

use std::time::Duration;

use ledger_storage_ledger_8::{
	DefaultDB,
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::{default_storage, try_get_default_storage},
};
use midnight_primitives_ledger::LedgerStorageExt;

use crate::ledger_9;

type DbSeparate = ParityDb;
type DbUnified = ParityDb<sha2::Sha256, OwnedDb, { LedgerStorageExt::COLUMN_OFFSET }>;

/// Advisory: whether the initialized ledger backend's write cache is empty.
///
/// The GC worker checks this before reclaim/sweep. Purely advisory — the
/// authoritative check runs under the backend lock inside
/// [`release_tagged_tips`] / [`collect_garbage`]. Returns `true` when no
/// storage is initialized (the reclaim path will then fail with its own error).
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

/// Tags of currently persisted wrapper roots (block hashes).
pub fn tagged_root_tags() -> Vec<Vec<u8>> {
	fn tags_in<D: DB + 'static>() -> Option<Vec<Vec<u8>>> {
		if try_get_default_storage::<D>().is_none() {
			return None;
		}
		Some(
			ledger_9::tagged_roots(&default_storage::<D>().arena)
				.into_iter()
				.map(|(_, tag)| tag)
				.collect(),
		)
	}
	tags_in::<DbSeparate>()
		.or_else(tags_in::<DbUnified>)
		.or_else(tags_in::<DefaultDB>)
		.unwrap_or_default()
}

/// Decrement each matching tagged wrapper once and flush, only when the write
/// cache is empty. Replay of the same tags is a no-op. A dirty cache returns
/// `Err` ("busy") without changing counts.
pub fn release_tagged_tips<M: AsRef<[u8]>>(tags: &[M]) -> Result<usize, String> {
	if tags.is_empty() {
		return Ok(0);
	}
	if let Some(r) = release_in::<DbSeparate, M>(tags) {
		return r;
	}
	if let Some(r) = release_in::<DbUnified, M>(tags) {
		return r;
	}
	if let Some(r) = release_in::<DefaultDB, M>(tags) {
		return r;
	}
	Err("ledger storage is not initialized".into())
}

fn release_in<D: DB + 'static, M: AsRef<[u8]>>(tags: &[M]) -> Option<Result<usize, String>> {
	if try_get_default_storage::<D>().is_none() {
		return None;
	}
	Some(
		ledger_9::release_tagged_if_quiescent(&default_storage::<D>().arena, tags)
			.map_err(|e| e.to_string()),
	)
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

fn gc_in<D: DB + 'static>(budget: Duration) -> Result<Option<usize>, String> {
	if try_get_default_storage::<D>().is_none() {
		return Ok(None);
	}
	let culled = default_storage::<D>().with_backend(|backend| {
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

	fn persist_tagged_ledger(network_id: &str, tag: &[u8]) -> (Vec<u8>, Vec<u8>) {
		use crate::ledger_9::api::Ledger;
		use mn_ledger_9::structure::LedgerState;

		let state: LedgerState<DefaultDB> = LedgerState::new(network_id);
		let inner = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		ledger_9::persist_tagged(&default_storage::<DefaultDB>().arena, tag.to_vec(), &inner);
		default_storage::<DefaultDB>().with_backend(|b| b.flush_all_changes_to_db());
		let mut tip = vec![];
		tagged_serialize(&inner.as_typed_key(), &mut tip).expect("tip serializes");
		(tag.to_vec(), tip)
	}

	fn pin_count(tip: &[u8]) -> u32 {
		use crate::ledger_9::api::Ledger;
		use ledger_storage_ledger_8::arena::{ArenaKey, TypedArenaKey};

		let typed: TypedArenaKey<Ledger<DefaultDB>, _> =
			midnight_serialize::tagged_deserialize(&mut &tip[..]).expect("tip decodes");
		let key: ArenaKey<_> = typed.into();
		ledger_9::tagged_pin_count(&default_storage::<DefaultDB>().arena, key.hash()).unwrap_or(0)
	}

	fn release_retrying(tags: &[Vec<u8>]) -> usize {
		for _ in 0..50 {
			match release_tagged_tips(tags) {
				Ok(n) => return n,
				Err(e) if e.contains("busy") => {
					std::thread::sleep(Duration::from_millis(10));
				},
				Err(e) => panic!("release_tagged_tips failed: {e}"),
			}
		}
		panic!("ledger write cache never became quiescent");
	}

	#[test]
	fn release_decrements_once_per_tag() {
		let (tag, tip) = persist_tagged_ledger("gc-once", &[1u8; 32]);
		assert_eq!(pin_count(&tip), 1);
		assert_eq!(release_retrying(std::slice::from_ref(&tag)), 1);
		assert_eq!(pin_count(&tip), 0);
		assert_eq!(release_retrying(std::slice::from_ref(&tag)), 0);
	}

	#[test]
	fn sibling_hashes_release_independently() {
		use crate::ledger_9::api::Ledger;
		use mn_ledger_9::structure::LedgerState;

		let state: LedgerState<DefaultDB> = LedgerState::new("gc-sib");
		let inner = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		let a = vec![1u8; 32];
		let b = vec![2u8; 32];
		ledger_9::persist_tagged(&default_storage::<DefaultDB>().arena, a.clone(), &inner);
		ledger_9::persist_tagged(&default_storage::<DefaultDB>().arena, b.clone(), &inner);
		default_storage::<DefaultDB>().with_backend(|be| be.flush_all_changes_to_db());

		let mut tip = vec![];
		tagged_serialize(&inner.as_typed_key(), &mut tip).unwrap();
		assert_eq!(pin_count(&tip), 2);
		assert_eq!(release_retrying(std::slice::from_ref(&a)), 1);
		assert_eq!(pin_count(&tip), 1);
		assert_eq!(release_retrying(std::slice::from_ref(&b)), 1);
		assert_eq!(pin_count(&tip), 0);
	}

	#[test]
	fn unknown_tag_is_noop() {
		let _ = default_storage::<DefaultDB>();
		assert_eq!(release_retrying(&[vec![1, 2, 3]]), 0);
	}

	#[test]
	fn tagged_root_tags_lists_wrappers() {
		let (tag, _) = persist_tagged_ledger("gc-list", &[9u8; 32]);
		assert!(tagged_root_tags().iter().any(|t| t == &tag));
		assert_eq!(release_retrying(std::slice::from_ref(&tag)), 1);
	}
}

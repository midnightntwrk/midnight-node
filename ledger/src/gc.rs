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

//! Reclaim number-tagged Anchored wrappers and run incremental arena GC.

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

/// Tags of currently persisted wrapper roots (block numbers, little-endian).
pub fn tagged_root_tags() -> Vec<Vec<u8>> {
	fn tags_in<D: DB + 'static>() -> Option<Vec<Vec<u8>>> {
		try_get_default_storage::<D>()?;
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

/// Decrement each matching tagged wrapper once. Does not flush — durability is
/// the next block-boundary `flush_storage`. Replay of the same tags is a
/// no-op. Returns 0 when ledger storage is not initialized.
pub fn release_tagged_tips<M: AsRef<[u8]>>(tags: &[M]) -> usize {
	if tags.is_empty() {
		return 0;
	}
	if let Some(n) = release_in::<DbSeparate, M>(tags) {
		return n;
	}
	if let Some(n) = release_in::<DbUnified, M>(tags) {
		return n;
	}
	if let Some(n) = release_in::<DefaultDB, M>(tags) {
		return n;
	}
	0
}

fn release_in<D: DB + 'static, M: AsRef<[u8]>>(tags: &[M]) -> Option<usize> {
	try_get_default_storage::<D>()?;
	Some(ledger_9::release_tagged(&default_storage::<D>().arena, tags))
}

/// Run incremental arena GC for up to `budget`. Returns culled node count.
///
/// Sweeps only when the ledger write cache is empty: mark/sweep reachability
/// is DB-based, so staged-but-unflushed state is invisible to it. A dirty
/// cache returns `0`.
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

	#[test]
	fn release_decrements_once_per_tag() {
		let (tag, tip) = persist_tagged_ledger("gc-once", &101u32.to_le_bytes());
		assert_eq!(pin_count(&tip), 1);
		assert_eq!(release_tagged_tips(std::slice::from_ref(&tag)), 1);
		assert_eq!(pin_count(&tip), 0);
		assert_eq!(release_tagged_tips(std::slice::from_ref(&tag)), 0);
	}

	#[test]
	fn sibling_heights_release_independently() {
		use crate::ledger_9::api::Ledger;
		use mn_ledger_9::structure::LedgerState;

		let state: LedgerState<DefaultDB> = LedgerState::new("gc-sib");
		let inner = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		let a = 201u32.to_le_bytes().to_vec();
		let b = 202u32.to_le_bytes().to_vec();
		ledger_9::persist_tagged(&default_storage::<DefaultDB>().arena, a.clone(), &inner);
		ledger_9::persist_tagged(&default_storage::<DefaultDB>().arena, b.clone(), &inner);
		default_storage::<DefaultDB>().with_backend(|be| be.flush_all_changes_to_db());

		let mut tip = vec![];
		tagged_serialize(&inner.as_typed_key(), &mut tip).unwrap();
		assert_eq!(pin_count(&tip), 2);
		assert_eq!(release_tagged_tips(std::slice::from_ref(&a)), 1);
		assert_eq!(pin_count(&tip), 1);
		assert_eq!(release_tagged_tips(std::slice::from_ref(&b)), 1);
		assert_eq!(pin_count(&tip), 0);
	}

	#[test]
	fn unknown_tag_is_noop() {
		let _ = default_storage::<DefaultDB>();
		assert_eq!(release_tagged_tips(&[vec![1, 2, 3]]), 0);
	}

	#[test]
	fn tagged_root_tags_lists_wrappers() {
		let (tag, _) = persist_tagged_ledger("gc-list", &9u32.to_le_bytes());
		assert!(tagged_root_tags().iter().any(|t| t == &tag));
		assert_eq!(release_tagged_tips(std::slice::from_ref(&tag)), 1);
	}
}

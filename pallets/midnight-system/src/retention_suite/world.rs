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

//! The storage each world runs against, and what the properties read off it.

use midnight_node_ledger::{
	ephemeral::EphemeralDb,
	ledger_9::suite::arena_storage::{
		DefaultDB,
		arena::{ArenaHash, ArenaKey},
		db::DB,
		storage::{
			DEFAULT_CACHE_SIZE, Storage, default_storage, set_default_storage,
			unsafe_drop_default_storage,
		},
	},
	suite_storage::SuiteDb,
};
use std::{
	collections::HashSet,
	sync::{Mutex, MutexGuard, PoisonError},
};

pub type Hash = ArenaHash<<DefaultDB as DB>::Hasher>;
pub type Root = ArenaKey<<DefaultDB as DB>::Hasher>;

/// Holds the worlds for one history at a time: they are process-wide.
pub fn lock() -> MutexGuard<'static, ()> {
	static LOCK: Mutex<()> = Mutex::new(());
	LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Gives world `W` an empty storage, and an off-chain storage reading through it. Returns its
/// disk.
pub fn reset<const W: u8>() -> DefaultDB {
	let disk = DefaultDB::default();
	unsafe_drop_default_storage::<SuiteDb<W>>();
	unsafe_drop_default_storage::<EphemeralDb<SuiteDb<W>>>();
	let db = SuiteDb::<W>(disk.clone());
	let set = set_default_storage(|| Storage::new(DEFAULT_CACHE_SIZE, db)).is_ok()
		&& set_default_storage(|| {
			Storage::new(DEFAULT_CACHE_SIZE, EphemeralDb::<SuiteDb<W>>::default())
		})
		.is_ok();
	assert!(set, "the lock keeps other cases out");
	disk
}

/// Adds the nodes under `root` that `D`'s storage holds to `found`, and returns whether it holds
/// all of them. Nodes already in `found` are not walked again, which saves walking the subtrees
/// states share. `get_ledger` reads only the root node; a missing descendant panics on first
/// deref instead.
pub fn walk<D: DB<Hasher = <DefaultDB as DB>::Hasher>>(
	root: &Root,
	found: &mut HashSet<Hash>,
) -> bool {
	default_storage::<D>().with_backend(|backend| {
		let mut stack: Vec<Hash> = root.refs().into_iter().cloned().collect();
		let mut all = true;
		while let Some(hash) = stack.pop() {
			if found.contains(&hash) {
				continue;
			}
			match backend.get(&hash) {
				Some(object) => {
					stack.extend(object.children.iter().flat_map(|child| child.refs()).cloned());
					found.insert(hash);
				},
				None => all = false,
			}
		}
		all
	})
}

/// The nodes under `released` that `D`'s storage still holds and no root in `live` reaches. A
/// state shares nodes with live ones, and an inline root has no node of its own, so whether a
/// released key still resolves says nothing: its own nodes must go.
pub fn leftover<D: DB<Hasher = <DefaultDB as DB>::Hasher>>(
	live: &[Root],
	released: &[Root],
) -> Vec<Hash> {
	let mut found = HashSet::new();
	for root in live {
		walk::<D>(root, &mut found);
	}
	let live = found.clone();
	for root in released {
		walk::<D>(root, &mut found);
	}
	found.into_iter().filter(|hash| !live.contains(hash)).collect()
}

/// The nodes on `disk` that no root on it reaches.
pub fn unreachable(disk: &DefaultDB) -> Vec<Hash> {
	let mut reached = HashSet::new();
	let mut stack: Vec<Hash> = disk.get_roots().into_keys().collect();
	while let Some(hash) = stack.pop() {
		if reached.insert(hash.clone())
			&& let Some(object) = disk.get_node(&hash)
		{
			stack.extend(object.children.iter().flat_map(|child| child.refs()).cloned());
		}
	}
	let nodes = disk.scan(None, usize::MAX).0;
	nodes
		.into_iter()
		.map(|(hash, _)| hash)
		.filter(|hash| !reached.contains(hash))
		.collect()
}

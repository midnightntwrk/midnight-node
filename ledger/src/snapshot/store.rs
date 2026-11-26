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

//! [`DbSnapshot`]: the arena's database in snapshot mode, a handle to the process-wide current
//! store (Midnight's `ParityDb` over a snapshot directory), which a snapshot replaces.
//!
//! The store lock is always taken last: inside the arena's backend lock, never around it.

use super::{Hash, Hasher, Node, StoreDb};
use midnight_storage_core::db::{
	DB, DummyArbitrary, Update,
	paritydb::{NODE_COLUMN, NUM_COLUMNS, OwnedDb, set_init_options},
};
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	sync::{Arc, OnceLock, RwLock},
};

pub struct Store {
	db: RwLock<StoreDb>,
	raw: Arc<parity_db::Db>,
	dir: PathBuf,
}

impl Store {
	/// Open or create the store in `dir`.
	pub fn open(dir: &Path) -> Result<Self, String> {
		let mut options = parity_db::Options::with_columns(dir, NUM_COLUMNS);
		set_init_options(&mut options, 0, false);
		let db = parity_db::Db::open_or_create(&options)
			.map_err(|e| format!("open {}: {e}", dir.display()))?;
		let raw = Arc::new(db);
		let db = StoreDb::from_existing_db(OwnedDb(raw.clone()));
		Ok(Self { db: RwLock::new(db), raw, dir: dir.to_path_buf() })
	}

	pub(crate) fn dir(&self) -> &Path {
		&self.dir
	}

	pub fn get_node(&self, key: &Hash) -> Option<Node> {
		self.db.read().unwrap_or_else(|e| e.into_inner()).get_node(key)
	}

	/// Whether the store holds `key`, without reading the node; `false` if that cannot be told.
	pub(crate) fn contains(&self, key: &Hash) -> bool {
		self.raw.get_size(NODE_COLUMN, &key.0).is_ok_and(|size| size.is_some())
	}

	pub fn batch_update(&self, iter: impl Iterator<Item = (Hash, Update<Hasher>)>) {
		self.db.write().unwrap_or_else(|e| e.into_inner()).batch_update(iter)
	}
}

static CURRENT: RwLock<Option<Store>> = RwLock::new(None);

/// Make `store` the current store; returns the one it replaces.
pub(super) fn install(store: Store) -> Option<Store> {
	CURRENT.write().unwrap_or_else(|e| e.into_inner()).replace(store)
}

/// A node from the current store.
pub fn get_node(key: &Hash) -> Option<Node> {
	with(|store| store.get_node(key))
}

/// The arena only exists once [`super::init`] installed a store, which is never removed.
fn with<R>(f: impl FnOnce(&Store) -> R) -> R {
	let current = CURRENT.read().unwrap_or_else(|e| e.into_inner());
	f(current
		.as_ref()
		.expect("snapshot mode installs its store before creating its arena"))
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DbSnapshot;

impl DummyArbitrary for DbSnapshot {}

/// The `i`th key of the read-cache filler: keys whose 24-byte prefix no SHA-256 node hash has
/// in practice, each standing for an empty node. Reading `storage_cache_size` of them pushes
/// every other node out of the arena's read cache.
pub(super) fn filler(i: u64) -> Hash {
	let mut key = [0xff; 32];
	key[24..].copy_from_slice(&i.to_be_bytes());
	midnight_storage_core::arena::ArenaHash(key.into())
}

fn empty_node() -> Node {
	static EMPTY: OnceLock<Node> = OnceLock::new();
	EMPTY
		.get_or_init(|| {
			use midnight_serialize::{Deserializable, Serializable};
			let mut bytes = Vec::new();
			Vec::<u8>::new().serialize(&mut bytes).expect("serialize to memory");
			Vec::<super::Key>::new().serialize(&mut bytes).expect("serialize to memory");
			Node::deserialize(&mut &bytes[..], 0).expect("an empty node")
		})
		.clone()
}

impl DB for DbSnapshot {
	type Hasher = Hasher;
	type ScanResumeHandle = ();

	fn get_node(&self, key: &Hash) -> Option<Node> {
		if key.0[..24] == [0xff; 24] {
			return Some(empty_node());
		}
		get_node(key)
	}

	fn insert_node(&mut self, key: Hash, object: Node) {
		with(|store| store.batch_update(std::iter::once((key, Update::InsertNode(object)))))
	}

	/// Snapshot mode never runs the garbage collector, the only caller.
	fn delete_node(&mut self, _key: &Hash) {}

	fn batch_update<I>(&mut self, iter: I)
	where
		I: Iterator<Item = (Hash, Update<Self::Hasher>)>,
	{
		// The arena flushes only when a snapshot switches stores, and then rewrites the nodes the
		// snapshot just copied: those are skipped. Root counts are not kept, and nodes are only
		// deleted by the garbage collector, which never runs.
		with(|store| {
			let inserts: Vec<_> = iter
				.filter(|(key, update)| {
					matches!(update, Update::InsertNode(_)) && !store.contains(key)
				})
				.collect();
			store.batch_update(inserts.into_iter())
		})
	}

	fn batch_get_nodes<I>(&self, keys: I) -> Vec<(Hash, Option<Node>)>
	where
		I: Iterator<Item = Hash>,
	{
		with(|store| {
			keys.map(|key| {
				let node = store.get_node(&key);
				(key, node)
			})
			.collect()
		})
	}

	/// Snapshot stores keep no root counts, as no garbage collector runs on them. The arena
	/// still adds its pending root count changes to the database's count when it flushes, so a
	/// count is read as a fixed base: a change can then never take it below zero, whichever
	/// store the count it reverses was flushed to.
	fn get_root_count(&self, _key: &Hash) -> u32 {
		1 << 30
	}

	fn set_root_count(&mut self, _key: Hash, _count: u32) {}

	fn get_roots(&self) -> HashMap<Hash, u32> {
		HashMap::new()
	}

	fn size(&self) -> usize {
		with(|store| store.db.read().unwrap_or_else(|e| e.into_inner()).size())
	}

	/// Snapshot mode never runs the garbage collector.
	fn scan(
		&self,
		_resume_from: Option<()>,
		_batch_size: usize,
	) -> (Vec<(Hash, Node)>, Option<()>) {
		(Vec::new(), None)
	}
}

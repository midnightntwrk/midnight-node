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

//! The storage arena behind every ledger object the toolkit builds.
//!
//! [`ToolkitDB`] is the `DefaultDB` of both ledger generations in this crate. It is
//! either in memory (the default, the same as the storage crate's `InMemoryDB`) or a
//! ParityDB on disk. The in-memory arena is installed when the process starts, so any
//! code can build ledger objects without setup; [`install_persistent_storage`] swaps
//! it for the on-disk one, but only while it is still empty.

use ledger_storage_ledger_8::{
	DefaultHasher, Storage,
	arena::ArenaHash,
	backend::OnDiskObject,
	db::{DB, DummyArbitrary, InMemoryDB, ParityDb, Update, paritydb::OwnedDb},
	storage::{set_default_storage, try_get_default_storage},
};
use std::{
	collections::HashMap,
	fmt,
	path::Path,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
};

/// Objects kept in the read cache of an on-disk arena. The cache holds encoded
/// nodes, a few hundred bytes each, so this bounds it to tens of MiB.
pub const PERSISTENT_CACHE_SIZE: usize = 200_000;

/// The arena database: the in-memory DB, or a ParityDB that outlives the process.
pub enum ToolkitDB {
	Memory(InMemoryDB<DefaultHasher>),
	/// `ParityDb` keeps its handle private, so the variant keeps a second one for `Clone`.
	Disk(ParityDb<DefaultHasher, OwnedDb>, Arc<parity_db::Db>),
}

impl ToolkitDB {
	/// Open (or create) a ParityDB in `dir`. ParityDB allows one process per
	/// directory; the caller must hold a lock that guarantees it.
	pub fn open_disk(dir: &Path) -> std::io::Result<Self> {
		std::fs::create_dir_all(dir)?;
		let owned = OwnedDb::new(dir);
		let handle = owned.0.clone();
		Ok(Self::Disk(ParityDb::from_existing_db(owned), handle))
	}
}

impl Default for ToolkitDB {
	fn default() -> Self {
		Self::Memory(InMemoryDB::default())
	}
}

impl Clone for ToolkitDB {
	fn clone(&self) -> Self {
		match self {
			Self::Memory(db) => Self::Memory(db.clone()),
			// `ParityDb` is not `Clone`; share the same open database instead.
			Self::Disk(_, handle) => {
				Self::Disk(ParityDb::from_existing_db(OwnedDb(handle.clone())), handle.clone())
			},
		}
	}
}

impl fmt::Debug for ToolkitDB {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Memory(_) => f.write_str("ToolkitDB::Memory"),
			Self::Disk(..) => f.write_str("ToolkitDB::Disk"),
		}
	}
}

impl DummyArbitrary for ToolkitDB {}

/// Resume point of a GC sweep, in the form of whichever DB is in use.
#[derive(Debug, Clone)]
pub enum ScanResume {
	Memory(<InMemoryDB<DefaultHasher> as DB>::ScanResumeHandle),
	Disk(<ParityDb<DefaultHasher, OwnedDb> as DB>::ScanResumeHandle),
}

type Hash = ArenaHash<DefaultHasher>;
type Node = OnDiskObject<DefaultHasher>;

macro_rules! dispatch {
	($self:expr, $db:ident => $body:expr) => {
		match $self {
			ToolkitDB::Memory($db) => $body,
			ToolkitDB::Disk($db, _) => $body,
		}
	};
}

impl DB for ToolkitDB {
	type Hasher = DefaultHasher;
	type ScanResumeHandle = ScanResume;

	fn get_node(&self, key: &Hash) -> Option<Node> {
		dispatch!(self, db => db.get_node(key))
	}

	fn insert_node(&mut self, key: Hash, object: Node) {
		dispatch!(self, db => db.insert_node(key, object))
	}

	fn delete_node(&mut self, key: &Hash) {
		dispatch!(self, db => db.delete_node(key))
	}

	fn batch_update<I>(&mut self, iter: I)
	where
		I: Iterator<Item = (Hash, Update<DefaultHasher>)>,
	{
		dispatch!(self, db => db.batch_update(iter))
	}

	fn batch_get_nodes<I>(&self, keys: I) -> Vec<(Hash, Option<Node>)>
	where
		I: Iterator<Item = Hash>,
	{
		dispatch!(self, db => db.batch_get_nodes(keys))
	}

	fn bfs_get_nodes<C>(
		&self,
		key: &Hash,
		cache_get: C,
		truncate: bool,
		max_depth: Option<usize>,
		max_count: Option<usize>,
	) -> Vec<(Hash, Node)>
	where
		C: Fn(&Hash) -> Option<Node>,
	{
		dispatch!(self, db => db.bfs_get_nodes(key, cache_get, truncate, max_depth, max_count))
	}

	fn get_root_count(&self, key: &Hash) -> u32 {
		dispatch!(self, db => db.get_root_count(key))
	}

	fn set_root_count(&mut self, key: Hash, count: u32) {
		dispatch!(self, db => db.set_root_count(key, count))
	}

	fn get_roots(&self) -> HashMap<Hash, u32> {
		dispatch!(self, db => db.get_roots())
	}

	fn size(&self) -> usize {
		dispatch!(self, db => db.size())
	}

	fn scan(
		&self,
		resume_from: Option<ScanResume>,
		batch_size: usize,
	) -> (Vec<(Hash, Node)>, Option<ScanResume>) {
		match self {
			Self::Memory(db) => {
				let resume = resume_from.map(|r| match r {
					ScanResume::Memory(h) => h,
					ScanResume::Disk(_) => panic!("disk scan handle passed to the in-memory DB"),
				});
				let (nodes, next) = db.scan(resume, batch_size);
				(nodes, next.map(ScanResume::Memory))
			},
			Self::Disk(db, _) => {
				let resume = resume_from.map(|r| match r {
					ScanResume::Disk(h) => h,
					ScanResume::Memory(_) => panic!("in-memory scan handle passed to the disk DB"),
				});
				let (nodes, next) = db.scan(resume, batch_size);
				(nodes, next.map(ScanResume::Disk))
			},
		}
	}
}

/// The process-wide arena, if one is installed.
pub fn try_default_storage() -> Option<Arc<Storage<ToolkitDB>>> {
	try_get_default_storage::<ToolkitDB>()
}

/// Install the in-memory arena unless an arena is already installed.
pub fn ensure_default_storage() {
	if try_default_storage().is_none() {
		let _ = set_default_storage(Storage::<ToolkitDB>::default);
	}
}

// The storage crate only creates a default storage on demand for its own `InMemoryDB`;
// for any other `DB` an unset default panics. Installing it before `main` (and before
// every test) means no entry point can forget to.
#[ctor::ctor]
fn install_in_memory_storage_at_startup() {
	ensure_default_storage();
}

/// Roots taken only to keep nodes reachable until they are on disk (see
/// [`persist_until_flushed`]).
static TRANSIENT_ROOTS: std::sync::Mutex<Vec<ArenaHash<DefaultHasher>>> =
	std::sync::Mutex::new(Vec::new());

/// Persist `sp` so its nodes survive until the next flush, after which
/// [`release_flushed_roots`] drops the root again. For a lazy `Sp` re-typed from an
/// eager one (the 8 -> 9 fork does this): its nodes must stay in the backend until
/// they are written, but should not stay GC roots for ever.
pub fn persist_until_flushed<T: ledger_storage_ledger_8::Storable<ToolkitDB>>(
	sp: &mut ledger_storage_ledger_8::arena::Sp<T, ToolkitDB>,
) {
	sp.persist();
	TRANSIENT_ROOTS
		.lock()
		.unwrap_or_else(|e| e.into_inner())
		.extend(sp.as_child().refs().into_iter().cloned());
}

/// Release the roots taken by [`persist_until_flushed`]. Call only right after a
/// flush wrote every pending node to the database.
pub fn release_flushed_roots() {
	let roots = std::mem::take(&mut *TRANSIENT_ROOTS.lock().unwrap_or_else(|e| e.into_inner()));
	if roots.is_empty() {
		return;
	}
	if let Some(storage) = try_default_storage() {
		storage.arena.with_backend(|b| roots.iter().for_each(|h| b.unpersist(h)));
	}
}

/// Set once the on-disk arena is installed; the backend does not expose its DB.
static PERSISTENT: AtomicBool = AtomicBool::new(false);

/// Why [`install_persistent_storage`] did not install the on-disk arena.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
	#[error(
		"a ledger arena is already installed in this process (persistent: {persistent}); it is chosen once per process"
	)]
	AlreadyInstalled { persistent: bool },
	#[error("cannot open the ledger arena database: {0}")]
	Open(#[from] std::io::Error),
}

/// Install a ParityDB in `dir` as the process-wide arena, replacing the start-up
/// in-memory one. Fails once the installed arena holds any object (or is already on
/// disk): objects keep a handle to the arena they were built in, and mixing them into
/// the on-disk state would leave nodes that never reach the database.
pub fn install_persistent_storage(dir: &Path) -> Result<Arc<Storage<ToolkitDB>>, InstallError> {
	static SWAP: std::sync::Mutex<()> = std::sync::Mutex::new(());
	let _guard = SWAP.lock().unwrap_or_else(|e| e.into_inner());
	// Objects up to the storage crate's small-object limit are stored inline in their
	// parent and carry their own data; only larger ones are tracked by the arena.
	if is_persistent() || try_default_storage().is_some_and(|s| s.arena.size() > 0) {
		return Err(InstallError::AlreadyInstalled { persistent: is_persistent() });
	}
	// Open before dropping the in-memory arena, so a failed open leaves it in place.
	let db = ToolkitDB::open_disk(dir)?;
	ledger_storage_ledger_8::storage::unsafe_drop_default_storage::<ToolkitDB>();
	let storage = set_default_storage(|| Storage::new(PERSISTENT_CACHE_SIZE, db))
		.map_err(|_| InstallError::AlreadyInstalled { persistent: is_persistent() })?;
	PERSISTENT.store(true, Ordering::SeqCst);
	Ok(storage)
}

/// Whether the installed arena is on disk.
pub fn is_persistent() -> bool {
	PERSISTENT.load(Ordering::SeqCst)
}

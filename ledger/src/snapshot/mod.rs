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

//! Snapshot ledger storage: the ledger states of recent blocks on disk as of the last snapshot,
//! newer states in memory.
//!
//! Opt-in: nothing here runs, and no store is installed, unless the node calls [`init`].
//!
//! Block execution puts each post-block state in the [`window`] instead of persisting it, and
//! `flush_storage` does nothing. The node evicts window states and calls [`take_snapshot`],
//! which copies every window state into a fresh store and switches the arena to it. A call
//! reading a window state pins it against eviction until the call ends.
//!
//! # Why a switch is safe
//!
//! The arena treats a node it holds as read (or as only reference-counted) as already in its
//! database, and never writes it. Replacing the database must therefore leave every such node
//! readable. The switch runs while no runtime call is running ([`call_guard`]) and no block is
//! being authored ([`BlockInProgress`]), so the only live states are the window's; it copies
//! them into the new store, flushes the arena's pending nodes into it, then reads
//! `storage_cache_size` filler keys (see [`store::filler`]) so its read cache keeps no node of the
//! old store. The off-chain arena reads through this one and also keeps what it read as stored,
//! so its read cache is pushed out the same way; that refill reaches this arena only for its own
//! misses, so it does not replace this one's. Off-chain objects (cached verified transactions,
//! states runtime-API instances hold) stay valid in the off-chain arena, which is never replaced,
//! as they can be dropped at any time; the switch empties the verified-transaction caches and
//! stops off-chain reads of the states off-chain calls produced. Only then is the old store
//! deleted.

mod marker;
mod store;
pub mod window;
mod writer;

pub use marker::Marker;
pub(crate) use store::DbSnapshot;

use crate::ephemeral::EphemeralDb;
use midnight_storage_core::{
	Storable, Storage,
	arena::{ArenaHash, ArenaKey, Sp},
	backend::{OnDiskObject, StorageBackend},
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::{default_storage, set_default_storage},
};
use std::{
	cell::{Cell, RefCell},
	collections::HashSet,
	path::{Path, PathBuf},
	sync::{
		Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
		TryLockError,
		atomic::{AtomicBool, Ordering},
	},
	time::Instant,
};

pub(crate) const LOG_TARGET: &str = "midnight::ledger::snapshot";

pub(crate) type Hasher = storage_sha2::Sha256;
pub(crate) type Hash = ArenaHash<Hasher>;
pub(crate) type Key = ArenaKey<Hasher>;
pub(crate) type Node = OnDiskObject<Hasher>;
pub(crate) type StoreDb = ParityDb<Hasher>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|e| e.into_inner())
}

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether snapshot mode runs in this process.
pub(crate) fn is_active() -> bool {
	ACTIVE.load(Ordering::Acquire)
}

/// The arena key a `StateKey` (a tagged `TypedArenaKey`) holds, for any ledger version.
fn root_of_state_key(state_key: &[u8]) -> Result<Key, String> {
	use midnight_serialize::Deserializable;
	let (_, mut body) =
		crate::boundary::types::split_tag(state_key).ok_or("StateKey has no tag")?;
	let key = <Key as Deserializable>::deserialize(&mut body, 0)
		.map_err(|e| format!("StateKey body is not an arena key: {e}"))?;
	if !body.is_empty() {
		return Err("trailing bytes in StateKey".into());
	}
	Ok(key)
}

static SWITCH: RwLock<()> = RwLock::new(());

thread_local! {
	static CALL_DEPTH: Cell<usize> = const { Cell::new(0) };
	/// The window states the running call reads, pinned until it ends.
	static CALL_PINS: RefCell<Vec<window::Pinned>> = const { RefCell::new(Vec::new()) };
}

/// Held by every runtime call and every direct ledger read while it runs, so a snapshot
/// switches stores only between them, and the window states it reads stay until it ends.
/// Nested calls on one thread share the outermost guard.
pub struct CallGuard {
	_read: Option<RwLockReadGuard<'static, ()>>,
}

impl Drop for CallGuard {
	fn drop(&mut self) {
		let outermost = CALL_DEPTH.with(|d| {
			d.set(d.get() - 1);
			d.get() == 0
		});
		if outermost {
			drop(CALL_PINS.take());
		}
	}
}

/// A [`CallGuard`] in snapshot mode, `None` otherwise.
pub fn call_guard() -> Option<CallGuard> {
	if !is_active() {
		return None;
	}
	let outermost = CALL_DEPTH.with(|d| {
		d.set(d.get() + 1);
		d.get() == 1
	});
	Some(CallGuard { _read: outermost.then(|| SWITCH.read().unwrap_or_else(|e| e.into_inner())) })
}

/// For a direct read of `state_key` (not a runtime call): in snapshot mode, refuse a state outside
/// the window and hold off a switch for as long as the returned guard lives.
pub(crate) fn read_guard(state_key: &[u8]) -> Result<Option<CallGuard>, String> {
	let call = call_guard();
	if call.is_some() && !readable(state_key) {
		return Err("ledger state is outside the snapshot-mode window".into());
	}
	Ok(call)
}

static BLOCKS_IN_PROGRESS: Mutex<usize> = Mutex::new(0);
static NO_BLOCK_IN_PROGRESS: Condvar = Condvar::new();

/// Held with each intermediate state of a block being executed. Block authoring holds them
/// between runtime calls; a snapshot switches stores only once none is held.
pub(crate) struct BlockInProgress(());

impl BlockInProgress {
	pub(crate) fn start() -> Self {
		*lock(&BLOCKS_IN_PROGRESS) += 1;
		Self(())
	}
}

impl Drop for BlockInProgress {
	fn drop(&mut self) {
		let mut blocks = lock(&BLOCKS_IN_PROGRESS);
		*blocks -= 1;
		if *blocks == 0 {
			NO_BLOCK_IN_PROGRESS.notify_all();
		}
	}
}

/// Exclusive access for a switch: no runtime call running, no block in progress. Intermediate
/// states are only created inside runtime calls, so none can appear while this is held.
///
/// It waits for the blocks in progress to end before it asks for the lock: a writer waiting for
/// the lock holds up every new call, the author's next one included, until the calls running
/// finish, however long they take. Returns when it last asked for the lock, from which new calls
/// are held up.
fn exclusive() -> (RwLockWriteGuard<'static, ()>, Instant) {
	loop {
		drop(
			NO_BLOCK_IN_PROGRESS
				.wait_while(lock(&BLOCKS_IN_PROGRESS), |blocks| *blocks > 0)
				.unwrap_or_else(|e| e.into_inner()),
		);
		let asked = Instant::now();
		let guard = SWITCH.write().unwrap_or_else(|e| e.into_inner());
		// A call that was running may have started a block meanwhile.
		if *lock(&BLOCKS_IN_PROGRESS) == 0 {
			return (guard, asked);
		}
	}
}

/// `StateKey`s of the states off-chain calls produced, which their runtime-API instances hold in
/// the off-chain arena until they are dropped.
static OFFCHAIN_STATES: Mutex<Option<HashSet<Vec<u8>>>> = Mutex::new(None);

/// Let off-chain calls read `state_key`, a state an off-chain call produced, until the next switch.
/// A state its holder dropped meanwhile is refused by the read itself: its root is gone.
pub(crate) fn produced_offchain(state_key: &[u8]) {
	lock(&OFFCHAIN_STATES)
		.get_or_insert_with(HashSet::new)
		.insert(state_key.to_vec());
}

/// Whether a state may be read outside block execution: a window state, or one an off-chain call
/// produced since the last switch. Anything else fails up front with `NoLedgerState`.
fn is_queryable(state_key: &[u8]) -> bool {
	window::contains(state_key)
		|| lock(&OFFCHAIN_STATES).as_ref().is_some_and(|states| states.contains(state_key))
}

/// [`is_queryable`], pinning a window state against eviction until the running call ends: its
/// nodes are read one by one as the call goes. Outside a call nothing is pinned.
fn readable(state_key: &[u8]) -> bool {
	if CALL_DEPTH.with(Cell::get) == 0 {
		return is_queryable(state_key);
	}
	let pinned = CALL_PINS.with_borrow_mut(|pins| {
		if pins.iter().any(|p| p.0.iter().any(|k| k == state_key)) {
			return true;
		}
		let pin = window::pin(vec![state_key.to_vec()]);
		let held = !pin.0.is_empty();
		if held {
			pins.push(pin);
		}
		held
	});
	pinned || lock(&OFFCHAIN_STATES).as_ref().is_some_and(|states| states.contains(state_key))
}

/// Whether a call over the database `D` may read `state_key` (see [`readable`]). Block execution
/// (over [`DbSnapshot`]) reads any state it is given; off-chain calls refuse the others.
pub(crate) fn admits<D: 'static>(state_key: &[u8]) -> bool {
	!is_active()
		|| readable(state_key)
		|| std::any::TypeId::of::<D>() == std::any::TypeId::of::<DbSnapshot>()
}

/// The window states the running call pinned, pinned again for as long as the returned value
/// lives: a state an off-chain call produced shares the nodes it did not change with them.
pub(crate) fn pin_call_reads() -> window::Pinned {
	window::pin(CALL_PINS.with_borrow(|pins| pins.iter().flat_map(|p| p.0.clone()).collect()))
}

/// Hold a new post-block state in the window instead of persisting it. Tracked, so the arena
/// keeps every node it reaches, as `persist` does; unloaded, so the window keeps those nodes and
/// not the data executing the block loaded from them, which the next block loads again as it
/// would from a persisted state.
pub(crate) fn hold<T: Storable<D>, D: DB>(state_key: Vec<u8>, state: &Sp<T, D>)
where
	Sp<T, D>: Send,
{
	let mut handle = state.into_tracked();
	handle.unload();
	window::insert(state_key, Box::new(handle));
}

/// Forget every verified transaction both ledger versions cache: they hold ledger nodes, and
/// those read from a replaced store would be taken as stored.
pub(crate) fn forget_verified_transactions() {
	forget_entries(&crate::ledger_8::STRICT_TX_VALIDATION_CACHE);
	forget_entries(&crate::ledger_9::STRICT_TX_VALIDATION_CACHE);
}

/// Entry by entry, then `sync`, which drops every value, and the nodes it holds, here. Not
/// `invalidate_all`: it only hides the entries, a `sync` then drops a bounded batch of them (with
/// 600 entries about 100 values stayed alive), and it spares an entry stamped in the same tick of
/// the cache's clock.
fn forget_entries<K, V>(cache: &moka::sync::Cache<K, V>)
where
	K: std::hash::Hash + Eq + Send + Sync + 'static,
	V: Clone + Send + Sync + 'static,
{
	use moka::sync::ConcurrentCacheExt;
	for (key, _) in cache.iter() {
		cache.invalidate(&*key);
	}
	cache.sync();
}

struct Settings {
	root: PathBuf,
	/// The arenas' read cache capacity.
	cache_size: usize,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();
static CURRENT_MARKER: Mutex<Option<Marker>> = Mutex::new(None);
static SNAPSHOT_RUNNING: Mutex<()> = Mutex::new(());
static REQUESTED: AtomicBool = AtomicBool::new(false);

fn settings() -> Result<&'static Settings, String> {
	SETTINGS.get().ok_or_else(|| "snapshot mode is not initialised".into())
}

/// The marker of the snapshot the arena uses.
pub fn current_marker() -> Option<Marker> {
	lock(&CURRENT_MARKER).clone()
}

/// Record `marker` as the arena's snapshot; its state, on disk, is queryable.
fn set_current(marker: &Marker, state_key: &[u8]) {
	*lock(&CURRENT_MARKER) = Some(marker.clone());
	window::insert(state_key.to_vec(), Box::new(()));
}

/// Ask for a snapshot at the next finalized block.
pub fn request_snapshot() {
	REQUESTED.store(true, Ordering::Release);
}

pub fn is_requested() -> bool {
	REQUESTED.load(Ordering::Acquire)
}

/// Install the first store, the arena over it, which reads whichever store is current, and the
/// off-chain arena over that one. Neither arena is ever replaced.
fn install(store: store::Store, cache_size: usize) -> Result<(), String> {
	store::install(store);
	let initialised = "snapshot mode is already initialised";
	set_default_storage(|| Storage::new(cache_size, DbSnapshot)).map_err(|_| initialised)?;
	set_default_storage(|| Storage::new(cache_size, EphemeralDb::<DbSnapshot>::default()))
		.map_err(|_| initialised)?;
	ACTIVE.store(true, Ordering::Release);
	Ok(())
}

/// Open the store of `marker`, checking it holds the root of its state.
fn open_marked(root: &Path, marker: &Marker) -> Result<store::Store, String> {
	let store = store::Store::open(&marker.path(root))?;
	let state_key = marker.state_key_bytes()?;
	match root_of_state_key(&state_key)?
		.refs()
		.into_iter()
		.find(|h| store.get_node(h).is_none())
	{
		Some(missing) => Err(format!(
			"snapshot {} lacks node {} of its own state",
			marker.dir,
			hex::encode(missing.0)
		)),
		None => Ok(store),
	}
}

/// Remove a store directory; one left behind is removed at the next startup.
fn remove_store(dir: &Path) {
	if let Err(e) = std::fs::remove_dir_all(dir) {
		log::warn!(target: LOG_TARGET, "Failed to remove {}: {e}", dir.display());
	}
}

/// Open the marked snapshot under `root`, removing any other snapshot directory. With no
/// snapshot, seed one from `genesis_state`, unless `legacy` holds a database to convert: then
/// return `None` and leave that to [`convert_legacy`].
pub fn init(
	root: &Path,
	genesis_state: &[u8],
	legacy: Option<&Path>,
	cache_size: usize,
) -> Result<Option<Marker>, String> {
	marker::ensure_root(root)?;
	SETTINGS
		.set(Settings { root: root.to_path_buf(), cache_size })
		.map_err(|_| "snapshot mode is already initialised")?;
	let marker = marker::read(root)?;
	marker::clean(root, marker.as_ref())?;
	if let Some(marker) = marker {
		install(open_marked(root, &marker)?, cache_size)?;
		set_current(&marker, &marker.state_key_bytes()?);
		return Ok(Some(marker));
	}
	if legacy.is_some_and(|l| l.join("metadata").is_file()) {
		return Ok(None);
	}
	// The genesis state, written as the persistent layout writes it, into a store the snapshot
	// below copies and then deletes.
	install(store::Store::open(&root.join(marker::snapshot_dir_name(0, None)))?, cache_size)?;
	let state_key = if crate::ledger_8::storage::genesis_matches_this_version(genesis_state) {
		crate::ledger_8::storage::alloc_with_initial_state::<
			crate::ledger_8::TransactionSignature,
			DbSnapshot,
		>(genesis_state)
	} else {
		crate::ledger_9::storage::alloc_with_initial_state::<
			crate::ledger_9::TransactionSignature,
			DbSnapshot,
		>(genesis_state)
	};
	window::insert(state_key.clone(), Box::new(()));
	take_snapshot(&SnapshotRequest { block_number: 0, block_hash: None, state_key }).map(Some)
}

#[derive(Debug, Clone)]
pub struct SnapshotRequest {
	pub block_number: u64,
	/// `None` for genesis, whose hash is not known when its snapshot is written.
	pub block_hash: Option<Vec<u8>>,
	pub state_key: Vec<u8>,
}

/// Fill a new store at `dir` with `copy` and close it; on failure, remove it.
fn write_store(
	dir: &Path,
	copy: impl FnOnce(&store::Store) -> Result<(), String>,
) -> Result<(), String> {
	let copied = store::Store::open(dir).and_then(|target| copy(&target));
	let synced = copied.and_then(|()| marker::fsync_dir(dir));
	if synced.is_err() {
		remove_store(dir);
	}
	synced
}

/// Copy every node of the `pinned` window states into `target`, reading each from the current
/// store or, for those it lacks, with `in_memory`. A node `target` already holds is taken to come
/// with everything below it.
fn copy_window(
	pinned: &window::Pinned,
	target: &store::Store,
	in_memory: impl FnMut(&[Hash]) -> Vec<Option<Node>>,
) -> Result<(), String> {
	let roots = pinned.0.iter().map(|k| root_of_state_key(k)).collect::<Result<Vec<_>, _>>()?;
	writer::copy(&roots, store::get_node, in_memory, target)
}

fn from_memory(backend: &mut StorageBackend<DbSnapshot>, keys: &[Hash]) -> Vec<Option<Node>> {
	keys.iter().map(|k| backend.get(k).cloned()).collect()
}

/// Push every node out of an arena's read cache by reading `cache_size` filler keys.
fn refill<D: DB<Hasher = Hasher>>(backend: &mut StorageBackend<D>, cache_size: usize) {
	for i in 0..cache_size as u64 {
		backend.get(&store::filler(i));
	}
}

/// Turn a persistent-layout ledger database into the first snapshot: copy the state at the
/// request's block into a fresh store, mark it and open it. `legacy` is opened read-only;
/// parity-db still enacts a write-ahead log a crash left there, as any open does.
pub fn convert_legacy(legacy: &Path, request: &SnapshotRequest) -> Result<Marker, String> {
	let Settings { root, cache_size } = settings()?;
	let mut options =
		parity_db::Options::with_columns(legacy, midnight_storage_core::db::paritydb::NUM_COLUMNS);
	midnight_storage_core::db::paritydb::set_init_options(&mut options, 0, false);
	let db = parity_db::Db::open_read_only(&options)
		.map_err(|e| format!("open {} read-only: {e}", legacy.display()))?;
	let source = StoreDb::from_existing_db(OwnedDb(Arc::new(db)));
	let name = marker::snapshot_dir_name(request.block_number, request.block_hash.as_deref());
	let state_root = root_of_state_key(&request.state_key)?;
	write_store(&root.join(&name), |target| {
		let stored = |k: &Hash| midnight_storage_core::db::DB::get_node(&source, k);
		writer::copy(&[state_root], stored, |keys| vec![None; keys.len()], target)
	})?;
	drop(source);
	let marker =
		Marker::new(name, request.block_number, request.block_hash.as_deref(), &request.state_key);
	let store = open_marked(root, &marker)?;
	marker::write(root, &marker)?;
	install(store, *cache_size)?;
	set_current(&marker, &request.state_key);
	Ok(marker)
}

/// Take a snapshot of the window state `request.state_key`.
///
/// 1. With every window state pinned, copy them into a fresh store, then copy what the window
///    gained meanwhile. Calls keep running.
/// 2. Close the new store, which makes it durable, then write the marker. If that fails, the new
///    store is removed.
/// 3. With no runtime call running and no block in progress: copy what the window gained since 1,
///    switch to the new store, flush the pending nodes into it, push every node out of both
///    arenas' read caches and empty the verified-transaction caches. Then delete the old store.
///    If this step fails, the marker goes back to the snapshot still in use. The log gives how
///    long new calls were held up, from when the switch asked for its lock.
pub fn take_snapshot(request: &SnapshotRequest) -> Result<Marker, String> {
	let _running = match SNAPSHOT_RUNNING.try_lock() {
		Ok(guard) => guard,
		Err(TryLockError::Poisoned(e)) => e.into_inner(),
		Err(TryLockError::WouldBlock) => return Err("a snapshot is being taken".into()),
	};
	let started = Instant::now();
	let Settings { root, cache_size } = settings()?;
	let cache_size = *cache_size;
	let storage = default_storage::<DbSnapshot>();
	let in_memory = |keys: &[Hash]| storage.with_backend(|b| from_memory(b, keys));

	let name = marker::snapshot_dir_name(request.block_number, request.block_hash.as_deref());
	let dir = root.join(&name);
	let pinned = window::pin_all();
	if !pinned.0.contains(&request.state_key) {
		return Err(format!("block #{}'s state is not in the window", request.block_number));
	}
	write_store(&dir, |target| {
		copy_window(&pinned, target, in_memory)?;
		// Catch up with what the window gained meanwhile, so the switch has less to copy.
		copy_window(&window::pin_all(), target, in_memory)
	})?;
	drop(pinned);
	let marker =
		Marker::new(name, request.block_number, request.block_hash.as_deref(), &request.state_key);
	let new_store = open_marked(root, &marker).inspect_err(|_| remove_store(&dir))?;
	if let Err(e) = marker::write(root, &marker) {
		// The marker names the new store only if the write failed at the directory fsync.
		if marker::read(root).is_ok_and(|m| m.as_ref() != Some(&marker)) {
			remove_store(&dir);
		}
		return Err(e);
	}

	let (exclusive, held_from) = exclusive();
	let pinned = window::pin_all();
	let switched = storage.with_backend(|backend| {
		copy_window(&pinned, &new_store, |keys| from_memory(backend, keys))?;
		let old = store::install(new_store);
		backend.flush_all_changes_to_db();
		// Not left to the off-chain refill below: that one reads through this arena only on its
		// own misses, none if no off-chain call read since the last switch. A node of the old
		// store left in this read cache would be taken as stored, and a block that builds it
		// again would then rely on the deleted store.
		refill(backend, cache_size);
		Ok::<_, String>(old)
	});
	drop(pinned);
	let old = match switched {
		Ok(old) => old,
		Err(e) => {
			if let Some(current) = current_marker() {
				marker::write(root, &current)?;
				remove_store(&dir);
			}
			return Err(format!("switch to snapshot {} failed: {e}", marker.dir));
		},
	};
	default_storage::<EphemeralDb<DbSnapshot>>().with_backend(|b| refill(b, cache_size));
	*lock(&OFFCHAIN_STATES) = None;
	forget_verified_transactions();
	set_current(&marker, &request.state_key);
	REQUESTED.store(false, Ordering::Release);
	let paused = held_from.elapsed();
	drop(exclusive);

	if let Some(old) = old {
		let old_dir = old.dir().to_path_buf();
		drop(old);
		remove_store(&old_dir);
	}
	log::info!(
		target: LOG_TARGET,
		"Ledger snapshot {} at #{} in {:?}; runtime calls held for {paused:?}",
		marker.dir,
		request.block_number,
		started.elapsed(),
	);
	Ok(marker)
}

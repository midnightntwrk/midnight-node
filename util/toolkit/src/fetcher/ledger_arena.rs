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

//! The on-disk ledger arena under `--ledger-state-db`.
//!
//! With a ledger state cache, every ledger object of the run lives in a ParityDB in
//! `<ledger-state-db>/ledger-arena`. A cached ledger state is a GC root in that DB:
//! its snapshot file only records the root key, a command opens it lazily and loads
//! the nodes it touches. See `docs/toolkit-paritydb-storage.md`.
//!
//! ParityDB is single-process, so the arena is guarded by an exclusive lock on
//! `<ledger-state-db>/ledger-arena.lock`; a second toolkit process on the same
//! directory waits for the first to exit.

use crate::tx_generator::source::FetchCacheConfig;
use midnight_ledger_unsafe_helpers::{
	ArenaKey, DB, DefaultDB, Deserializable, Serializable, Sp, Storable, Tagged, arena_db,
	mn_ledger_storage::arena::TypedArenaKey,
};
use std::{
	fs::File,
	io::{self, Write},
	path::{Path, PathBuf},
	sync::{
		Mutex, OnceLock,
		atomic::{AtomicU64, Ordering},
	},
	time::{Duration, Instant},
};

/// ParityDB directory, relative to `--ledger-state-db`.
pub const ARENA_DIR: &str = "ledger-arena";
const LOCK_FILE: &str = "ledger-arena.lock";
/// Nodes written since the last full GC, carried across runs.
const GC_STATE_FILE: &str = "ledger-arena.gc";

/// Run a full mark-and-sweep once this many nodes were written since the last one.
/// Roughly the size of a long chain's live state (preprod at block 600k: 3.4M
/// nodes), so the DB holds at most about twice what is reachable.
const GC_THRESHOLD_NODES: u64 = 4_000_000;

/// Seconds to wait for another toolkit process holding the arena; 0 waits forever.
const LOCK_TIMEOUT_ENV: &str = "MN_LEDGER_STATE_DB_LOCK_TIMEOUT";

#[derive(Debug, thiserror::Error)]
pub enum ArenaError {
	#[error("ledger state db {path}: {source}")]
	Io { path: PathBuf, source: io::Error },
	#[error(
		"ledger state db {path} is in use by another toolkit process ({holder}); gave up after {waited:?} (set {LOCK_TIMEOUT_ENV} to wait longer, 0 to wait forever)"
	)]
	Locked { path: PathBuf, holder: String, waited: Duration },
	#[error(transparent)]
	Install(#[from] arena_db::InstallError),
}

struct OpenArena {
	root: PathBuf,
	/// Held for the life of the process: the OS releases the lock on exit.
	_lock: File,
}

static OPEN: OnceLock<OpenArena> = OnceLock::new();
static FLUSHED_SINCE_GC: AtomicU64 = AtomicU64::new(0);
static INIT: Mutex<()> = Mutex::new(());

/// Choose this process's ledger arena: on disk under `ledger_state_db` when the
/// ledger state cache is enabled (same rule as `create_file_wallet_cache`), in
/// memory otherwise. A no-op once an arena is installed.
pub fn init(ledger_state_db: &str, fetch_cache: &FetchCacheConfig) -> Result<(), ArenaError> {
	let _guard = INIT.lock().unwrap_or_else(|e| e.into_inner());
	if arena_db::try_default_storage().is_some() {
		return Ok(());
	}
	if ledger_state_db.is_empty() || matches!(fetch_cache, FetchCacheConfig::InMemory) {
		arena_db::ensure_default_storage();
		return Ok(());
	}
	open(Path::new(ledger_state_db))
}

/// [`init`] off the async runtime's worker threads, since it may wait for the lock.
pub async fn init_async(
	ledger_state_db: &str,
	fetch_cache: &FetchCacheConfig,
) -> Result<(), ArenaError> {
	let (db, cache) = (ledger_state_db.to_string(), fetch_cache.clone());
	tokio::task::spawn_blocking(move || init(&db, &cache))
		.await
		.expect("ledger arena init task panicked")
}

/// Install the arena in `<ledger_state_db>/ledger-arena`, waiting for the lock.
pub fn open(ledger_state_db: &Path) -> Result<(), ArenaError> {
	let io_err = |path: &Path| {
		let path = path.to_path_buf();
		move |source| ArenaError::Io { path, source }
	};
	std::fs::create_dir_all(ledger_state_db).map_err(io_err(ledger_state_db))?;
	let lock_path = ledger_state_db.join(LOCK_FILE);
	let lock = acquire_lock(&lock_path)?;

	let dir = ledger_state_db.join(ARENA_DIR);
	let t = Instant::now();
	arena_db::install_persistent_storage(&dir)?;
	let flushed = std::fs::read_to_string(ledger_state_db.join(GC_STATE_FILE))
		.ok()
		.and_then(|s| s.trim().parse().ok())
		.unwrap_or(0);
	FLUSHED_SINCE_GC.store(flushed, Ordering::Relaxed);
	log::info!("opened ledger arena {} in {:?}", dir.display(), t.elapsed());
	let _ = OPEN.set(OpenArena { root: ledger_state_db.to_path_buf(), _lock: lock });
	Ok(())
}

fn acquire_lock(path: &Path) -> Result<File, ArenaError> {
	use fs2::FileExt;
	let io_err = |source| ArenaError::Io { path: path.to_path_buf(), source };
	let mut file = std::fs::OpenOptions::new()
		.read(true)
		.write(true)
		.create(true)
		.truncate(false)
		.open(path)
		.map_err(io_err)?;
	let timeout = std::env::var(LOCK_TIMEOUT_ENV)
		.ok()
		.and_then(|v| v.parse::<u64>().ok())
		.filter(|&s| s > 0)
		.map(Duration::from_secs);
	let start = Instant::now();
	let mut last_log: Option<Instant> = None;
	while file.try_lock_exclusive().is_err() {
		let holder = std::fs::read_to_string(path).unwrap_or_default().trim().to_string();
		if timeout.is_some_and(|t| start.elapsed() >= t) {
			return Err(ArenaError::Locked {
				path: path.to_path_buf(),
				holder,
				waited: start.elapsed(),
			});
		}
		if last_log.is_none_or(|t| t.elapsed() >= Duration::from_secs(60)) {
			log::warn!(
				"ledger state db is in use by another toolkit process ({holder}); waiting for it to finish (lock {})",
				path.display()
			);
			last_log = Some(Instant::now());
		}
		std::thread::sleep(Duration::from_millis(500));
	}
	file.set_len(0).map_err(io_err)?;
	write!(file, "pid {}", std::process::id()).map_err(io_err)?;
	Ok(file)
}

/// Whether ledger states are persisted in an on-disk arena in this process.
pub fn is_persistent() -> bool {
	arena_db::is_persistent()
}

/// Make the state behind `sp` a GC root and write every pending node to disk.
/// Returns the root key to record in the snapshot.
pub fn persist_root<T: Storable<DefaultDB>>(sp: &Sp<T, DefaultDB>) -> io::Result<Vec<u8>> {
	let mut root = sp.clone();
	root.persist();
	flush();
	let mut bytes = Vec::new();
	root.as_child().serialize(&mut bytes)?;
	Ok(bytes)
}

/// Drop one GC-root reference taken by [`persist_root`]. The nodes stay on disk
/// until a GC finds them unreachable.
pub fn release_root(key_bytes: &[u8]) -> io::Result<()> {
	let key = decode_key(key_bytes)?;
	let storage = arena_db::try_default_storage().expect("ledger arena not installed");
	storage
		.arena
		.with_backend(|b| key.refs().into_iter().for_each(|h| b.unpersist(h)));
	Ok(())
}

/// Lazily open the state rooted at `key_bytes`: only its top node is read now.
pub fn load_root<T: Storable<DefaultDB> + Tagged>(
	key_bytes: &[u8],
) -> io::Result<Sp<T, DefaultDB>> {
	let key = decode_key(key_bytes)?;
	let storage = arena_db::try_default_storage().expect("ledger arena not installed");
	let missing = storage
		.arena
		.with_backend(|b| key.refs().into_iter().find(|h| b.get(h).is_none()).cloned());
	if let Some(hash) = missing {
		return Err(io::Error::new(
			io::ErrorKind::NotFound,
			format!("ledger state root {hash:?} is not in the ledger arena"),
		));
	}
	storage.arena.get_lazy(&TypedArenaKey::<T, _>::from(key))
}

fn decode_key(bytes: &[u8]) -> io::Result<ArenaKey<<DefaultDB as DB>::Hasher>> {
	Deserializable::deserialize(&mut &bytes[..], 0)
}

/// Write every pending node and root-count change to disk.
pub fn flush() {
	let Some(storage) = arena_db::try_default_storage() else { return };
	let t = Instant::now();
	let written = storage.arena.with_backend(|b| {
		let n = b.get_write_cache_len();
		b.flush_all_changes_to_db();
		n
	});
	FLUSHED_SINCE_GC.fetch_add(written as u64, Ordering::Relaxed);
	log::debug!("[perf] ledger arena flush: {written} objects in {:?}", t.elapsed());
}

/// Flush, then collect garbage when enough was written since the last collection.
/// The GC keeps every persisted root and every node still in use in memory.
pub fn finish_save() {
	flush();
	arena_db::release_flushed_roots();
	flush();
	let Some(open) = OPEN.get() else { return };
	let flushed = FLUSHED_SINCE_GC.load(Ordering::Relaxed);
	if flushed >= GC_THRESHOLD_NODES {
		let storage = arena_db::try_default_storage().expect("ledger arena not installed");
		let t = Instant::now();
		log::info!("ledger arena: collecting garbage ({flushed} nodes written since the last GC)");
		// One unbounded call runs the whole mark and sweep.
		let removed = storage.arena.with_backend(|b| b.gc(Duration::from_secs(24 * 3600)));
		log::info!("ledger arena: GC removed {removed} nodes in {:?}", t.elapsed());
		FLUSHED_SINCE_GC.store(0, Ordering::Relaxed);
	}
	let state = open.root.join(GC_STATE_FILE);
	if let Err(e) = std::fs::write(&state, FLUSHED_SINCE_GC.load(Ordering::Relaxed).to_string()) {
		log::warn!("failed to write {}: {e}", state.display());
	}
}

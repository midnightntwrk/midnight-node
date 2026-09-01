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

//! File-based storage backend for wallet state caching.
//!
//! Stores ledger snapshots and per-wallet state as plain files:
//! ```text
//! <root>/<chain_id_hex>/ledger/<block_height>.zstd
//! <root>/<chain_id_hex>/wallets/<seed_hash_hex>.bin
//! ```
//!
//! Write `.tmp` + atomic rename ensure data consistency on POSIX when
//! used from multiple processes.

use super::WalletStateCaching;
use crate::fetcher::wallet_state_cache::{CachedWalletState, LedgerSnapshot};
use async_trait::async_trait;
use std::{
	fs, io,
	path::{Path, PathBuf},
	time::Duration,
};
use subxt::utils::H256;
use tempfile::{Builder as TempFileBuilder, NamedTempFile};

/// Ledger snapshots younger than this are never GC'd, giving concurrent
/// processes time to finish saving wallet states that reference them.
const GC_GRACE_PERIOD: Duration = Duration::from_secs(5 * 60);

/// The newest N ledger snapshots are always retained by GC, even when no
/// cached wallet references their height. Wallet entries can lag behind the
/// snapshot just saved (`write_wallet_if_newer` skips files whose recorded
/// height didn't advance), and without this floor the reference-based GC
/// deletes exactly the snapshot the next warm start needs.
const MIN_SNAPSHOTS_RETAINED: usize = 2;

pub struct FileBackend {
	root: PathBuf,
}

impl FileBackend {
	pub fn new(root: impl Into<PathBuf>) -> Self {
		let root = root.into();
		fs::create_dir_all(&root).unwrap_or_else(|e| {
			panic!("failed to create ledger_state_db directory '{}': {}", root.display(), e)
		});
		Self { root }
	}

	fn ledger_dir(&self, chain_id: H256) -> PathBuf {
		self.root.join(hex::encode(chain_id.0)).join("ledger")
	}

	fn wallets_dir(&self, chain_id: H256) -> PathBuf {
		self.root.join(hex::encode(chain_id.0)).join("wallets")
	}

	fn ledger_path(&self, chain_id: H256, block_height: u64) -> PathBuf {
		self.ledger_dir(chain_id).join(format!("{:012}.zstd", block_height))
	}

	fn wallet_path(&self, chain_id: H256, seed_hash: H256) -> PathBuf {
		self.wallets_dir(chain_id).join(format!("{}.bin", hex::encode(seed_hash.0)))
	}
}

fn parse_ledger_height(filename: &str) -> Option<u64> {
	filename.strip_suffix(".zstd")?.parse().ok()
}

fn parse_seed_hash(filename: &str) -> Option<H256> {
	let hex_str = filename.strip_suffix(".bin")?;
	let bytes = hex::decode(hex_str).ok()?;
	if bytes.len() == 32 { Some(H256::from_slice(&bytes)) } else { None }
}

/// Create the staging file for an atomic replace inside `dir`.
///
/// [`NamedTempFile::new_in`] hard-codes mode 0600 and `persist()` renames, so that mode lands on
/// the finished cache file — and no umask can widen it, since a umask only ever clears bits. A
/// `--ledger-state-db` shared between users (perf boxes run jobs as one user and interactive
/// sessions as another, both in a common group) therefore ends up holding entries only the
/// writing user can open: every other user's run sees each seed as a cache miss and replays from
/// genesis, and its snapshot GC sweep used to delete the entries it could not read.
///
/// So request 0666 and let the process umask narrow it — 0002 yields 0664, the usual 0022 yields
/// 0644, and 0077 still yields 0600 for anyone who wants a private cache. Directories created by
/// this backend are left to the umask the same way (`fs::create_dir_all` uses `0777 & !umask`),
/// so a shared deployment sets one umask and both halves follow.
fn staging_file(dir: &Path) -> io::Result<NamedTempFile> {
	// `mut` is only needed by the `cfg(unix)` block below.
	#[allow(unused_mut)]
	let mut builder = TempFileBuilder::new();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		builder.permissions(fs::Permissions::from_mode(0o666));
	}
	builder.tempfile_in(dir)
}

/// Write to a unique temp file, then rename over `<path>`.
fn write_via_tmp_and_rename(path: &Path, data: &[u8]) -> io::Result<()> {
	let dir = path
		.parent()
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
	let tmp = staging_file(dir)?;
	fs::write(tmp.path(), data)?;
	// persist() uses rename(2) on POSIX — atomic directory-entry swap
	tmp.persist(path).map_err(|e| e.error)?;
	Ok(())
}

/// Read the recorded block height out of a wallet entry's header.
///
/// `Err(_)` means the file could not be read at all (missing, unreadable by this user, io error);
/// `Ok(None)` means it was read but is not this format version (a stale entry from an older
/// toolkit, whose cache key differs anyway — see `WALLET_CACHE_FORMAT_VERSION`). Callers must keep
/// the two apart: an entry we merely cannot open right now is not a corrupt entry, and must never
/// be deleted on that basis.
fn read_wallet_height(path: &Path) -> io::Result<Option<u64>> {
	let mut file = fs::File::open(path)?;
	// Header is `[version: u8][block_height: u64 LE]` (9 bytes).
	let mut header = [0u8; 9];
	io::Read::read_exact(&mut file, &mut header)?;
	Ok(CachedWalletState::block_height_from_header(&header))
}

/// Write wallet data only if `new_height` exceeds the existing file's height.
/// Check happens after writing the temp file but before rename to minimize the TOCTOU window.
/// A concurrent writer can still race between our read and rename — we accept that
/// the consequence is a benign height regression (extra replay on next startup).
///
/// Returns `Ok(None)` when written, or `Ok(Some(existing_height))` when skipped
/// because the file already records a same-or-newer height.
fn write_wallet_if_newer(path: &Path, new_height: u64, data: &[u8]) -> io::Result<Option<u64>> {
	let dir = path
		.parent()
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
	let tmp = staging_file(dir)?;
	fs::write(tmp.path(), data)?;
	match read_wallet_height(path) {
		Ok(Some(existing)) if existing >= new_height => {
			return Ok(Some(existing)); // tmp auto-deleted on drop
		},
		Ok(_) => {},
		Err(e) if e.kind() == io::ErrorKind::NotFound => {},
		// Present but unreadable (e.g. written 0600 by another user before this backend started
		// asking for 0666). We cannot compare heights, so we replace it — which also heals the
		// permissions, since the replacement comes from `staging_file`.
		Err(e) => log::warn!(
			"Cannot read existing wallet cache entry {path:?} ({e}); replacing it at height {new_height}"
		),
	}
	// persist() uses rename(2) on POSIX — atomic directory-entry swap
	tmp.persist(path).map_err(|e| e.error)?;
	Ok(None)
}

/// List filenames in a directory, returning empty vec if the directory doesn't exist.
fn list_dir(dir: &Path) -> Vec<String> {
	let entries = match fs::read_dir(dir) {
		Ok(e) => e,
		Err(_) => return Vec::new(),
	};
	entries
		.filter_map(|e| e.ok())
		.filter_map(|e| e.file_name().into_string().ok())
		.collect()
}

#[async_trait]
impl WalletStateCaching for FileBackend {
	async fn get_ledger_snapshot(
		&self,
		chain_id: H256,
		block_height: u64,
	) -> Option<LedgerSnapshot> {
		let path = self.ledger_path(chain_id, block_height);
		let data = match tokio::task::spawn_blocking(move || fs::read(&path)).await {
			Ok(Ok(data)) => data,
			_ => return None,
		};

		match LedgerSnapshot::from_value_bytes(&data, block_height) {
			Ok(snapshot) => Some(snapshot),
			Err(e) => {
				log::warn!("Failed to decode ledger snapshot from file: {e}");
				None
			},
		}
	}

	async fn set_ledger_snapshot(&self, chain_id: H256, snapshot: LedgerSnapshot) {
		let block_height = snapshot.block_height;
		let encoded: Vec<u8> = match snapshot.to_value_bytes() {
			Ok(b) => b,
			Err(e) => {
				log::warn!("Failed to serialize ledger snapshot: {e}");
				return;
			},
		};

		let dir = self.ledger_dir(chain_id);
		let path = self.ledger_path(chain_id, block_height);
		let size = encoded.len();
		if let Err(e) = tokio::task::spawn_blocking(move || {
			fs::create_dir_all(&dir)?;
			write_via_tmp_and_rename(&path, &encoded)
		})
		.await
		.unwrap_or_else(|e| Err(io::Error::new(io::ErrorKind::Other, e)))
		{
			log::warn!("Failed to write ledger snapshot file: {e}");
			return;
		}

		log::info!("Saved ledger snapshot at block {} ({} bytes)", block_height, size);
	}

	async fn get_latest_ledger_height(&self, chain_id: H256) -> Option<u64> {
		let dir = self.ledger_dir(chain_id);
		let filenames =
			tokio::task::spawn_blocking(move || list_dir(&dir)).await.unwrap_or_default();
		filenames.iter().filter_map(|f| parse_ledger_height(f)).max()
	}

	async fn get_wallet_states(
		&self,
		chain_id: H256,
		seed_hashes: &[H256],
	) -> Vec<Option<CachedWalletState>> {
		let paths: Vec<_> =
			seed_hashes.iter().map(|&h| (h, self.wallet_path(chain_id, h))).collect();

		tokio::task::spawn_blocking(move || {
			paths
				.into_iter()
				.map(|(seed_hash, path)| {
					let data = match fs::read(&path) {
						Ok(data) => data,
						Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
						// A cache entry that exists but cannot be read is not a cache miss we
						// should pass over in silence: it forces this seed's whole history to be
						// replayed from genesis. The usual cause is a shared `ledger_state_db`
						// holding another user's 0600 entries. Say so, and leave the file alone.
						Err(e) => {
							log::warn!(
								"Wallet cache entry {path:?} exists but cannot be read ({e}); \
								 treating as a miss — this seed will replay from genesis"
							);
							return None;
						},
					};
					match CachedWalletState::from_value_bytes(&data, seed_hash) {
						Ok(cached) => Some(cached),
						Err(e) => {
							// Genuinely corrupt: the cache key folds in the format version, so a
							// file under *this* key was written by a writer of this format. Treat
							// as a miss and evict so it is not retried on every run.
							log::warn!("Evicting undecodable wallet state file {path:?}: {e}");
							let _ = fs::remove_file(&path);
							None
						},
					}
				})
				.collect()
		})
		.await
		.unwrap_or_else(|_| seed_hashes.iter().map(|_| None).collect())
	}

	async fn set_wallet_states(&self, chain_id: H256, wallets: &[CachedWalletState]) {
		if wallets.is_empty() {
			return;
		}

		let dir = self.wallets_dir(chain_id);
		let items: Vec<_> = wallets
			.iter()
			.filter_map(|w: &CachedWalletState| {
				let encoded = match w.to_value_bytes() {
					Ok(b) => b,
					Err(e) => {
						log::warn!("Failed to serialize wallet state for {:?}: {e}", w.seed_hash);
						return None;
					},
				};
				Some((self.wallet_path(chain_id, w.seed_hash), w.block_height, encoded))
			})
			.collect();

		let count = items.len();
		let skipped = match tokio::task::spawn_blocking(move || -> io::Result<Vec<u64>> {
			fs::create_dir_all(&dir)?;
			let mut skipped = Vec::new();
			for (path, new_height, data) in &items {
				if let Some(existing) = write_wallet_if_newer(path, *new_height, data)? {
					skipped.push(existing);
				}
			}
			Ok(skipped)
		})
		.await
		.unwrap_or_else(|e| Err(io::Error::new(io::ErrorKind::Other, e)))
		{
			Ok(skipped) => skipped,
			Err(e) => {
				log::warn!("Failed to write wallet state files: {e}");
				return;
			},
		};

		if skipped.is_empty() {
			log::info!("Saved {} wallet cache entries", count);
		} else {
			// Skips here mean the snapshot heights we just produced do not
			// advance the on-disk entries — the ledger snapshot saved next to
			// them will be unreferenced, so surface it loudly.
			log::warn!(
				"Saved {} wallet cache entries: {} written, {} skipped (existing files already at heights {}..={})",
				count,
				count - skipped.len(),
				skipped.len(),
				skipped.iter().min().copied().unwrap_or_default(),
				skipped.iter().max().copied().unwrap_or_default(),
			);
		}
	}

	async fn delete_wallet_states(&self, chain_id: H256, seed_hashes: &[H256]) {
		if seed_hashes.is_empty() {
			return;
		}

		let paths: Vec<_> = seed_hashes.iter().map(|&h| self.wallet_path(chain_id, h)).collect();

		tokio::task::spawn_blocking(move || {
			for path in &paths {
				match fs::remove_file(path) {
					Ok(()) => {},
					Err(e) if e.kind() == io::ErrorKind::NotFound => {},
					Err(e) => log::warn!("Failed to delete wallet state file: {e}"),
				}
			}
		})
		.await
		.ok();
	}

	async fn gc_ledger_snapshots(&self, chain_id: H256, keep_heights: &[u64]) {
		let dir = self.ledger_dir(chain_id);
		let mut keep: std::collections::HashSet<u64> = keep_heights.iter().copied().collect();

		let removed = tokio::task::spawn_blocking(move || {
			// Always spare the newest MIN_SNAPSHOTS_RETAINED snapshots, referenced or not.
			let mut heights: Vec<u64> =
				list_dir(&dir).iter().filter_map(|n| parse_ledger_height(n)).collect();
			heights.sort_unstable_by(|a, b| b.cmp(a));
			keep.extend(heights.iter().take(MIN_SNAPSHOTS_RETAINED));

			let mut removed = 0u64;
			for name in list_dir(&dir) {
				if let Some(height) = parse_ledger_height(&name) {
					if !keep.contains(&height) {
						let path = dir.join(&name);
						let dominated_by_grace_period = fs::metadata(&path)
							.and_then(|m| m.modified())
							.is_ok_and(|t| t.elapsed().unwrap_or(Duration::ZERO) < GC_GRACE_PERIOD);
						if dominated_by_grace_period {
							continue;
						}
						match fs::remove_file(&path) {
							Ok(()) => removed += 1,
							Err(e) if e.kind() == io::ErrorKind::NotFound => {},
							Err(e) => log::warn!("Failed to GC ledger snapshot file {name}: {e}"),
						}
					}
				}
			}
			removed
		})
		.await
		.unwrap_or(0);

		if removed > 0 {
			log::info!("GC: removed {} stale ledger snapshots", removed);
		}
	}

	async fn get_all_cached_wallet_heights(&self, chain_id: H256) -> Vec<u64> {
		let dir = self.wallets_dir(chain_id);

		tokio::task::spawn_blocking(move || {
			let mut heights = std::collections::HashSet::new();
			for name in list_dir(&dir) {
				if parse_seed_hash(&name).is_none() {
					continue;
				}
				let path = dir.join(&name);
				// Read-only on purpose. This scan feeds snapshot GC; it is reached from every
				// cache save by every command, and it sees the *whole* directory rather than the
				// seeds of the calling run. Deleting from here meant one command could destroy
				// another user's — or an older toolkit build's — entries wholesale, on nothing
				// more than a failed read. An entry it cannot account for is simply not counted
				// as a snapshot reference.
				match read_wallet_height(&path) {
					Ok(Some(h)) => {
						heights.insert(h);
					},
					Ok(None) => log::debug!(
						"Ignoring wallet cache file {name}: not this cache format version"
					),
					Err(e) if e.kind() == io::ErrorKind::NotFound => {},
					Err(e) => log::warn!("Ignoring unreadable wallet cache file {name}: {e}"),
				}
			}
			heights.into_iter().collect()
		})
		.await
		.unwrap_or_default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn test_snapshot(block_height: u64) -> LedgerSnapshot {
		use crate::fetcher::wallet_state_cache::SerializableBlockContext;
		LedgerSnapshot {
			block_height,
			ledger_state_bytes: vec![0xAA; 1024],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 1234567890,
				tblock_err: 7,
				parent_block_hash: [0xBB; 32],
				last_block_time: 9876543210,
			},
			state_root: [0xCC; 32],
		}
	}

	fn test_wallet(seed_hash: H256, block_height: u64) -> CachedWalletState {
		CachedWalletState {
			seed_hash,
			block_height,
			shielded_state_bytes: vec![0xDD; 500],
			dust_local_state_bytes: Some(vec![0xEE; 200]),
		}
	}

	fn chain_id() -> H256 {
		H256::from([0x01; 32])
	}

	fn test_fixture() -> (TempDir, FileBackend, H256) {
		let tmp = TempDir::new().unwrap();
		let backend = FileBackend::new(tmp.path());
		(tmp, backend, chain_id())
	}

	#[tokio::test]
	async fn ledger_snapshot_roundtrip() {
		let (_, backend, cid) = test_fixture();

		let snapshot = test_snapshot(42);
		backend.set_ledger_snapshot(cid, snapshot.clone()).await;
		let restored = backend.get_ledger_snapshot(cid, 42).await.expect("snapshot missing");

		assert_eq!(snapshot, restored);
	}

	#[tokio::test]
	async fn get_latest_ledger_height_multiple() {
		let (_, backend, cid) = test_fixture();

		assert_eq!(backend.get_latest_ledger_height(cid).await, None);

		backend.set_ledger_snapshot(cid, test_snapshot(100)).await;
		assert_eq!(backend.get_latest_ledger_height(cid).await, Some(100));

		backend.set_ledger_snapshot(cid, test_snapshot(200)).await;
		assert_eq!(backend.get_latest_ledger_height(cid).await, Some(200));

		backend.set_ledger_snapshot(cid, test_snapshot(50)).await;
		assert_eq!(backend.get_latest_ledger_height(cid).await, Some(200));
	}

	#[tokio::test]
	async fn wallet_states_batch() {
		let (_, backend, cid) = test_fixture();

		let h1 = H256::from([0x01; 32]);
		let h2 = H256::from([0x02; 32]);
		let h3 = H256::from([0x03; 32]);

		let (wallet1, wallet2) = (test_wallet(h1, 100), test_wallet(h2, 100));
		backend.set_wallet_states(cid, &[wallet1.clone(), wallet2.clone()]).await;

		let results = backend.get_wallet_states(cid, &[h2, h3, h1]).await;
		assert_eq!(results, vec![Some(wallet2), None, Some(wallet1)]);
	}

	#[tokio::test]
	async fn delete_wallet_states() {
		let (_, backend, cid) = test_fixture();

		let h1 = H256::from([0x01; 32]);
		let h2 = H256::from([0x02; 32]);

		backend
			.set_wallet_states(cid, &[test_wallet(h1, 100), test_wallet(h2, 100)])
			.await;
		backend.delete_wallet_states(cid, &[h1]).await;

		let results = backend.get_wallet_states(cid, &[h1, h2]).await;
		assert!(results[0].is_none());
		assert!(results[1].is_some());
	}

	fn backdate_ledger_snapshot(backend: &FileBackend, cid: H256, height: u64) {
		use std::{fs::FileTimes, time::SystemTime};
		let path = backend.ledger_path(cid, height);
		let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
		let times = FileTimes::new().set_modified(old);
		fs::File::options().write(true).open(&path).unwrap().set_times(times).unwrap();
	}

	#[tokio::test]
	async fn gc_ledger_snapshots() {
		let (_, backend, cid) = test_fixture();

		backend.set_ledger_snapshot(cid, test_snapshot(100)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(200)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(300)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(400)).await;

		// Backdate files past grace period so GC can remove them
		backdate_ledger_snapshot(&backend, cid, 100);
		backdate_ledger_snapshot(&backend, cid, 200);
		backdate_ledger_snapshot(&backend, cid, 300);
		backdate_ledger_snapshot(&backend, cid, 400);

		backend.gc_ledger_snapshots(cid, &[200]).await;

		// 100 is unreferenced and outside the newest-2 retention floor.
		assert!(backend.get_ledger_snapshot(cid, 100).await.is_none());
		// 200 is referenced by a cached wallet height.
		assert!(backend.get_ledger_snapshot(cid, 200).await.is_some());
		// 300 and 400 are the newest MIN_SNAPSHOTS_RETAINED, kept unconditionally.
		assert!(backend.get_ledger_snapshot(cid, 300).await.is_some());
		assert!(backend.get_ledger_snapshot(cid, 400).await.is_some());
	}

	#[tokio::test]
	async fn gc_retains_newest_snapshots_even_unreferenced() {
		let (_, backend, cid) = test_fixture();

		backend.set_ledger_snapshot(cid, test_snapshot(100)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(200)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(300)).await;

		backdate_ledger_snapshot(&backend, cid, 100);
		backdate_ledger_snapshot(&backend, cid, 200);
		backdate_ledger_snapshot(&backend, cid, 300);

		// No wallet references at all: the newest two must still survive.
		backend.gc_ledger_snapshots(cid, &[]).await;

		assert!(backend.get_ledger_snapshot(cid, 100).await.is_none());
		assert!(backend.get_ledger_snapshot(cid, 200).await.is_some());
		assert!(backend.get_ledger_snapshot(cid, 300).await.is_some());
	}

	#[tokio::test]
	async fn gc_spares_recent_snapshots() {
		let (_, backend, cid) = test_fixture();

		// 50 sits outside the newest-MIN_SNAPSHOTS_RETAINED floor ({100, 200})
		// and is unreferenced, so only the grace period protects it.
		backend.set_ledger_snapshot(cid, test_snapshot(50)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(100)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(200)).await;

		backend.gc_ledger_snapshots(cid, &[]).await;
		assert!(
			backend.get_ledger_snapshot(cid, 50).await.is_some(),
			"recent snapshot should survive GC via the grace period"
		);

		// Once past the grace period, it is collected.
		backdate_ledger_snapshot(&backend, cid, 50);
		backend.gc_ledger_snapshots(cid, &[]).await;
		assert!(backend.get_ledger_snapshot(cid, 50).await.is_none());
		assert!(backend.get_ledger_snapshot(cid, 100).await.is_some());
		assert!(backend.get_ledger_snapshot(cid, 200).await.is_some());
	}

	#[tokio::test]
	async fn get_all_cached_wallet_heights() {
		let (_, backend, cid) = test_fixture();

		let h1 = H256::from([0x01; 32]);
		let h2 = H256::from([0x02; 32]);
		let h3 = H256::from([0x03; 32]);

		backend
			.set_wallet_states(
				cid,
				&[test_wallet(h1, 100), test_wallet(h2, 100), test_wallet(h3, 200)],
			)
			.await;

		let mut heights = backend.get_all_cached_wallet_heights(cid).await;
		heights.sort();
		assert_eq!(heights, vec![100, 200]);
	}

	#[tokio::test]
	async fn empty_dir_reads() {
		let (_, backend, cid) = test_fixture();

		assert!(backend.get_ledger_snapshot(cid, 42).await.is_none());
		assert_eq!(backend.get_latest_ledger_height(cid).await, None);
		assert!(backend.get_wallet_states(cid, &[H256::zero()]).await[0].is_none());
		assert!(backend.get_all_cached_wallet_heights(cid).await.is_empty());
	}

	#[tokio::test]
	async fn wallet_state_overwrite() {
		let (_, backend, cid) = test_fixture();
		let h1 = H256::from([0x01; 32]);

		backend.set_wallet_states(cid, &[test_wallet(h1, 100)]).await;
		backend.set_wallet_states(cid, &[test_wallet(h1, 200)]).await;

		let results = backend.get_wallet_states(cid, &[h1]).await;
		assert_eq!(results, vec![Some(test_wallet(h1, 200))]);
	}

	#[tokio::test]
	async fn wallet_state_no_height_regression() {
		let (_, backend, cid) = test_fixture();
		let h1 = H256::from([0x01; 32]);

		backend.set_wallet_states(cid, &[test_wallet(h1, 200)]).await;
		backend.set_wallet_states(cid, &[test_wallet(h1, 100)]).await;

		let results = backend.get_wallet_states(cid, &[h1]).await;
		assert_eq!(results, vec![Some(test_wallet(h1, 200))]);
	}

	#[tokio::test]
	async fn corrupted_wallet_file_is_evicted_on_read() {
		let (_, backend, cid) = test_fixture();

		let h1 = H256::from([0x01; 32]);

		// Write a valid wallet, then overwrite with garbage. The cache key folds in the format
		// version, so a file under this key can only have come from a writer of this format —
		// undecodable content there is real corruption, and reading the seed evicts it.
		backend.set_wallet_states(cid, &[test_wallet(h1, 300)]).await;
		let path = backend.wallet_path(cid, h1);
		assert!(path.exists());
		fs::write(&path, b"short").unwrap();

		assert!(backend.get_wallet_states(cid, &[h1]).await[0].is_none());
		assert!(!path.exists(), "corrupt entry should have been evicted");
	}

	/// The mode a file gets from `open(O_CREAT, 0o666)` under this process's umask — what
	/// `staging_file` should be producing, whatever the umask happens to be in CI.
	#[cfg(unix)]
	fn umask_default_file_mode(dir: &Path) -> u32 {
		use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
		let probe = dir.join("umask-probe");
		fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.mode(0o666)
			.open(&probe)
			.unwrap();
		let mode = fs::metadata(&probe).unwrap().permissions().mode() & 0o777;
		fs::remove_file(&probe).unwrap();
		mode
	}

	/// Cache files must not be owner-only: a `--ledger-state-db` is routinely shared between users
	/// (a CI job and an interactive session on the same box), and 0600 entries make every other
	/// user's run miss the whole cache and replay from genesis. `NamedTempFile` defaults to 0600
	/// and `persist()` keeps it, so the mode has to be asked for explicitly — a umask cannot widen
	/// it. Pinned against the umask rather than a literal so the intent survives any CI umask.
	#[cfg(unix)]
	#[tokio::test]
	async fn cache_files_are_written_at_the_umask_default_not_owner_only() {
		use std::os::unix::fs::PermissionsExt;

		let (tempdir, backend, cid) = test_fixture();
		let expected = umask_default_file_mode(tempdir.path());

		let h1 = H256::from([0x01; 32]);
		backend.set_wallet_states(cid, &[test_wallet(h1, 100)]).await;
		backend.set_ledger_snapshot(cid, test_snapshot(100)).await;

		for path in [backend.wallet_path(cid, h1), backend.ledger_path(cid, 100)] {
			let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
			assert_eq!(
				mode, expected,
				"{path:?} was written 0{mode:o}, expected the umask default 0{expected:o}"
			);
		}
	}

	/// A file this user cannot open is not a corrupt file. The sweep runs on every save by every
	/// command and sees the whole directory, so deleting on a failed read let one command destroy
	/// another user's warm cache.
	#[cfg(unix)]
	#[tokio::test]
	async fn sweep_ignores_unreadable_entry_instead_of_deleting_it() {
		use std::os::unix::fs::PermissionsExt;

		let (_, backend, cid) = test_fixture();
		let (h1, h2) = (H256::from([0x01; 32]), H256::from([0x02; 32]));
		backend
			.set_wallet_states(cid, &[test_wallet(h1, 100), test_wallet(h2, 200)])
			.await;

		let unreadable = backend.wallet_path(cid, h1);
		fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

		let heights = backend.get_all_cached_wallet_heights(cid).await;
		assert_eq!(heights, vec![200], "unreadable entry must not be counted as a reference");
		assert!(unreadable.exists(), "unreadable entry must not be deleted");

		// And it reads back as a miss without being destroyed, so it still heals once the
		// permissions are fixed.
		assert!(backend.get_wallet_states(cid, &[h1]).await[0].is_none());
		assert!(unreadable.exists());
		fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
		assert_eq!(
			backend.get_wallet_states(cid, &[h1]).await[0].as_ref().map(|w| w.block_height),
			Some(100),
			"entry should be usable again once readable"
		);
	}

	/// An entry from an older cache format belongs to an older toolkit build, which keys its
	/// entries differently and may still be in use against the same directory. Skip it; deleting
	/// it made two builds sharing a directory wipe each other on every save.
	#[tokio::test]
	async fn sweep_ignores_foreign_format_entry_instead_of_deleting_it() {
		let (_, backend, cid) = test_fixture();
		let h1 = H256::from([0x01; 32]);
		backend.set_wallet_states(cid, &[test_wallet(h1, 100)]).await;

		// v1 layout: bare 8-byte LE block height, no version prefix.
		let foreign = backend.wallet_path(cid, H256::from([0xaa; 32]));
		let mut v1 = 100u64.to_le_bytes().to_vec();
		v1.extend_from_slice(&[0u8; 32]);
		fs::write(&foreign, &v1).unwrap();

		assert_eq!(backend.get_all_cached_wallet_heights(cid).await, vec![100]);
		assert!(foreign.exists(), "foreign-format entry must not be deleted");
	}

	/// Replacing an entry whose height cannot be read (another user's 0600 file, pre-fix) is how a
	/// shared cache heals: the replacement is written at the umask default.
	#[cfg(unix)]
	#[tokio::test]
	async fn unreadable_entry_is_replaced_and_permissions_heal() {
		use std::os::unix::fs::PermissionsExt;

		let (tempdir, backend, cid) = test_fixture();
		let expected = umask_default_file_mode(tempdir.path());

		let h1 = H256::from([0x01; 32]);
		backend.set_wallet_states(cid, &[test_wallet(h1, 100)]).await;
		let path = backend.wallet_path(cid, h1);
		fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();

		backend.set_wallet_states(cid, &[test_wallet(h1, 200)]).await;

		let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
		assert_eq!(mode, expected, "replacement was written 0{mode:o}");
		assert_eq!(
			backend.get_wallet_states(cid, &[h1]).await[0].as_ref().map(|w| w.block_height),
			Some(200),
		);
	}
}

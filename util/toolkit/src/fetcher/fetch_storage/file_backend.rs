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
use tempfile::NamedTempFile;

/// Ledger snapshots younger than this are never GC'd, giving concurrent
/// processes time to finish saving wallet states that reference them.
const GC_GRACE_PERIOD: Duration = Duration::from_secs(5 * 60);

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

/// Write to a unique temp file, then rename over `<path>`.
fn write_via_tmp_and_rename(path: &Path, data: &[u8]) -> io::Result<()> {
	let dir = path
		.parent()
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
	let tmp = NamedTempFile::new_in(dir)?;
	fs::write(tmp.path(), data)?;
	// persist() uses rename(2) on POSIX — atomic directory-entry swap
	tmp.persist(path).map_err(|e| e.error)?;
	Ok(())
}

fn read_wallet_height(path: &Path) -> Option<u64> {
	let mut file = fs::File::open(path).ok()?;
	let mut header = [0u8; 8];
	io::Read::read_exact(&mut file, &mut header).ok()?;
	CachedWalletState::block_height_from_header(&header)
}

/// Write wallet data only if `new_height` exceeds the existing file's height.
/// Check happens after writing the temp file but before rename to minimize the TOCTOU window.
/// A concurrent writer can still race between our read and rename — we accept that
/// the consequence is a benign height regression (extra replay on next startup).
fn write_wallet_if_newer(path: &Path, new_height: u64, data: &[u8]) -> io::Result<()> {
	let dir = path
		.parent()
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
	let tmp = NamedTempFile::new_in(dir)?;
	fs::write(tmp.path(), data)?;
	if let Some(existing) = read_wallet_height(path) {
		if existing >= new_height {
			return Ok(()); // tmp auto-deleted on drop
		}
	}
	// persist() uses rename(2) on POSIX — atomic directory-entry swap
	tmp.persist(path).map_err(|e| e.error)?;
	Ok(())
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
					let data = fs::read(&path).ok()?;
					match CachedWalletState::from_value_bytes(&data, seed_hash) {
						Ok(cached) => Some(cached),
						Err(e) => {
							log::warn!("Failed to decode wallet state from file: {e}");
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
		if let Err(e) = tokio::task::spawn_blocking(move || -> io::Result<()> {
			fs::create_dir_all(&dir)?;
			for (path, new_height, data) in &items {
				write_wallet_if_newer(path, *new_height, data)?;
			}
			Ok(())
		})
		.await
		.unwrap_or_else(|e| Err(io::Error::new(io::ErrorKind::Other, e)))
		{
			log::warn!("Failed to write wallet state files: {e}");
			return;
		}

		log::info!("Saved {} wallet cache entries", count);
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
		let keep: std::collections::HashSet<u64> = keep_heights.iter().copied().collect();

		let removed = tokio::task::spawn_blocking(move || {
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
				let height = read_wallet_height(&path);
				match height {
					Some(h) => {
						heights.insert(h);
					},
					None => {
						log::error!(
							"Removing corrupted wallet cache file {name}: \
							 could not extract block_height"
						);
						let _ = fs::remove_file(&path);
					},
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

		// Backdate files past grace period so GC can remove them
		backdate_ledger_snapshot(&backend, cid, 100);
		backdate_ledger_snapshot(&backend, cid, 200);
		backdate_ledger_snapshot(&backend, cid, 300);

		backend.gc_ledger_snapshots(cid, &[200]).await;

		assert!(backend.get_ledger_snapshot(cid, 100).await.is_none());
		assert!(backend.get_ledger_snapshot(cid, 200).await.is_some());
		assert!(backend.get_ledger_snapshot(cid, 300).await.is_none());
	}

	#[tokio::test]
	async fn gc_spares_recent_snapshots() {
		let (_, backend, cid) = test_fixture();

		backend.set_ledger_snapshot(cid, test_snapshot(100)).await;
		backend.set_ledger_snapshot(cid, test_snapshot(200)).await;

		// Backdate only height 100
		backdate_ledger_snapshot(&backend, cid, 100);

		backend.gc_ledger_snapshots(cid, &[]).await;

		assert!(backend.get_ledger_snapshot(cid, 100).await.is_none());
		assert!(
			backend.get_ledger_snapshot(cid, 200).await.is_some(),
			"recent snapshot should survive GC"
		);
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
	async fn corrupted_wallet_file_is_deleted() {
		let (_, backend, cid) = test_fixture();

		let h1 = H256::from([0x01; 32]);

		// Write a valid wallet, then overwrite with garbage
		backend.set_wallet_states(cid, &[test_wallet(h1, 300)]).await;
		let path = backend.wallet_path(cid, h1);
		assert!(path.exists());
		fs::write(&path, b"short").unwrap();

		// Height should not be found and the corrupted file should be deleted
		let heights = backend.get_all_cached_wallet_heights(cid).await;
		assert!(heights.is_empty());
		assert!(!path.exists(), "corrupted file should have been deleted");
	}
}

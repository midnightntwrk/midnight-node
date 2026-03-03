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

use std::{any::type_name, cmp::Ordering, path::Path, sync::Arc};

use async_trait::async_trait;
use core::fmt::Debug;
use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;
use redb::{Database, Key, ReadableDatabase, TableDefinition, TypeName, Value};
use serde::{Deserialize, Serialize};
use subxt::utils::H256;
use tokio::sync::RwLock;

use super::{FetchStorage, WalletStateCache, WalletStateCaching};
use crate::fetcher::wallet_state_cache::{WalletCacheKey, compress, decompress};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockKey {
	chain_id: H256,
	block_number: u64,
}

/// Persistent [`FetchStorage`] backend using [redb](https://github.com/cberner/redb).
///
/// Data is serialized as BSON. Uses `RwLock` for concurrent read access.
/// Wallet state is compressed with zstd for efficient storage.
#[derive(Clone)]
pub struct RedbBackend {
	pub db: Arc<RwLock<Database>>,
	pub block_data_table: TableDefinition<'static, Serde<BlockKey>, Serde<RawBlockData>>,
	pub highest_verified_table: TableDefinition<'static, [u8; 32], u64>,
	pub wallet_state_table: TableDefinition<'static, Serde<WalletCacheKey>, &'static [u8]>,
}

impl RedbBackend {
	/// Creates or opens a database at the given path. Will fail if open in another process.
	pub fn new(path: impl AsRef<Path>) -> Self {
		let p = path.as_ref();
		if let Some(parent) = p.parent() {
			std::fs::create_dir_all(parent)
				.expect("failed to create parent dir for redb fetch cache");
		}
		Self {
			db: Arc::new(RwLock::new(
				Database::create(path).expect("failed to create database - is it already open?"),
			)),
			block_data_table: TableDefinition::new("raw_block_data_v1"),
			highest_verified_table: TableDefinition::new("highest_verified"),
			wallet_state_table: TableDefinition::new("wallet_state"),
		}
	}
}

#[async_trait]
impl FetchStorage for RedbBackend {
	async fn get_block_data(&self, chain_id: H256, block_number: u64) -> Option<RawBlockData> {
		let read_txn = self.db.read().await.begin_read().expect("failed to begin read txn");
		let Ok(table) = read_txn.open_table(self.block_data_table) else { return None };
		table
			.get(BlockKey { chain_id, block_number })
			.expect("failed to get from table")
			.map(|a| a.value())
	}
	async fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> Vec<Option<RawBlockData>> {
		let read_txn = self.db.read().await.begin_read().expect("failed to begin read txn");
		let Ok(table) = read_txn.open_table(self.block_data_table) else {
			return std::iter::repeat_n(None, range.count()).collect();
		};
		range
			.into_iter()
			.map(|block_number| {
				table
					.get(BlockKey { chain_id, block_number })
					.expect("failed to get from table")
					.map(|a| a.value())
			})
			.collect()
	}

	async fn insert_block_data(&self, chain_id: H256, block_number: u64, block: RawBlockData) {
		// Can only open the table as writable from one thread
		let write_txn = self.db.write().await.begin_write().expect("failed to begin write txn");
		{
			let mut table =
				write_txn.open_table(self.block_data_table).expect("failed to open table");
			table
				.insert(BlockKey { chain_id, block_number }, block)
				.expect("failed to insert block");
		}
		write_txn.commit().expect("failed to commit write")
	}

	async fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, RawBlockData)> + Send,
	) {
		// Can only open the table as writable from one thread
		let write_txn = self.db.write().await.begin_write().expect("failed to begin write txn");
		{
			let mut table =
				write_txn.open_table(self.block_data_table).expect("failed to open table");
			for (block_number, block) in range {
				table
					.insert(BlockKey { chain_id, block_number }, block)
					.expect("failed to insert block");
			}
		}
		write_txn.commit().expect("failed to commit write")
	}

	async fn get_highest_verified_block(&self, chain_id: H256) -> Option<u64> {
		let read_txn = self.db.read().await.begin_read().expect("failed to begin read txn");
		let Ok(table) = read_txn.open_table(self.highest_verified_table) else { return None };
		table.get(&chain_id.0).expect("failed to get from table").map(|a| a.value())
	}

	async fn set_highest_verified_block(&self, chain_id: H256, height: u64) {
		let write_txn = self.db.write().await.begin_write().expect("failed to begin write txn");
		{
			let mut table =
				write_txn.open_table(self.highest_verified_table).expect("failed to open table");
			table.insert(&chain_id.0, height).expect("failed to insert highest verified");
		}
		write_txn.commit().expect("failed to commit write")
	}

	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let read_txn = match self.db.read().await.begin_read() {
			Ok(txn) => txn,
			Err(e) => {
				log::warn!("Failed to begin read transaction for wallet state: {e}");
				return None;
			},
		};
		let Ok(table) = read_txn.open_table(self.wallet_state_table) else { return None };

		let compressed = match table.get(key) {
			Ok(Some(data)) => data,
			Ok(None) => return None,
			Err(e) => {
				log::warn!("Failed to get wallet state from table: {e}");
				return None;
			},
		};

		// Decompress and deserialize
		let decompressed = match decompress(compressed.value()) {
			Ok(data) => data,
			Err(e) => {
				log::warn!("Failed to decompress wallet state cache: {e}");
				return None;
			},
		};

		match bson::deserialize_from_slice(&decompressed) {
			Ok(cache) => Some(cache),
			Err(e) => {
				log::warn!("Failed to deserialize wallet state cache: {e}");
				None
			},
		}
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let block_height = cache.block_height;

		// Serialize and compress
		let serialized = match bson::serialize_to_vec(&cache) {
			Ok(data) => data,
			Err(e) => {
				log::warn!("Failed to serialize wallet state: {e}");
				return;
			},
		};
		let compressed = match compress(&serialized) {
			Ok(data) => data,
			Err(e) => {
				log::warn!("Failed to compress wallet state: {e}");
				return;
			},
		};

		let write_txn = match self.db.write().await.begin_write() {
			Ok(txn) => txn,
			Err(e) => {
				log::warn!("Failed to begin write transaction for wallet state: {e}");
				return;
			},
		};
		{
			let mut table = match write_txn.open_table(self.wallet_state_table) {
				Ok(t) => t,
				Err(e) => {
					log::warn!("Failed to open wallet state table: {e}");
					return;
				},
			};
			if let Err(e) = table.insert(key, compressed.as_slice()) {
				log::warn!("Failed to insert wallet state: {e}");
				return;
			}
		}
		if let Err(e) = write_txn.commit() {
			log::warn!("Failed to commit wallet state write: {e}");
			return;
		}

		log::info!(
			"Cached wallet state at block {} (compressed: {} bytes)",
			block_height,
			compressed.len()
		);
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		<Self as FetchStorage>::get_wallet_state(self, chain_id, wallet_id)
			.await
			.map(|c| c.block_height)
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let write_txn = match self.db.write().await.begin_write() {
			Ok(txn) => txn,
			Err(e) => {
				log::warn!("Failed to begin write transaction for wallet state deletion: {e}");
				return;
			},
		};
		{
			if let Ok(mut table) = write_txn.open_table(self.wallet_state_table) {
				let _ = table.remove(key);
			}
		}
		if let Err(e) = write_txn.commit() {
			log::warn!("Failed to commit wallet state deletion: {e}");
		}
	}
}

// Implement WalletStateCaching for RedbBackend (delegates to FetchStorage impl)
#[async_trait]
impl WalletStateCaching for RedbBackend {
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		<Self as FetchStorage>::get_wallet_state(self, chain_id, wallet_id).await
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		<Self as FetchStorage>::set_wallet_state(self, chain_id, wallet_id, cache).await
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		<Self as FetchStorage>::get_cached_block_height(self, chain_id, wallet_id).await
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		<Self as FetchStorage>::delete_wallet_state(self, chain_id, wallet_id).await
	}
}

/// Wrapper type to handle keys and values using bincode serialization
#[derive(Debug)]
pub struct Serde<T>(pub T);

impl<T> Value for Serde<T>
where
	for<'a> T: Debug + Serialize + Deserialize<'a>,
{
	type SelfType<'a>
		= T
	where
		Self: 'a;

	type AsBytes<'a>
		= Vec<u8>
	where
		Self: 'a;

	fn fixed_width() -> Option<usize> {
		None
	}

	fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
	where
		Self: 'a,
	{
		bson::deserialize_from_slice(&data).unwrap()
	}

	fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
	where
		Self: 'a,
		Self: 'b,
	{
		bson::serialize_to_vec(&value).unwrap()
	}

	fn type_name() -> TypeName {
		TypeName::new(&format!("Serde<{}>", type_name::<T>()))
	}
}

impl<T> Key for Serde<T>
where
	for<'a> T: Debug + Deserialize<'a> + Serialize + Ord,
{
	fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
		Self::from_bytes(data1).cmp(&Self::from_bytes(data2))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::fetcher::wallet_state_cache::{SerializableBlockContext, WalletSnapshot};
	use tempfile::tempdir;

	fn create_test_cache(block_height: u64) -> WalletStateCache {
		WalletStateCache {
			chain_id: H256::from([1u8; 32]),
			wallet_id: H256::from([2u8; 32]),
			block_height,
			ledger_state_bytes: vec![0u8; 1000], // 1KB of test data
			wallet_snapshots: vec![WalletSnapshot {
				seed_hash: H256::from([3u8; 32]),
				shielded_state_bytes: vec![],
				dust_local_state_bytes: None,
			}],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 1234567890,
				tblock_err: 0,
				parent_block_hash: [4u8; 32],
				last_block_time: 1234567890,
			},
			state_root: Some(vec![5u8; 32]),
			version: "wallet-state-cache-v1".to_string(),
		}
	}

	#[tokio::test]
	async fn test_redb_wallet_state_roundtrip() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test.db");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::from([1u8; 32]);
		let wallet_id = H256::from([2u8; 32]);
		let cache = create_test_cache(100);

		// Initially no cache
		let result = WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id).await;
		assert!(result.is_none());

		// Save cache
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache.clone()).await;

		// Retrieve cache
		let retrieved = WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id).await;
		assert!(retrieved.is_some());
		let retrieved = retrieved.unwrap();

		assert_eq!(retrieved.chain_id, cache.chain_id);
		assert_eq!(retrieved.wallet_id, cache.wallet_id);
		assert_eq!(retrieved.block_height, cache.block_height);
		assert_eq!(retrieved.ledger_state_bytes, cache.ledger_state_bytes);
		assert_eq!(retrieved.version, cache.version);
		assert_eq!(retrieved.state_root, cache.state_root);
	}

	#[tokio::test]
	async fn test_redb_wallet_state_get_cached_height() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test.db");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::from([1u8; 32]);
		let wallet_id = H256::from([2u8; 32]);

		// No cache initially
		assert!(
			WalletStateCaching::get_cached_block_height(&backend, chain_id, wallet_id)
				.await
				.is_none()
		);

		// Save cache at height 500
		let cache = create_test_cache(500);
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache).await;

		// Check height
		let height =
			WalletStateCaching::get_cached_block_height(&backend, chain_id, wallet_id).await;
		assert_eq!(height, Some(500));
	}

	#[tokio::test]
	async fn test_redb_wallet_state_delete() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test.db");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::from([1u8; 32]);
		let wallet_id = H256::from([2u8; 32]);
		let cache = create_test_cache(100);

		// Save and verify exists
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache).await;
		assert!(
			WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id)
				.await
				.is_some()
		);

		// Delete
		WalletStateCaching::delete_wallet_state(&backend, chain_id, wallet_id).await;

		// Verify deleted
		assert!(
			WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id)
				.await
				.is_none()
		);
	}

	#[tokio::test]
	async fn test_redb_wallet_state_update() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test.db");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::from([1u8; 32]);
		let wallet_id = H256::from([2u8; 32]);

		// Save at height 100
		let cache1 = create_test_cache(100);
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache1).await;
		assert_eq!(
			WalletStateCaching::get_cached_block_height(&backend, chain_id, wallet_id).await,
			Some(100)
		);

		// Update to height 200
		let cache2 = create_test_cache(200);
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache2).await;
		assert_eq!(
			WalletStateCaching::get_cached_block_height(&backend, chain_id, wallet_id).await,
			Some(200)
		);
	}

	#[tokio::test]
	async fn test_redb_wallet_state_multiple_wallets() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test.db");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::from([1u8; 32]);
		let wallet_id_1 = H256::from([2u8; 32]);
		let wallet_id_2 = H256::from([3u8; 32]);

		let mut cache1 = create_test_cache(100);
		cache1.wallet_id = wallet_id_1;

		let mut cache2 = create_test_cache(200);
		cache2.wallet_id = wallet_id_2;

		// Save both
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id_1, cache1).await;
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id_2, cache2).await;

		// Verify independent
		assert_eq!(
			WalletStateCaching::get_cached_block_height(&backend, chain_id, wallet_id_1).await,
			Some(100)
		);
		assert_eq!(
			WalletStateCaching::get_cached_block_height(&backend, chain_id, wallet_id_2).await,
			Some(200)
		);

		// Delete one doesn't affect other
		WalletStateCaching::delete_wallet_state(&backend, chain_id, wallet_id_1).await;
		assert!(
			WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id_1)
				.await
				.is_none()
		);
		assert!(
			WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id_2)
				.await
				.is_some()
		);
	}

	#[tokio::test]
	async fn test_redb_concurrent_access() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test.db");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::from([1u8; 32]);
		let num_wallets = 10;
		let num_operations = 5;

		// Spawn concurrent tasks that each operate on their own wallet
		let mut handles = vec![];
		for wallet_idx in 0..num_wallets {
			let backend_clone = backend.clone();
			let wallet_id = H256::from([wallet_idx as u8; 32]);

			let handle = tokio::spawn(async move {
				for op in 0..num_operations {
					let cache = WalletStateCache {
						chain_id,
						wallet_id,
						block_height: (wallet_idx * 100 + op) as u64,
						ledger_state_bytes: vec![wallet_idx as u8; 100],
						wallet_snapshots: vec![],
						latest_block_context: SerializableBlockContext {
							tblock_secs: 1234567890,
							tblock_err: 0,
							parent_block_hash: [wallet_idx as u8; 32],
							last_block_time: 1234567890,
						},
						state_root: Some(vec![op as u8; 32]),
						version: "wallet-state-cache-v1".to_string(),
					};

					// Write
					WalletStateCaching::set_wallet_state(
						&backend_clone,
						chain_id,
						wallet_id,
						cache,
					)
					.await;

					// Read back
					let retrieved =
						WalletStateCaching::get_wallet_state(&backend_clone, chain_id, wallet_id)
							.await;
					assert!(retrieved.is_some(), "Wallet {} should have cache", wallet_idx);

					// Height check
					let height = WalletStateCaching::get_cached_block_height(
						&backend_clone,
						chain_id,
						wallet_id,
					)
					.await;
					assert!(height.is_some(), "Wallet {} should have height", wallet_idx);
				}
			});
			handles.push(handle);
		}

		// Wait for all tasks to complete
		for handle in handles {
			handle.await.expect("Task should complete successfully");
		}

		// Verify all wallets have their final state
		for wallet_idx in 0..num_wallets {
			let wallet_id = H256::from([wallet_idx as u8; 32]);
			let cache = WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id).await;
			assert!(cache.is_some(), "Wallet {} should have final state", wallet_idx);
		}
	}
}

// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

//! Redb-based persistent storage backend for wallet state caching.
//!
//! This backend stores wallet state in a local file using the redb embedded database,
//! providing persistence across toolkit sessions for single-instance deployments.

use std::{any::type_name, cmp::Ordering, path::Path, sync::Arc};

use async_trait::async_trait;
use core::fmt::Debug;
use redb::{Database, Key, ReadableDatabase, TableDefinition, TypeName, Value};
use serde::{Deserialize, Serialize};
use subxt::utils::H256;
use tokio::sync::RwLock;

use super::{WalletCacheKey, WalletStateCache, WalletStateStorage};

/// Persistent [`WalletStateStorage`] backend using [redb](https://github.com/cberner/redb).
///
/// Data is serialized as BSON. Uses `RwLock` for concurrent read access.
/// The database file can be shared with the existing `FetchStorage` redb backend
/// since table names are distinct.
#[derive(Clone)]
pub struct RedbBackend {
	db: Arc<RwLock<Database>>,
	wallet_state_table: TableDefinition<'static, Serde<WalletCacheKey>, Serde<WalletStateCache>>,
}

impl RedbBackend {
	/// Table name for wallet state cache entries.
	const WALLET_STATE_TABLE: &'static str = "wallet_state_cache";

	/// Creates or opens a database at the given path.
	///
	/// This can share the same database file as `FetchStorage::RedbBackend` since
	/// the table names are distinct.
	///
	/// # Panics
	///
	/// Panics if the database cannot be created or is already open by another process.
	pub fn new(path: impl AsRef<Path>) -> Self {
		let p = path.as_ref();
		if let Some(parent) = p.parent() {
			std::fs::create_dir_all(parent)
				.expect("failed to create parent dir for redb wallet cache");
		}
		Self {
			db: Arc::new(RwLock::new(
				Database::create(path).expect("failed to create database - is it already open?"),
			)),
			wallet_state_table: TableDefinition::new(Self::WALLET_STATE_TABLE),
		}
	}

	/// Creates a backend from an existing database handle.
	///
	/// Useful for sharing a database with `FetchStorage::RedbBackend`.
	pub fn from_database(db: Arc<RwLock<Database>>) -> Self {
		Self { db, wallet_state_table: TableDefinition::new(Self::WALLET_STATE_TABLE) }
	}
}

#[async_trait]
impl WalletStateStorage for RedbBackend {
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let read_txn = self.db.read().await.begin_read().expect("failed to begin read txn");
		let Ok(table) = read_txn.open_table(self.wallet_state_table) else {
			return None;
		};
		table.get(key).expect("failed to get from table").map(|a| a.value())
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let write_txn = self.db.write().await.begin_write().expect("failed to begin write txn");
		{
			let mut table =
				write_txn.open_table(self.wallet_state_table).expect("failed to open table");
			table.insert(key, cache).expect("failed to insert wallet state cache");
		}
		write_txn.commit().expect("failed to commit write")
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		// For redb, we load the full cache to get the height
		// This could be optimized with a separate height table if needed
		self.get_wallet_state(chain_id, wallet_id).await.map(|c| c.block_height)
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let write_txn = self.db.write().await.begin_write().expect("failed to begin write txn");
		{
			let mut table =
				write_txn.open_table(self.wallet_state_table).expect("failed to open table");
			let _ = table.remove(key); // Ignore if key doesn't exist
		}
		write_txn.commit().expect("failed to commit write")
	}
}

/// Wrapper type to handle keys and values using BSON serialization.
///
/// This is the same pattern as used in `fetcher::fetch_storage::redb_backend::Serde`.
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
		bson::deserialize_from_slice(data).unwrap()
	}

	fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
	where
		Self: 'a,
		Self: 'b,
	{
		bson::serialize_to_vec(value).unwrap()
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
	use crate::wallet_state_storage::{CACHE_VERSION, SerializableBlockContext};
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_redb_backend_roundtrip() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test_wallet_cache.redb");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::random();
		let wallet_id = H256::random();

		// Initially empty
		assert!(backend.get_wallet_state(chain_id, wallet_id).await.is_none());
		assert!(backend.get_cached_block_height(chain_id, wallet_id).await.is_none());

		// Insert cache
		let cache = WalletStateCache {
			chain_id,
			wallet_id,
			block_height: 12345,
			ledger_state_bytes: vec![1, 2, 3, 4, 5],
			wallet_snapshots: vec![],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 1000,
				tblock_err: 30,
				parent_block_hash: [0u8; 32],
			},
			state_root: Some(vec![9, 8, 7]),
			version: CACHE_VERSION.to_string(),
		};
		backend.set_wallet_state(chain_id, wallet_id, cache.clone()).await;

		// Retrieve and verify
		let retrieved = backend.get_wallet_state(chain_id, wallet_id).await;
		assert!(retrieved.is_some());
		let retrieved = retrieved.unwrap();
		assert_eq!(retrieved.block_height, 12345);
		assert_eq!(retrieved.ledger_state_bytes, vec![1, 2, 3, 4, 5]);

		// Check height shortcut
		let height = backend.get_cached_block_height(chain_id, wallet_id).await;
		assert_eq!(height, Some(12345));
	}

	#[tokio::test]
	async fn test_redb_backend_delete() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test_wallet_cache_delete.redb");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::random();
		let wallet_id = H256::random();

		let cache = WalletStateCache {
			chain_id,
			wallet_id,
			block_height: 500,
			ledger_state_bytes: vec![],
			wallet_snapshots: vec![],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 0,
				tblock_err: 0,
				parent_block_hash: [0u8; 32],
			},
			state_root: None,
			version: CACHE_VERSION.to_string(),
		};

		backend.set_wallet_state(chain_id, wallet_id, cache).await;
		assert!(backend.get_wallet_state(chain_id, wallet_id).await.is_some());

		backend.delete_wallet_state(chain_id, wallet_id).await;
		assert!(backend.get_wallet_state(chain_id, wallet_id).await.is_none());
	}

	#[tokio::test]
	async fn test_redb_backend_overwrite() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test_wallet_cache_overwrite.redb");
		let backend = RedbBackend::new(&db_path);

		let chain_id = H256::random();
		let wallet_id = H256::random();

		// Insert first version
		let cache1 = WalletStateCache {
			chain_id,
			wallet_id,
			block_height: 100,
			ledger_state_bytes: vec![1],
			wallet_snapshots: vec![],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 0,
				tblock_err: 0,
				parent_block_hash: [0u8; 32],
			},
			state_root: None,
			version: CACHE_VERSION.to_string(),
		};
		backend.set_wallet_state(chain_id, wallet_id, cache1).await;

		// Overwrite with second version
		let cache2 = WalletStateCache {
			chain_id,
			wallet_id,
			block_height: 200,
			ledger_state_bytes: vec![2, 2],
			wallet_snapshots: vec![],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 0,
				tblock_err: 0,
				parent_block_hash: [0u8; 32],
			},
			state_root: None,
			version: CACHE_VERSION.to_string(),
		};
		backend.set_wallet_state(chain_id, wallet_id, cache2).await;

		// Verify overwrite
		let retrieved = backend.get_wallet_state(chain_id, wallet_id).await.unwrap();
		assert_eq!(retrieved.block_height, 200);
		assert_eq!(retrieved.ledger_state_bytes, vec![2, 2]);
	}

	#[tokio::test]
	async fn test_redb_backend_persistence() {
		let dir = tempdir().unwrap();
		let db_path = dir.path().join("test_wallet_cache_persist.redb");
		let chain_id = H256::random();
		let wallet_id = H256::random();

		// Create, insert, and drop
		{
			let backend = RedbBackend::new(&db_path);
			let cache = WalletStateCache {
				chain_id,
				wallet_id,
				block_height: 999,
				ledger_state_bytes: vec![42],
				wallet_snapshots: vec![],
				latest_block_context: SerializableBlockContext {
					tblock_secs: 0,
					tblock_err: 0,
					parent_block_hash: [0u8; 32],
				},
				state_root: None,
				version: CACHE_VERSION.to_string(),
			};
			backend.set_wallet_state(chain_id, wallet_id, cache).await;
		}

		// Reopen and verify data persisted
		{
			let backend = RedbBackend::new(&db_path);
			let retrieved = backend.get_wallet_state(chain_id, wallet_id).await.unwrap();
			assert_eq!(retrieved.block_height, 999);
			assert_eq!(retrieved.ledger_state_bytes, vec![42]);
		}
	}
}

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

//! Wallet state caching for the toolkit.
//!
//! This module provides the [`WalletStateStorage`] trait and implementations for persisting
//! wallet state across toolkit sessions. By caching the [`LedgerState`] and wallet components,
//! subsequent sessions can restore from the cache and only replay new blocks since the
//! checkpoint, dramatically reducing startup time.
//!
//! # Backends
//!
//! - [`InMemory`] - RAM-only storage for testing (no persistence)
//! - [`RedbBackend`](redb_backend::RedbBackend) - File-based storage using redb
//! - [`PostgresBackend`](postgres_backend::PostgresBackend) - PostgreSQL storage for multi-instance deployments

pub mod cache_helpers;
pub mod postgres_backend;
pub mod redb_backend;

use async_trait::async_trait;
use midnight_node_ledger_helpers::BlockContext;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use subxt::utils::H256;
use tokio::sync::Mutex;

/// Cache entry for wallet state at a specific block height.
///
/// This structure contains all the serialized state needed to restore a
/// [`LedgerContext`](midnight_node_ledger_helpers::LedgerContext) without replaying
/// all transactions from genesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletStateCache {
	/// Chain identity (block 1 hash) - ensures cache is not applied to wrong network
	pub chain_id: H256,

	/// Wallet identity - hash of wallet public keys
	pub wallet_id: H256,

	/// Block height at which this cache was created
	pub block_height: u64,

	/// Serialized LedgerState (using mn_ledger_serialize)
	pub ledger_state_bytes: Vec<u8>,

	/// Snapshots of each wallet's state
	pub wallet_snapshots: Vec<WalletSnapshot>,

	/// Latest block context at cache time
	pub latest_block_context: SerializableBlockContext,

	/// State root hash for integrity verification
	pub state_root: Option<Vec<u8>>,

	/// Version tag for cache format compatibility
	pub version: String,
}

/// Serializable representation of BlockContext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableBlockContext {
	pub tblock_secs: u64,
	pub tblock_err: u64,
	pub parent_block_hash: [u8; 32],
}

impl From<&BlockContext> for SerializableBlockContext {
	fn from(ctx: &BlockContext) -> Self {
		Self {
			tblock_secs: ctx.tblock.to_secs(),
			tblock_err: ctx.tblock_err as u64,
			parent_block_hash: ctx.parent_block_hash.0,
		}
	}
}

/// Snapshot of a single wallet's state for caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSnapshot {
	/// Hash of the wallet seed (for matching on restore)
	pub seed_hash: H256,

	/// Serialized WalletState<D> (shielded coin tracking)
	pub shielded_state_bytes: Vec<u8>,

	/// Serialized DustLocalState<D> (DUST tracking), if present
	pub dust_local_state_bytes: Option<Vec<u8>>,
}

/// Cache key combining chain and wallet identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct WalletCacheKey {
	pub chain_id: H256,
	pub wallet_id: H256,
}

impl WalletCacheKey {
	pub fn new(chain_id: H256, wallet_id: H256) -> Self {
		Self { chain_id, wallet_id }
	}

	/// Create a byte representation for storage backends.
	pub fn to_bytes(&self) -> Vec<u8> {
		[self.chain_id.as_bytes(), self.wallet_id.as_bytes()].concat()
	}
}

/// Compute a wallet identity hash from public key material.
///
/// The wallet identity is derived from the shielded coin public key and dust public key,
/// ensuring the cache is only restored for the correct wallet.
pub fn compute_wallet_id(shielded_coin_pub_key: &[u8; 32], dust_pub_key: &[u8]) -> H256 {
	let mut hasher = Sha256::new();
	hasher.update(shielded_coin_pub_key);
	hasher.update(dust_pub_key);
	H256::from_slice(&hasher.finalize())
}

/// Current cache format version. Increment when format changes.
pub const CACHE_VERSION: &str = "wallet-state-cache-v1";

/// Storage backend for wallet state caching.
///
/// Provides methods to store and retrieve [`WalletStateCache`] by chain ID and wallet ID.
/// Implementations should handle serialization and persistence details.
#[async_trait]
pub trait WalletStateStorage: Send + Sync {
	/// Retrieve cached wallet state for the given chain and wallet.
	///
	/// Returns `None` if no cache exists or if the cache is invalid/corrupted.
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache>;

	/// Store wallet state cache.
	///
	/// Overwrites any existing cache for the same chain/wallet combination.
	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache);

	/// Get the cached block height for a chain/wallet pair.
	///
	/// Returns `None` if no cache exists. This is a lightweight check
	/// to determine if restoration is possible without loading the full cache.
	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64>;

	/// Delete cached wallet state.
	///
	/// Used for cache invalidation when state root verification fails.
	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256);
}

/// In-memory implementation of [`WalletStateStorage`].
///
/// Useful for testing. Does not persist across process restarts.
#[derive(Clone, Default)]
pub struct InMemory {
	cache: Arc<Mutex<HashMap<WalletCacheKey, WalletStateCache>>>,
}

impl InMemory {
	pub fn new() -> Self {
		Self::default()
	}
}

#[async_trait]
impl WalletStateStorage for InMemory {
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let guard = self.cache.lock().await;
		guard.get(&key).cloned()
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let mut guard = self.cache.lock().await;
		guard.insert(key, cache);
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let guard = self.cache.lock().await;
		guard.get(&key).map(|c| c.block_height)
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		let mut guard = self.cache.lock().await;
		guard.remove(&key);
	}
}

/// Configuration for wallet state caching backend.
///
/// Similar to [`FetchCacheConfig`](crate::tx_generator::source::FetchCacheConfig),
/// this enum specifies which storage backend to use for wallet state caching.
#[derive(Clone, Debug)]
pub enum WalletCacheConfig {
	/// No caching - wallet state is always rebuilt from genesis.
	Disabled,
	/// In-memory caching (no persistence across restarts).
	InMemory,
	/// File-based caching using redb embedded database.
	Redb { filename: String },
	/// PostgreSQL-based caching for multi-instance deployments.
	Postgres { database_url: String },
}

impl Default for WalletCacheConfig {
	fn default() -> Self {
		Self::Disabled
	}
}

/// Error parsing wallet cache configuration.
#[derive(Debug, thiserror::Error)]
pub enum WalletCacheConfigParseError {
	#[error("unknown prefix for wallet cache config: {0}")]
	UnknownPrefix(String),
}

impl std::str::FromStr for WalletCacheConfig {
	type Err = WalletCacheConfigParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (prefix, opts) = match s.split_once(':') {
			Some((p, o)) => (p.to_string(), o.to_string()),
			None => (s.to_string(), String::new()),
		};

		match prefix.as_str() {
			"disabled" | "none" | "off" => Ok(Self::Disabled),
			"inmemory" => Ok(Self::InMemory),
			"redb" => Ok(Self::Redb { filename: opts }),
			"postgres" => Ok(Self::Postgres { database_url: s.to_string() }),
			_ => Err(WalletCacheConfigParseError::UnknownPrefix(prefix)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_inmemory_cache_miss() {
		let storage = InMemory::new();
		let chain_id = H256::random();
		let wallet_id = H256::random();

		assert!(storage.get_wallet_state(chain_id, wallet_id).await.is_none());
		assert!(storage.get_cached_block_height(chain_id, wallet_id).await.is_none());
	}

	#[tokio::test]
	async fn test_inmemory_cache_hit() {
		let storage = InMemory::new();
		let chain_id = H256::random();
		let wallet_id = H256::random();

		let cache = WalletStateCache {
			chain_id,
			wallet_id,
			block_height: 1000,
			ledger_state_bytes: vec![1, 2, 3],
			wallet_snapshots: vec![],
			latest_block_context: SerializableBlockContext {
				tblock_secs: 12345,
				tblock_err: 30,
				parent_block_hash: [0u8; 32],
			},
			state_root: Some(vec![4, 5, 6]),
			version: CACHE_VERSION.to_string(),
		};

		storage.set_wallet_state(chain_id, wallet_id, cache.clone()).await;

		let retrieved = storage.get_wallet_state(chain_id, wallet_id).await;
		assert!(retrieved.is_some());
		let retrieved = retrieved.unwrap();
		assert_eq!(retrieved.block_height, 1000);
		assert_eq!(retrieved.ledger_state_bytes, vec![1, 2, 3]);

		let height = storage.get_cached_block_height(chain_id, wallet_id).await;
		assert_eq!(height, Some(1000));
	}

	#[tokio::test]
	async fn test_inmemory_delete() {
		let storage = InMemory::new();
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

		storage.set_wallet_state(chain_id, wallet_id, cache).await;
		assert!(storage.get_wallet_state(chain_id, wallet_id).await.is_some());

		storage.delete_wallet_state(chain_id, wallet_id).await;
		assert!(storage.get_wallet_state(chain_id, wallet_id).await.is_none());
	}

	#[test]
	fn test_compute_wallet_id() {
		let coin_pub = [1u8; 32];
		let dust_pub = [2u8; 16];

		let id1 = compute_wallet_id(&coin_pub, &dust_pub);
		let id2 = compute_wallet_id(&coin_pub, &dust_pub);
		assert_eq!(id1, id2);

		let different_coin = [3u8; 32];
		let id3 = compute_wallet_id(&different_coin, &dust_pub);
		assert_ne!(id1, id3);
	}

	#[test]
	fn test_wallet_cache_key_bytes() {
		let chain_id = H256::from([1u8; 32]);
		let wallet_id = H256::from([2u8; 32]);
		let key = WalletCacheKey::new(chain_id, wallet_id);

		let bytes = key.to_bytes();
		assert_eq!(bytes.len(), 64);
		assert_eq!(&bytes[..32], chain_id.as_bytes());
		assert_eq!(&bytes[32..], wallet_id.as_bytes());
	}
}

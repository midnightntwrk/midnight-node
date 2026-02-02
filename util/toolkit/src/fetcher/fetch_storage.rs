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

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use subxt::utils::H256;
use tokio::sync::Mutex;

use super::{MidnightBlock, wallet_state_cache::WalletStateCache};
use async_trait::async_trait;
use midnight_node_ledger_helpers::{
	BlockContext, DB, ProofKind, SerdeTransaction, SignatureKind, Tagged,
};

pub mod postgres_backend;
pub mod redb_backend;

// Re-export for convenience
pub use super::wallet_state_cache::{WalletCacheKey, WalletStateCache as WalletCache};

/// Trait for wallet state caching operations.
///
/// This is a simpler trait without the complex type bounds of `FetchStorage`,
/// making it easier to use in contexts where only wallet caching is needed.
#[async_trait]
pub trait WalletStateCaching: Send + Sync {
	/// Retrieve cached wallet state for the given chain and wallet.
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache>;

	/// Store wallet state cache.
	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache);

	/// Get the cached block height for a chain/wallet pair.
	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64>;

	/// Delete cached wallet state.
	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256);
}

#[derive(Clone)]
pub struct FetchedBlock {
	pub block: MidnightBlock,
	pub state_root: Option<Vec<u8>>,
}

pub type FetchedTransaction<S, P, D> = SerdeTransaction<S, P, D>;

/// Block data stored by [`FetchStorage`] implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB> {
	pub hash: H256,
	pub parent_hash: H256,
	pub number: u64,
	#[serde(bound(
		deserialize = "Vec<FetchedTransaction<S, P, D>>: Deserialize<'de>",
		serialize = "Vec<FetchedTransaction<S, P, D>>: Serialize"
	))]
	pub transactions: Vec<FetchedTransaction<S, P, D>>,
	pub context: BlockContext,
	pub state_root: Option<Vec<u8>>,
}

/// Storage backend for fetched block data and wallet state caching.
///
/// Provides methods to store and retrieve [`BlockData`] by chain ID and block number,
/// as well as tracking the highest verified block per chain.
///
/// Also provides methods for wallet state caching to enable fast session restoration
/// without replaying all transactions from genesis.
#[async_trait]
pub trait FetchStorage<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB + Clone> {
	// =========================================================================
	// Block data methods (existing)
	// =========================================================================

	async fn get_block_data(&self, chain_id: H256, block_number: u64)
	-> Option<BlockData<S, P, D>>;
	async fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> Vec<Option<BlockData<S, P, D>>> {
		let block_stream = stream::iter(
			range.map(async |block_number| self.get_block_data(chain_id, block_number).await),
		);
		let buffered = block_stream.buffered(10);
		buffered.collect().await
	}

	async fn insert_block_data(&self, chain_id: H256, block_number: u64, block: BlockData<S, P, D>);
	async fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, BlockData<S, P, D>)> + Send,
	) {
		let block_stream = stream::iter(range.map(async |(block_number, block)| {
			self.insert_block_data(chain_id, block_number, block).await
		}));
		let buffered = block_stream.buffer_unordered(10);
		buffered.collect().await
	}
	async fn get_highest_verified_block(&self, chain_id: H256) -> Option<u64>;
	async fn set_highest_verified_block(&self, chain_id: H256, height: u64);

	// =========================================================================
	// Wallet state caching methods (new)
	// =========================================================================

	/// Retrieve cached wallet state for the given chain and wallet.
	///
	/// Returns `None` if no cache exists or if the cache is invalid/corrupted.
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		let _ = (chain_id, wallet_id);
		None // Default: no caching support
	}

	/// Store wallet state cache.
	///
	/// Overwrites any existing cache for the same chain/wallet combination.
	/// Uses zstd compression to reduce storage size.
	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		let _ = (chain_id, wallet_id, cache);
		// Default: no-op (caching not supported)
	}

	/// Get the cached block height for a chain/wallet pair.
	///
	/// Returns `None` if no cache exists. This is a lightweight check
	/// to determine if restoration is possible without loading the full cache.
	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		let _ = (chain_id, wallet_id);
		None // Default: no caching support
	}

	/// Delete cached wallet state.
	///
	/// Used for cache invalidation when state root verification fails.
	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		let _ = (chain_id, wallet_id);
		// Default: no-op
	}
}

#[derive(Clone)]
pub struct InMemory<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB> {
	highest_verified: Arc<Mutex<HashMap<H256, u64>>>,
	blocks: Arc<Mutex<HashMap<Vec<u8>, BlockData<S, P, D>>>>,
	wallet_cache: Arc<Mutex<HashMap<WalletCacheKey, WalletStateCache>>>,
}

impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D>> Default for InMemory<S, P, D> {
	fn default() -> Self {
		Self {
			highest_verified: Arc::new(Mutex::new(HashMap::new())),
			blocks: Arc::new(Mutex::new(HashMap::new())),
			wallet_cache: Arc::new(Mutex::new(HashMap::new())),
		}
	}
}

impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D>> InMemory<S, P, D> {
	fn block_key(chain_id: &[u8], block_number: u64) -> Vec<u8> {
		[chain_id, b":", &block_number.to_be_bytes()[..]].concat()
	}
}

#[async_trait]
impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D>> FetchStorage<S, P, D>
	for InMemory<S, P, D>
{
	async fn get_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
	) -> Option<BlockData<S, P, D>> {
		let k = Self::block_key(&chain_id.0, block_number);
		self.blocks.lock().await.get(&k).cloned()
	}
	async fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> Vec<Option<BlockData<S, P, D>>> {
		let blocks = self.blocks.lock().await;
		range
			.map(|block_number| {
				let k = Self::block_key(&chain_id.0, block_number);
				blocks.get(&k).cloned()
			})
			.collect()
	}

	async fn insert_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
		block: BlockData<S, P, D>,
	) {
		let k = Self::block_key(&chain_id.0, block_number);
		self.blocks.lock().await.insert(k, block);
	}
	async fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, BlockData<S, P, D>)> + Send,
	) {
		let mut blocks = self.blocks.lock().await;
		range.for_each(|(block_number, block)| {
			let k = Self::block_key(&chain_id.0, block_number);
			blocks.insert(k, block);
		});
	}

	async fn get_highest_verified_block(&self, chain_id: H256) -> Option<u64> {
		self.highest_verified.lock().await.get(&chain_id).cloned()
	}

	async fn set_highest_verified_block(&self, chain_id: H256, height: u64) {
		self.highest_verified.lock().await.insert(chain_id, height);
	}

	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		self.wallet_cache.lock().await.get(&key).cloned()
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		self.wallet_cache.lock().await.insert(key, cache);
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		self.wallet_cache.lock().await.get(&key).map(|c| c.block_height)
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		let key = WalletCacheKey::new(chain_id, wallet_id);
		self.wallet_cache.lock().await.remove(&key);
	}
}

// Implement WalletStateCaching for InMemory (delegates to FetchStorage impl)
#[async_trait]
impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D>> WalletStateCaching
	for InMemory<S, P, D>
{
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		<Self as FetchStorage<S, P, D>>::get_wallet_state(self, chain_id, wallet_id).await
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		<Self as FetchStorage<S, P, D>>::set_wallet_state(self, chain_id, wallet_id, cache).await
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		<Self as FetchStorage<S, P, D>>::get_cached_block_height(self, chain_id, wallet_id).await
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		<Self as FetchStorage<S, P, D>>::delete_wallet_state(self, chain_id, wallet_id).await
	}
}

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

use futures::stream::{self, StreamExt};
use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;
use std::{collections::HashMap, future::Future, sync::Arc};
use subxt::utils::H256;
use tokio::sync::Mutex;

use super::{MidnightBlock, wallet_state_cache::WalletStateCache};

pub mod postgres_backend;
pub mod redb_backend;

// Re-export for convenience
pub use super::wallet_state_cache::{WalletCacheKey, WalletStateCache as WalletCache};

/// Trait for wallet state caching operations.
///
/// This is a simpler trait without the complex type bounds of `FetchStorage`,
/// making it easier to use in contexts where only wallet caching is needed.
pub trait WalletStateCaching: Send + Sync {
	/// Retrieve cached wallet state for the given chain and wallet.
	fn get_wallet_state(
		&self,
		chain_id: H256,
		wallet_id: H256,
	) -> impl Future<Output = Option<WalletStateCache>> + Send;

	/// Store wallet state cache.
	fn set_wallet_state(
		&self,
		chain_id: H256,
		wallet_id: H256,
		cache: WalletStateCache,
	) -> impl Future<Output = ()> + Send;

	/// Get the cached block height for a chain/wallet pair.
	fn get_cached_block_height(
		&self,
		chain_id: H256,
		wallet_id: H256,
	) -> impl Future<Output = Option<u64>> + Send;

	/// Delete cached wallet state.
	fn delete_wallet_state(
		&self,
		chain_id: H256,
		wallet_id: H256,
	) -> impl Future<Output = ()> + Send;
}

#[derive(Clone)]
pub struct FetchedBlock {
	pub block: MidnightBlock,
	pub state_root: Option<Vec<u8>>,
	pub state: Option<Vec<u8>>,
}

/// Storage backend for fetched block data and wallet state caching.
///
/// Provides methods to store and retrieve [`RawBlockData`] by chain ID and block number,
/// as well as tracking the highest verified block per chain.
///
/// Also provides methods for wallet state caching to enable fast session restoration
/// without replaying all transactions from genesis.
pub trait FetchStorage: Send + Sync {
	// =========================================================================
	// Block data methods
	// =========================================================================

	fn get_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
	) -> impl Future<Output = Option<RawBlockData>> + Send;

	fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> impl Future<Output = Vec<Option<RawBlockData>>> + Send {
		async move {
			let block_stream =
				stream::iter(range.map(|block_number| self.get_block_data(chain_id, block_number)));
			let buffered = block_stream.buffered(10);
			buffered.collect().await
		}
	}

	fn insert_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
		block: RawBlockData,
	) -> impl Future<Output = ()> + Send;

	fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, RawBlockData)> + Send,
	) -> impl Future<Output = ()> + Send {
		async move {
			let block_stream = stream::iter(range.map(|(block_number, block)| {
				self.insert_block_data(chain_id, block_number, block)
			}));
			let buffered = block_stream.buffer_unordered(10);
			buffered.collect().await
		}
	}

	fn get_highest_verified_block(
		&self,
		chain_id: H256,
	) -> impl Future<Output = Option<u64>> + Send;

	fn set_highest_verified_block(
		&self,
		chain_id: H256,
		height: u64,
	) -> impl Future<Output = ()> + Send;

	// =========================================================================
	// Wallet state caching methods
	// =========================================================================

	/// Retrieve cached wallet state for the given chain and wallet.
	fn get_wallet_state(
		&self,
		chain_id: H256,
		wallet_id: H256,
	) -> impl Future<Output = Option<WalletStateCache>> + Send {
		async move {
			let _ = (chain_id, wallet_id);
			None
		}
	}

	/// Store wallet state cache.
	fn set_wallet_state(
		&self,
		chain_id: H256,
		wallet_id: H256,
		cache: WalletStateCache,
	) -> impl Future<Output = ()> + Send {
		async move {
			let _ = (chain_id, wallet_id, cache);
		}
	}

	/// Get the cached block height for a chain/wallet pair.
	fn get_cached_block_height(
		&self,
		chain_id: H256,
		wallet_id: H256,
	) -> impl Future<Output = Option<u64>> + Send {
		async move {
			let _ = (chain_id, wallet_id);
			None
		}
	}

	/// Delete cached wallet state.
	fn delete_wallet_state(
		&self,
		chain_id: H256,
		wallet_id: H256,
	) -> impl Future<Output = ()> + Send {
		async move {
			let _ = (chain_id, wallet_id);
		}
	}
}

#[derive(Clone)]
pub struct InMemory {
	highest_verified: Arc<Mutex<HashMap<H256, u64>>>,
	blocks: Arc<Mutex<HashMap<Vec<u8>, RawBlockData>>>,
	wallet_cache: Arc<Mutex<HashMap<WalletCacheKey, WalletStateCache>>>,
}

impl Default for InMemory {
	fn default() -> Self {
		Self {
			highest_verified: Arc::new(Mutex::new(HashMap::new())),
			blocks: Arc::new(Mutex::new(HashMap::new())),
			wallet_cache: Arc::new(Mutex::new(HashMap::new())),
		}
	}
}

impl InMemory {
	fn block_key(chain_id: &[u8], block_number: u64) -> Vec<u8> {
		[chain_id, b":", &block_number.to_be_bytes()[..]].concat()
	}
}

impl FetchStorage for InMemory {
	async fn get_block_data(&self, chain_id: H256, block_number: u64) -> Option<RawBlockData> {
		let k = Self::block_key(&chain_id.0, block_number);
		self.blocks.lock().await.get(&k).cloned()
	}
	async fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> Vec<Option<RawBlockData>> {
		let blocks = self.blocks.lock().await;
		range
			.map(|block_number| {
				let k = Self::block_key(&chain_id.0, block_number);
				blocks.get(&k).cloned()
			})
			.collect()
	}

	async fn insert_block_data(&self, chain_id: H256, block_number: u64, block: RawBlockData) {
		let k = Self::block_key(&chain_id.0, block_number);
		self.blocks.lock().await.insert(k, block);
	}
	async fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, RawBlockData)> + Send,
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
impl WalletStateCaching for InMemory {
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

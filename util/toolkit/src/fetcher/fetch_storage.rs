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

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;
use std::{collections::HashMap, future::Future, sync::Arc};
use subxt::utils::H256;
use tokio::sync::Mutex;

use super::MidnightBlock;
use super::wallet_state_cache::{CachedWalletState, LedgerSnapshot};

pub mod file_backend;
pub mod postgres_backend;
pub mod redb_backend;

/// Trait for per-wallet state caching operations.
///
/// Separates ledger snapshots (one per block height, ~49MB) from individual
/// wallet state (~5-15KB), eliminating redundant storage when multiple wallets
/// share the same chain state.
#[async_trait]
pub trait WalletStateCaching: Send + Sync {
	/// Retrieve a ledger snapshot at a specific block height.
	async fn get_ledger_snapshot(
		&self,
		chain_id: H256,
		block_height: u64,
	) -> Option<LedgerSnapshot>;

	/// Store a ledger snapshot.
	async fn set_ledger_snapshot(&self, chain_id: H256, snapshot: LedgerSnapshot);

	/// Get the latest (highest) ledger snapshot height for a chain.
	async fn get_latest_ledger_height(&self, chain_id: H256) -> Option<u64>;

	/// Batch-retrieve wallet states by seed hash. Returns `None` for uncached wallets.
	/// Implementors must keep the ordering in return value
	/// seed_hashes.len() == result.len()
	async fn get_wallet_states(
		&self,
		chain_id: H256,
		seed_hashes: &[H256],
	) -> Vec<Option<CachedWalletState>>;

	/// Batch-store wallet states.
	async fn set_wallet_states(&self, chain_id: H256, wallets: &[CachedWalletState]);

	/// Delete wallet states by seed hash.
	async fn delete_wallet_states(&self, chain_id: H256, seed_hashes: &[H256]);

	/// Remove ledger snapshots not referenced by any wallet cache entry.
	/// Keeps only snapshots at the specified heights.
	async fn gc_ledger_snapshots(&self, chain_id: H256, keep_heights: &[u64]);

	/// Return all distinct block heights referenced by cached wallets for a chain.
	async fn get_all_cached_wallet_heights(&self, chain_id: H256) -> Vec<u64>;
}

#[derive(Clone)]
pub struct FetchedBlock {
	pub block: MidnightBlock,
	pub raw_body: Vec<Vec<u8>>,
	pub state_root: Option<Vec<u8>>,
	pub state: Option<Vec<u8>>,
}

/// Storage backend for fetched block data.
///
/// Provides methods to store and retrieve [`RawBlockData`] by chain ID and block number,
/// as well as tracking the highest verified block per chain.
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
}

#[derive(Clone, Default)]
pub struct InMemory {
	highest_verified: Arc<Mutex<HashMap<H256, u64>>>,
	blocks: Arc<Mutex<HashMap<Vec<u8>, RawBlockData>>>,
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
}

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

use std::{any::type_name, cmp::Ordering, path::Path, sync::Arc};

use async_trait::async_trait;
use core::fmt::Debug;
use midnight_node_ledger_helpers::{DB, ProofKind, SignatureKind, Tagged};
use redb::{Database, Key, ReadableDatabase, TableDefinition, TypeName, Value};
use serde::{Deserialize, Serialize};
use subxt::utils::H256;
use tokio::sync::RwLock;

use super::{BlockData, FetchStorage};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockKey {
	chain_id: H256,
	block_number: u64,
}

/// Persistent [`FetchStorage`] backend using [redb](https://github.com/cberner/redb).
///
/// Data is serialized as BSON. Uses `RwLock` for concurrent read access.
#[derive(Clone)]
pub struct RedbBackend<S: SignatureKind<D> + Tagged, P: ProofKind<D> + Debug, D: DB> {
	pub db: Arc<RwLock<Database>>,
	pub block_data_table: TableDefinition<'static, Serde<BlockKey>, Serde<BlockData<S, P, D>>>,
	pub highest_verified_table: TableDefinition<'static, [u8; 32], u64>,
}

impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D> + Debug> RedbBackend<S, P, D> {
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
			block_data_table: TableDefinition::new("block_data"),
			highest_verified_table: TableDefinition::new("highest_verified"),
		}
	}
}

#[async_trait]
impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D> + Debug> FetchStorage<S, P, D>
	for RedbBackend<S, P, D>
{
	async fn get_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
	) -> Option<BlockData<S, P, D>> {
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
	) -> Vec<Option<BlockData<S, P, D>>> {
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

	async fn insert_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
		block: BlockData<S, P, D>,
	) {
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
		range: impl Iterator<Item = (u64, BlockData<S, P, D>)> + Send,
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

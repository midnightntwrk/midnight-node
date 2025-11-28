use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use subxt::utils::H256;
use tokio::sync::Mutex;

use super::MidnightBlock;
use async_trait::async_trait;
use midnight_node_ledger_helpers::{BlockContext, DB, ProofMarker, SerdeTransaction, Signature};

pub mod redb_backend;

#[derive(Clone)]
pub struct FetchedBlock {
	pub block: MidnightBlock,
	pub state_root: Option<Vec<u8>>,
}

pub type FetchedTransaction<D> = SerdeTransaction<Signature, ProofMarker, D>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData<D: DB> {
	pub hash: H256,
	pub parent_hash: H256,
	pub number: u64,
	#[serde(bound(
		deserialize = "Vec<FetchedTransaction<D>>: Deserialize<'de>",
		serialize = "Vec<FetchedTransaction<D>>: Serialize"
	))]
	pub transactions: Vec<FetchedTransaction<D>>,
	pub context: BlockContext,
	pub state_root: Option<Vec<u8>>,
}

#[async_trait]
pub trait FetchStorage<D: DB + Clone> {
	async fn get_block_data(&self, chain_id: H256, block_number: u64) -> Option<BlockData<D>>;
	async fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> Vec<Option<BlockData<D>>> {
		let block_stream = stream::iter(
			range.map(async |block_number| self.get_block_data(chain_id, block_number).await),
		);
		let buffered = block_stream.buffered(10);
		buffered.collect().await
	}

	async fn insert_block_data(&self, chain_id: H256, block_number: u64, block: BlockData<D>);
	async fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, BlockData<D>)> + Send,
	) {
		let block_stream = stream::iter(range.map(async |(block_number, block)| {
			self.insert_block_data(chain_id, block_number, block).await
		}));
		let buffered = block_stream.buffer_unordered(10);
		buffered.collect().await
	}

	async fn flush_all(&self);
}

#[derive(Default, Clone)]
pub struct InMemory<D: DB> {
	blocks: Arc<Mutex<HashMap<Vec<u8>, BlockData<D>>>>,
}

impl<D: DB> InMemory<D> {
	fn block_key(chain_id: &[u8], block_number: u64) -> Vec<u8> {
		[chain_id, b":", &block_number.to_be_bytes()[..]].concat()
	}
}

#[async_trait]
impl<D: DB + Clone> FetchStorage<D> for InMemory<D> {
	async fn get_block_data(&self, chain_id: H256, block_number: u64) -> Option<BlockData<D>> {
		let k = Self::block_key(&chain_id.0, block_number);
		self.blocks.lock().await.get(&k).cloned()
	}

	async fn insert_block_data(&self, chain_id: H256, block_number: u64, block: BlockData<D>) {
		let k = Self::block_key(&chain_id.0, block_number);
		self.blocks.lock().await.insert(k, block);
	}

	/// In-memory storage has no persistence, so flush is a no-op.
	async fn flush_all(&self) {}
}

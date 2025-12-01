use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use subxt::utils::H256;
use tokio::sync::Mutex;

use super::MidnightBlock;
use async_trait::async_trait;
use midnight_node_ledger_helpers::{
	BlockContext, DB, ProofKind, SerdeTransaction, SignatureKind, Tagged,
};

pub mod redb_backend;

#[derive(Clone)]
pub struct FetchedBlock {
	pub block: MidnightBlock,
	pub state_root: Option<Vec<u8>>,
}

pub type FetchedTransaction<S, P, D> = SerdeTransaction<S, P, D>;

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

#[async_trait]
pub trait FetchStorage<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB + Clone> {
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

	async fn flush_all(&self);
}

#[derive(Clone)]
pub struct InMemory<S: SignatureKind<D> + Tagged, P: ProofKind<D>, D: DB> {
	blocks: Arc<Mutex<HashMap<Vec<u8>, BlockData<S, P, D>>>>,
}

impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D>> Default for InMemory<S, P, D> {
	fn default() -> Self {
		Self { blocks: Arc::new(Mutex::new(HashMap::new())) }
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

	/// In-memory storage has no persistence, so flush is a no-op.
	async fn flush_all(&self) {}
}

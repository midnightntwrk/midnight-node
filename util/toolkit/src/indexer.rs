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

mod runtimes;

use backoff::{ExponentialBackoff, future::retry};
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{
	collections::{BTreeMap, VecDeque},
	path::{Path, PathBuf},
	sync::{Arc, atomic::AtomicBool},
	time::Duration,
};
use subxt::{
	OnlineClient,
	blocks::{Block, ExtrinsicEvents},
	config::{
		HashFor,
		substrate::{ConsensusEngineId, DigestItem, H256},
	},
};
use thiserror::Error;
use tokio::{
	select,
	sync::{Mutex, Notify, oneshot},
	task::JoinError,
};

use crate::{
	hash_to_str,
	indexer::runtimes::{
		MidnightMetadata, MidnightMetadata0_17_0, MidnightMetadata0_17_1, MidnightMetadata0_18_0,
		RuntimeVersion,
	},
	serde_def::{self, SourceBlockTransactions},
};

use midnight_node_ledger_helpers::{mn_ledger_serialize::tagged_deserialize, *};

use crate::client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig};

fn fatal(msg: String) {
	eprintln!("{}", msg);
	eprintln!("exiting...");
	std::process::exit(1);
}

#[derive(Error, Debug)]
pub enum IndexerError {
	#[error("indexer already running")]
	AlreadyRunning,
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("subxt_rpc error: {0}")]
	RpcClientError(#[from] subxt::ext::subxt_rpcs::Error),
	#[error("failed to decode transaction hash: {0}")]
	TransactionHashDecodeError(String),
	#[error("failed to decode transaction body: {0}")]
	TransactionBodyDecodeError(String),
	#[error("failed to deserialize transaction: {0}")]
	TransactionDeserializeError(String),
	#[error("invalid block number: {0}")]
	InvalidBlockNumber(u64),
	#[error("indexer failed to start")]
	StartFailed(String),
	#[error("indexer failed to stop")]
	StopFailed(#[from] JoinError),
	#[error("indexer received an unsupported network id")]
	UnsupportedNetworkId(Vec<u8>),
	#[error("indexer received a block with invalid node version: {0}")]
	InvalidProtocolVersion(parity_scale_codec::Error),
	#[error("indexer received a block made with unsupported node version {0}")]
	UnsupportedBlockVersion(u32),
}

struct InternalState<S: SignatureKind<DefaultDB> + Tagged, P: ProofKind<DefaultDB> + Send>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	blocks: VecDeque<SourceBlockTransactions<S, P>>,
}

impl<S: SignatureKind<DefaultDB> + Tagged, P: ProofKind<DefaultDB> + Send> InternalState<S, P>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub fn new() -> Self {
		Self { blocks: VecDeque::new() }
	}
}

pub struct IndexerHandle {
	task_handle: tokio::task::JoinHandle<std::result::Result<(), IndexerError>>,
	stop_chan: oneshot::Sender<bool>,
}

impl IndexerHandle {
	pub async fn stop(self) -> Result<(), IndexerError> {
		let _ = self
			.stop_chan
			.send(true)
			.inspect_err(|e| eprintln!("failed to send stop signal: {e}"));
		self.task_handle.await??;
		Ok(())
	}
}

impl From<ClientError> for IndexerError {
	fn from(err: ClientError) -> Self {
		match err {
			ClientError::SubxtError(err) => Self::SubxtError(err),
			ClientError::RpcClientError(err) => Self::RpcClientError(err),
			ClientError::UnsupportedNetworkId(bytes) => Self::UnsupportedNetworkId(bytes),
		}
	}
}

#[derive(Serialize, Deserialize)]
struct SyncCache<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	genesis: HashFor<MidnightNodeClientConfig>,
	/// Block hash and number
	until: Option<(HashFor<MidnightNodeClientConfig>, u64)>,
	#[serde(with = "serde_def::block_vec")]
	blocks: Vec<SourceBlockTransactions<S, P>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>> SyncCache<S, P>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	fn dir() -> String {
		std::env::var("MN_SYNC_CACHE").unwrap_or(".sync_cache".to_string())
	}

	fn filename(genesis: HashFor<MidnightNodeClientConfig>) -> PathBuf {
		Path::new(&Self::dir()).join(format!("{}.bin", hash_to_str(genesis)))
	}

	pub fn load(genesis: HashFor<MidnightNodeClientConfig>) -> Result<Self, IndexerError> {
		let default = SyncCache { genesis, until: None, blocks: Vec::new() };

		let dir = Self::dir();
		if std::fs::create_dir_all(&dir).is_err() {
			eprintln!("Failed to create sync cache dir {}", dir);
			return Ok(default);
		}

		let cache_filename = Self::filename(genesis);

		let data: Option<SyncCache<S, P>> = std::fs::File::open(&cache_filename)
			.map(|f| {
				eprintln!("sync cache detected, loading...");
				match bincode::deserialize_from(&f) {
					Ok(cache) => Some(cache),
					Err(e) => {
						eprintln!("error reading sync file: {:?}", e);
						eprintln!("will attempt to delete");
						None
					},
				}
			})
			.unwrap_or(None);

		if let Some(data) = data {
			Ok(data)
		} else {
			let _ = std::fs::remove_file(&cache_filename);
			Ok(default)
		}
	}

	pub fn blocks(&self) -> Vec<SourceBlockTransactions<S, P>> {
		self.blocks.clone()
	}

	pub fn save(
		&mut self,
		hash: HashFor<MidnightNodeClientConfig>,
		number: u64,
		blocks: &[SourceBlockTransactions<S, P>],
	) {
		self.until = Some((hash, number));
		self.blocks = blocks.to_vec();
		self.save_to_file();
	}

	fn save_to_file(&self) {
		// Attempt to write to file
		let s = bincode::serialize(self).unwrap();
		match std::fs::write(Self::filename(self.genesis), s) {
			Ok(_) => (),
			Err(e) => eprintln!("failed to write sync cache: {}", e),
		}
	}
}

pub struct Indexer<S: SignatureKind<DefaultDB> + Tagged, P: ProofKind<DefaultDB> + Send + 'static>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	state: Mutex<InternalState<S, P>>,
	looping: AtomicBool,
	fetch_concurrency: usize,
	notify_sync: Notify,
	node_client: MidnightNodeClient,
}

impl<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static,
> Indexer<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub async fn new(
		node_client: MidnightNodeClient,
		fetch_concurrency: usize,
	) -> Result<Self, IndexerError> {
		Ok(Indexer {
			state: Mutex::new(InternalState::new()),
			looping: AtomicBool::new(false),
			fetch_concurrency,
			notify_sync: Notify::new(),
			node_client,
		})
	}

	pub async fn get_blocks(&self) -> Vec<SourceBlockTransactions<S, P>> {
		let s = self.state.lock().await;
		s.blocks.clone().into()
	}

	pub async fn start(self: &Arc<Self>) -> Result<IndexerHandle, IndexerError> {
		if !self.looping.load(std::sync::atomic::Ordering::Relaxed) {
			self.looping.store(true, std::sync::atomic::Ordering::Relaxed);
			let (tx_start, rx_start) = oneshot::channel();
			let (tx_stop, rx_stop) = oneshot::channel();

			let handle = tokio::spawn(self.clone().new_index_loop(tx_start, rx_stop));
			match rx_start.await {
				Ok(_) => Ok(IndexerHandle { task_handle: handle, stop_chan: tx_stop }),
				Err(e) => {
					eprintln!("{:?}", handle.await);
					Err(IndexerError::StartFailed(e.to_string()))
				},
			}
		} else {
			Err(IndexerError::AlreadyRunning)
		}
	}

	async fn process_block(
		self: Arc<Self>,
		block: &Block<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>,
	) -> Result<SourceBlockTransactions<S, P>, IndexerError> {
		let version_number = block
			.header()
			.digest
			.logs
			.iter()
			.find_map(|item| {
				const VERSION_ID: ConsensusEngineId = *b"MNSV";
				if let DigestItem::Consensus(VERSION_ID, data) = item {
					Some(RuntimeVersion::try_from(data.as_slice()))
				} else {
					None
				}
			})
			.expect("no runtime version found")?;
		match version_number {
			RuntimeVersion::V0_17_0 => {
				self.process_block_with_protocol::<MidnightMetadata0_17_0>(block).await
			},
			RuntimeVersion::V0_17_1 => {
				self.process_block_with_protocol::<MidnightMetadata0_17_1>(block).await
			},
			RuntimeVersion::V0_18_0 => {
				self.process_block_with_protocol::<MidnightMetadata0_18_0>(block).await
			},
		}
	}

	async fn process_block_with_protocol<M: MidnightMetadata>(
		self: Arc<Self>,
		block: &Block<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>,
	) -> Result<SourceBlockTransactions<S, P>, IndexerError> {
		let block_hash = block.hash();
		let state_root = self.node_client.get_state_root_at(Some(block_hash)).await?;
		let block_header = block.header();
		let parent_block_hash = block_header.parent_hash;

		let extrinsics = block
			.extrinsics()
			.await
			.unwrap_or_else(|err| panic!("Error while fetching the transactions: {}", err));
		let events = block
			.events()
			.await
			.unwrap_or_else(|err| panic!("Error while fetching the events: {}", err));

		let mut timestamp_ms = None;
		let mut transactions = vec![];
		for ext in extrinsics.iter() {
			let Ok(call) = ext.as_root_extrinsic::<M::Call>() else {
				continue;
			};
			if let Some(ts) = M::timestamp_set(&call) {
				if timestamp_ms.is_some() {
					panic!("this block has two timestamps");
				}
				timestamp_ms = Some(ts);
			} else if let Some(bytes) = M::send_mn_transaction(&call) {
				let tx = tagged_deserialize(&mut bytes.as_slice())
					.map_err(|err| IndexerError::TransactionDeserializeError(err.to_string()))?;
				transactions.push(SerdeTransaction::Midnight(tx));
			} else if let Some(bytes) = M::send_mn_system_transaction(&call) {
				let tx = tagged_deserialize(&mut bytes.as_slice())
					.map_err(|err| IndexerError::TransactionDeserializeError(err.to_string()))?;
				transactions.push(SerdeTransaction::System(tx));
			} else if M::check_for_events(&call) {
				let ext_events = ExtrinsicEvents::new(ext.hash(), ext.index(), events.clone());
				for ev in ext_events.iter().filter_map(Result::ok) {
					if let Some(event) = ev.as_event::<M::SystemTransactionAppliedEvent>()? {
						let bytes = M::system_transaction_applied(event);
						let tx = tagged_deserialize(&mut bytes.as_slice()).map_err(|err| {
							IndexerError::TransactionDeserializeError(err.to_string())
						})?;
						transactions.push(SerdeTransaction::System(tx));
					}
				}
			}
		}

		let timestamp_ms = timestamp_ms.expect("failed to find a timestamp extrinsic in block");
		let context = BlockContext {
			tblock: Timestamp::from_secs(timestamp_ms / 1000),
			tblock_err: 30,
			parent_block_hash: HashOutput(parent_block_hash.0),
		};
		Ok(SourceBlockTransactions { transactions, context, state_root })
	}

	async fn fetch_midnight_block(
		self: Arc<Self>,
		hash: H256,
	) -> Result<Block<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>, subxt::Error>
	{
		self.node_client.api.blocks().at(hash).await
	}

	async fn fetch_until(
		self: Arc<Self>,
		tx_prog: Option<tokio::sync::mpsc::Sender<u32>>,
		start_hash: HashFor<MidnightNodeClientConfig>,
		end_hash: Option<HashFor<MidnightNodeClientConfig>>,
	) -> Result<VecDeque<SourceBlockTransactions<S, P>>, IndexerError> {
		if start_hash == end_hash.unwrap_or(H256::default()) {
			return Ok(VecDeque::new());
		}

		let mut blocks = VecDeque::new();

		let mut mn_block = retry(ExponentialBackoff::default(), || async {
			self.clone().fetch_midnight_block(start_hash).await.map_err(|e| {
				eprintln!("rpc fetch failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await?;

		let mut block_header = mn_block.header();
		let mut block_number = block_header.number;

		loop {
			let block = self.clone().process_block(&mn_block).await?;
			blocks.push_front(block);

			match &tx_prog {
				Some(chan) => chan.send(block_number).await.unwrap(),
				_ => (),
			}

			block_header = mn_block.header();
			block_number = block_header.number;
			let parent_block_hash = mn_block.header().parent_hash;

			if parent_block_hash.is_zero()
				|| parent_block_hash == end_hash.unwrap_or(H256::default())
			{
				break;
			}

			mn_block = retry(ExponentialBackoff::default(), || async {
				self.clone().fetch_midnight_block(parent_block_hash).await.map_err(|e| {
					eprintln!("rpc fetch failed, retrying: {e}");
					backoff::Error::transient(e)
				})
			})
			.await?;
		}

		Ok(blocks)
	}

	async fn fetch_all_blocks(
		self: Arc<Self>,
		from: (HashFor<MidnightNodeClientConfig>, u64),
		to: Option<(HashFor<MidnightNodeClientConfig>, u64)>,
	) -> Result<VecDeque<SourceBlockTransactions<S, P>>, IndexerError> {
		let mut prev_block_hash = Some(from.0);

		let num_blocks = from.1 as f32 - to.map(|to| to.1).unwrap_or(0) as f32;
		let block_div = num_blocks / self.fetch_concurrency as f32;

		let mut futures = vec![];

		let (tx, mut rx) = tokio::sync::mpsc::channel(100);

		for i in 0..self.fetch_concurrency {
			let mut block_hash: Option<HashFor<MidnightNodeClientConfig>> =
				to.map(|to| Some(to.0)).unwrap_or(None);

			if i < self.fetch_concurrency - 1 {
				let block_number = (from.1 as f32 - (block_div * (i + 1) as f32)) as u64;
				let block = self
					.node_client
					.rpc
					.chain_get_block_hash(Some(
						subxt::backend::legacy::rpc_methods::NumberOrHex::Number(block_number),
					))
					.await?;
				block_hash = Some(block.ok_or(IndexerError::InvalidBlockNumber(block_number))?);
			}

			let future = self
				.clone()
				.fetch_until(Some(tx.clone()), prev_block_hash.unwrap(), block_hash)
				.then(move |txs| async move { (i, txs) });
			futures.push(future);
			prev_block_hash = block_hash;
		}

		let total = self.fetch_concurrency;
		let final_handle = tokio::spawn(async move {
			let mut results = BTreeMap::new();
			let mut handles: Vec<_> = futures.into_iter().map(|f| tokio::spawn(f)).collect();

			// We want to unwrap here
			loop {
				let res: Result<
					(usize, Result<VecDeque<SourceBlockTransactions<S, P>>, IndexerError>),
					_,
				>;
				(res, _, handles) = futures::future::select_all(handles).await;
				if let Err(e) = res {
					eprintln!("worker thread failed: {e}");
					eprintln!("exiting...");
					std::process::exit(1);
				}
				let (i, worker_res) =
					res.map_err(|e| fatal(format!("worker thread failed: {e}"))).unwrap();
				let blocks = worker_res
					.map_err(|e| fatal(format!("worker failed while fetching: {e}")))
					.unwrap();
				results.insert(i, blocks);
				if handles.is_empty() {
					break;
				}
			}

			let mut blocks = VecDeque::new();
			// We reverse here to create a list of oldest to youngest txs
			for i in (0..total).rev() {
				let cur_blocks = results.remove(&i).unwrap();
				blocks.extend(cur_blocks);
			}

			blocks
		});

		let total = num_blocks as usize;
		let mut processed = 0;
		let mut start = std::time::Instant::now();

		for r in 0..total {
			let _b = rx.recv().await.unwrap();
			processed += 1;
			if start.elapsed() > Duration::from_secs(1) {
				eprintln!("speed: {processed} per sec ({r}/{total})");
				start = std::time::Instant::now();
				processed = 0;
			}
		}

		let txs = final_handle.await?;

		Ok(txs)
	}

	async fn new_index_loop(
		self: Arc<Self>,
		tx_start: oneshot::Sender<usize>,
		mut rx_stop: oneshot::Receiver<bool>,
	) -> Result<(), IndexerError> {
		let start = std::time::Instant::now();

		// Subscribe to all finalized blocks:
		let mut blocks_sub = self.node_client.api.blocks().subscribe_finalized().await?;

		// Load sync cache
		let genesis_hash = self.node_client.rpc.genesis_hash().await?;
		let mut cache = SyncCache::load(genesis_hash)?;

		// First, index everything from current block to genesis i.e. sync
		let latest_block = self.node_client.api.blocks().at_latest().await?;

		let num_blocks_to_fetch =
			u64::from(latest_block.number()) - cache.until.map(|u| u.1).unwrap_or(0);

		eprintln!(
			"fetching {} -> {}",
			hash_to_str(latest_block.hash()),
			cache.until.map(|u| hash_to_str(u.0)).unwrap_or("genesis".to_string())
		);

		let new_blocks = if num_blocks_to_fetch < 100 {
			self.clone()
				.fetch_until(None, latest_block.hash(), cache.until.map(|c| c.0))
				.await?
		} else {
			self.clone()
				.fetch_all_blocks((latest_block.hash(), latest_block.number().into()), cache.until)
				.await?
		};

		let num_new_txs = new_blocks.iter().map(|b| b.transactions.len()).sum::<usize>();
		let num_cached_blocks: usize;
		let num_txs: usize;

		{
			let mut blocks = cache.blocks();
			num_cached_blocks = blocks.len();
			blocks.extend(new_blocks);
			let mut s = self.state.lock().await;
			cache.save(latest_block.hash(), latest_block.number().into(), &blocks);
			s.blocks = blocks.into();
			num_txs = s.blocks.iter().map(|b| b.transactions.len()).sum::<usize>();
		}

		eprintln!(
			"finished syncing {} transactions from {} blocks in {:.2}s. New blocks: {}, New txs: {}, Cached blocks: {}",
			num_txs,
			latest_block.number(),
			start.elapsed().as_secs_f32(),
			num_blocks_to_fetch,
			num_new_txs,
			num_cached_blocks
		);

		// Finished full sync
		{
			let num_blocks_synced = self.state.lock().await.blocks.len();
			let _ = tx_start
				.send(num_blocks_synced)
				.inspect_err(|e| eprintln!("Sender dropped: {e}"));
		}

		loop {
			select! {
				// Check block subscriptions
				Some(block) = blocks_sub.next() => {
					let block = block?;
					// Get midnight transactions
					let mn_block = self.clone().fetch_midnight_block(block.hash()).await?;
					let block = self.clone().process_block(&mn_block).await?;
					let mut s = self.state.lock().await;
					s.blocks.push_back(block);
					self.notify_sync.notify_waiters();
				},
				// Check for stop signal
				_ = &mut rx_stop => {
					break;
				}
			}
		}

		Ok(())
	}
}

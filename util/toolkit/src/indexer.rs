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

use backoff::{ExponentialBackoff, future::retry};
use futures::FutureExt;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
	collections::{BTreeMap, VecDeque},
	path::{Path, PathBuf},
	sync::{Arc, atomic::AtomicBool},
	time::Duration,
};
use subxt::{
	OnlineClient, PolkadotConfig,
	backend::{legacy::LegacyRpcMethods, rpc::RpcClient},
	blocks::Block,
	config::substrate::H256,
};
use thiserror::Error;
use tokio::{
	select,
	sync::{Mutex, Notify, oneshot},
	task::JoinError,
};

use crate::{hash_to_str, mn_meta, serde_def};

use midnight_node_ledger_helpers::*;

fn fatal(msg: String) {
	eprintln!("{}", msg);
	eprintln!("exiting...");
	std::process::exit(1);
}

pub async fn get_network_id(rpc_url: &str) -> Result<NetworkId, IndexerError> {
	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(rpc_url).await?;
	let storage_query = mn_meta::storage().midnight().network_id();
	let network_id_vec = api.storage().at_latest().await?.fetch(&storage_query).await?;

	// TODO: Update this when we launch testnet/mainnet
	let network_id = if let Some(val) = network_id_vec {
		match val.0.as_slice() {
			[0] => NetworkId::Undeployed,
			[1] => NetworkId::DevNet,
			[2] => NetworkId::TestNet,
			[3] => NetworkId::MainNet,
			_ => return Err(IndexerError::UnsupportedNetworkId(val.0).into()),
		}
	} else {
		NetworkId::Undeployed
	};

	Ok(network_id)
}

#[derive(Error, Debug)]
pub enum IndexerError {
	#[error("indexer already running")]
	AlreadyRunning,
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
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
}

struct InternalState<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send> {
	txs: VecDeque<TransactionWithContext<S, P, DefaultDB>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send> InternalState<S, P> {
	pub fn new() -> Self {
		Self { txs: VecDeque::new() }
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
			.inspect_err(|e| println!("failed to send stop signal: {e}"));
		self.task_handle.await??;
		Ok(())
	}
}

#[derive(Serialize, Deserialize)]
struct SyncCache<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>> {
	genesis: <PolkadotConfig as subxt::Config>::Hash,
	/// Block hash and number
	until: Option<(<PolkadotConfig as subxt::Config>::Hash, u64)>,
	#[serde(with = "serde_def::tx_vec")]
	transactions: Vec<TransactionWithContext<S, P, DefaultDB>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>> SyncCache<S, P> {
	fn dir() -> String {
		std::env::var("MN_SYNC_CACHE").unwrap_or(".sync_cache".to_string())
	}

	fn filename(genesis: <PolkadotConfig as subxt::Config>::Hash) -> PathBuf {
		Path::new(&Self::dir()).join(format!("{}.bin", hash_to_str(genesis)))
	}

	pub fn load(
		genesis: <PolkadotConfig as subxt::Config>::Hash,
		network_id: NetworkId,
	) -> Result<Self, IndexerError> {
		let default = SyncCache { genesis, until: None, transactions: Vec::new() };

		let dir = Self::dir();
		if std::fs::create_dir_all(&dir).is_err() {
			println!("Failed to create sync cache dir {}", dir);
			return Ok(default);
		}

		let cache_filename = Self::filename(genesis);

		NETWORK_ID.with_borrow_mut(|n| *n = network_id);
		let data: Option<SyncCache<S, P>> = std::fs::File::open(&cache_filename)
			.map(|f| {
				println!("sync cache detected, loading...");
				match bincode::deserialize_from(&f) {
					Ok(cache) => Some(cache),
					Err(e) => {
						println!("error reading sync file: {:?}", e);
						println!("will attempt to delete");
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

	pub fn txs(&self) -> Vec<TransactionWithContext<S, P, DefaultDB>> {
		self.transactions.clone()
	}

	pub fn save(
		&mut self,
		network_id: NetworkId,
		hash: <PolkadotConfig as subxt::Config>::Hash,
		number: u64,
		txs: &[TransactionWithContext<S, P, DefaultDB>],
	) {
		self.until = Some((hash, number));
		self.transactions = txs.to_vec();
		self.save_to_file(network_id);
	}

	fn save_to_file(&self, network_id: NetworkId) {
		// Attempt to write to file
		NETWORK_ID.with_borrow_mut(|n| *n = network_id);
		let s = bincode::serialize(self).unwrap();
		match std::fs::write(Self::filename(self.genesis), s) {
			Ok(_) => (),
			Err(e) => println!("failed to write sync cache: {}", e),
		}
	}
}

pub struct Indexer<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + 'static> {
	pub network_id: NetworkId,
	state: Mutex<InternalState<S, P>>,
	looping: AtomicBool,
	rpc_url: String,
	fetch_concurrency: usize,
	notify_sync: Notify,
	api: OnlineClient<PolkadotConfig>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static>
	Indexer<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
{
	pub async fn new(
		network_id: NetworkId,
		rpc_url: String,
		api: OnlineClient<PolkadotConfig>,
		fetch_concurrency: usize,
	) -> Result<Self, IndexerError> {
		Ok(Indexer {
			network_id,
			state: Mutex::new(InternalState::new()),
			looping: AtomicBool::new(false),
			rpc_url,
			fetch_concurrency,
			notify_sync: Notify::new(),
			api,
		})
	}

	pub async fn get_txs(&self) -> Vec<TransactionWithContext<S, P, DefaultDB>> {
		let s = self.state.lock().await;
		s.txs.clone().into()
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
					println!("{:?}", handle.await);
					Err(IndexerError::StartFailed(e.to_string()))
				},
			}
		} else {
			Err(IndexerError::AlreadyRunning)
		}
	}

	async fn get_txs_from_block(
		self: Arc<Self>,
		block: &Block<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	) -> Result<Vec<TransactionWithContext<S, P, DefaultDB>>, IndexerError> {
		let block_header = block.header();
		let parent_block_hash = block_header.parent_hash;

		let extrinsics = block
			.extrinsics()
			.await
			.unwrap_or_else(|err| panic!("Error while fetching the transactions: {}", err));

		let calls = extrinsics
			.iter()
			.map(|extrinsic| {
				let call = extrinsic.unwrap().as_root_extrinsic::<mn_meta::Call>()?;
				Ok(call)
			})
			.filter_ok(|call| {
				matches!(call, mn_meta::Call::Midnight(_) | mn_meta::Call::Timestamp(_))
			})
			.collect::<Result<Vec<_>, subxt::Error>>()?;

		let timestamp_ms = *calls
			.iter()
			.find_map(|call| match call {
				mn_meta::Call::Timestamp(mn_meta::timestamp::Call::set { now }) => Some(now),
				_ => None,
			})
			.unwrap_or(&0); // For genesis block without Timestamp tx

		let txs_with_context = calls
			.into_iter()
			.filter_map(|call| match call {
				mn_meta::Call::Midnight(mn_meta::midnight::Call::send_mn_transaction {
					midnight_tx,
				}) => Some(midnight_tx),
				_ => None,
			})
			.map(|serialized_tx| {
				let bytes = hex::decode(&serialized_tx)
					.unwrap_or_else(|err| panic!("Error hex decoding tx: {}", err));

				let tx = deserialize(bytes.as_slice(), self.network_id)
					.unwrap_or_else(|err| panic!("Error deserializing tx: {}", err));
				TransactionWithContext {
					tx,
					block_context: BlockContext {
						tblock: Timestamp::from_secs(timestamp_ms / 1000),
						tblock_err: 30,
						parent_block_hash: HashOutput(parent_block_hash.0),
					},
				}
			})
			.collect::<Vec<TransactionWithContext<S, P, DefaultDB>>>();

		Ok(txs_with_context)
	}

	async fn fetch_midnight_block(
		self: Arc<Self>,
		hash: H256,
	) -> Result<Block<PolkadotConfig, OnlineClient<PolkadotConfig>>, subxt::Error> {
		self.api.blocks().at(hash).await
	}

	async fn fetch_until(
		self: Arc<Self>,
		tx_prog: Option<tokio::sync::mpsc::Sender<u32>>,
		start_hash: <PolkadotConfig as subxt::Config>::Hash,
		end_hash: Option<<PolkadotConfig as subxt::Config>::Hash>,
	) -> Result<VecDeque<TransactionWithContext<S, P, DefaultDB>>, IndexerError> {
		if start_hash == end_hash.unwrap_or(H256::default()) {
			return Ok(VecDeque::new());
		}

		let mut txs = VecDeque::new();

		let mut mn_block = retry(ExponentialBackoff::default(), || async {
			self.clone().fetch_midnight_block(start_hash).await.map_err(|e| {
				println!("rpc fetch failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await?;

		let mut block_header = mn_block.header();
		let mut block_number = block_header.number;

		loop {
			let txs_with_context = self.clone().get_txs_from_block(&mn_block).await?;

			for tx in txs_with_context.into_iter().rev() {
				txs.push_front(tx);
			}

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
					println!("rpc fetch failed, retrying: {e}");
					backoff::Error::transient(e)
				})
			})
			.await?;
		}

		Ok(txs)
	}

	async fn fetch_all_blocks(
		self: Arc<Self>,
		from: (<PolkadotConfig as subxt::Config>::Hash, u64),
		to: Option<(<PolkadotConfig as subxt::Config>::Hash, u64)>,
	) -> Result<VecDeque<TransactionWithContext<S, P, DefaultDB>>, IndexerError> {
		let rpc_client = RpcClient::from_insecure_url(&self.rpc_url).await?;
		let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());

		let mut prev_block_hash = Some(from.0);

		let num_blocks = from.1 as f32 - to.map(|to| to.1).unwrap_or(0) as f32;
		let block_div = num_blocks / self.fetch_concurrency as f32;

		let mut futures = vec![];

		let (tx, mut rx) = tokio::sync::mpsc::channel(100);

		for i in 0..self.fetch_concurrency {
			let mut block_hash: Option<<PolkadotConfig as subxt::Config>::Hash> =
				to.map(|to| Some(to.0)).unwrap_or(None);

			if i < self.fetch_concurrency - 1 {
				let block_number = (from.1 as f32 - (block_div * (i + 1) as f32)) as u64;
				let block = rpc
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
					(
						usize,
						Result<VecDeque<TransactionWithContext<S, P, DefaultDB>>, IndexerError>,
					),
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
				let txs = worker_res
					.map_err(|e| fatal(format!("worker failed while fetching: {e}")))
					.unwrap();
				results.insert(i, txs);
				if handles.is_empty() {
					break;
				}
			}

			let mut txs = VecDeque::new();
			// We reverse here to create a list of oldest to youngest txs
			for i in (0..total).rev() {
				let cur_txs = results.remove(&i).unwrap();
				txs.extend(cur_txs);
			}

			txs
		});

		let total = num_blocks as usize;
		let mut processed = 0;
		let mut start = std::time::Instant::now();

		for r in 0..total {
			let _b = rx.recv().await.unwrap();
			processed += 1;
			if start.elapsed() > Duration::from_secs(1) {
				println!("speed: {processed} per sec ({r}/{total})");
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
		let rpc_client = RpcClient::from_insecure_url(&self.rpc_url).await?;

		let start = std::time::Instant::now();

		// Subscribe to all finalized blocks:
		let mut blocks_sub = self.api.blocks().subscribe_finalized().await?;

		// Load sync cache
		let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());
		let genesis_hash = rpc.genesis_hash().await?;
		let mut cache = SyncCache::load(genesis_hash, self.network_id)?;

		// First, index everything from current block to genesis i.e. sync
		let latest_block = self.api.blocks().at_latest().await?;

		let num_blocks_to_fetch =
			latest_block.number() as u64 - cache.until.map(|u| u.1).unwrap_or(0);

		println!(
			"fetching {} -> {}",
			hash_to_str(latest_block.hash()),
			cache.until.map(|u| hash_to_str(u.0)).unwrap_or("genesis".to_string())
		);

		let new_txs = if num_blocks_to_fetch < 100 {
			self.clone()
				.fetch_until(None, latest_block.hash(), cache.until.map(|c| c.0))
				.await?
		} else {
			self.clone()
				.fetch_all_blocks((latest_block.hash(), latest_block.number() as u64), cache.until)
				.await?
		};

		let num_new_txs = new_txs.len();
		let num_cached_txs: usize;
		let num_txs: usize;

		{
			let mut txs = cache.txs();
			num_cached_txs = txs.len();
			txs.extend(new_txs);
			let mut s = self.state.lock().await;
			cache.save(self.network_id, latest_block.hash(), latest_block.number() as u64, &txs);
			s.txs = txs.into();
			num_txs = s.txs.len();
		}

		println!(
			"finished syncing {} transactions from {} blocks in {:.2}s. New blocks: {}, New txs: {}, Cached txs: {}",
			num_txs,
			latest_block.number(),
			start.elapsed().as_secs_f32(),
			num_blocks_to_fetch,
			num_new_txs,
			num_cached_txs
		);

		// Finished full sync
		{
			let num_blocks_synced = self.state.lock().await.txs.len();
			let _ = tx_start
				.send(num_blocks_synced)
				.inspect_err(|e| println!("Sender dropped: {e}"));
		}

		loop {
			select! {
				// Check block subscriptions
				Some(block) = blocks_sub.next() => {
					let block = block?;
					// Get midnight transactions
					let mn_block = self.clone().fetch_midnight_block(block.hash()).await?;

					let txs_with_context = self.clone().get_txs_from_block(&mn_block).await?;

					for tx in txs_with_context.into_iter().rev() {
						let mut s = self.state.lock().await;

						s.txs.push_back(tx);
					}

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

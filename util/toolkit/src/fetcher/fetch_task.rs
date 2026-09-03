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

use std::sync::{
	Arc,
	atomic::{AtomicU64, Ordering},
};

use backoff::{ExponentialBackoff, future::retry};
use futures::stream::{FuturesOrdered, StreamExt};
use hex::ToHex as _;
use subxt::{rpcs, rpcs::rpc_params, utils::H256};

use crate::{
	client::{ClientError, MidnightNodeClient},
	fetcher::{
		BLOCK_FETCH_TIMEOUT,
		compute_task::ComputeTask,
		fetch_storage::{FetchStorage, FetchedBlock},
	},
};

type FetchResult = Result<ComputeTask, FetchTaskError>;

#[derive(Default)]
pub struct FetchCounters {
	/// Blocks served from cache or fetched over RPC; drives the progress percentage.
	pub processed: AtomicU64,
	/// Blocks fetched over RPC only; drives the rate/ETA estimate - counting
	/// cache hits would make a resumed sync report an absurd rate.
	pub fetched_rpc: AtomicU64,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchTaskError {
	#[error("subxt error while fetching")]
	SubxtError(#[from] subxt::Error),
	#[error("subxt rpc error while fetching")]
	SubxtRpcError(#[from] rpcs::Error),
	#[error("node client error")]
	NodeClientError(#[from] ClientError),
	#[error("online client at block error: {0}")]
	OnlineClientAtBlockError(#[from] subxt::error::OnlineClientAtBlockError),
	#[error("extrinsic error: {0}")]
	ExtrinsicError(#[from] subxt::error::ExtrinsicError),
	#[error("block error: {0}")]
	BlockError(#[from] subxt::error::BlockError),
	#[error("storage error: {0}")]
	StorageError(#[from] subxt::error::StorageError),
	#[error("block hash missing for block number {0}")]
	BlockHashMissing(u64),
}

#[derive(Clone)]
pub enum FetchTask {
	FetchBlocks { min: u64, max: u64 },
	NoOp,
}

impl FetchTask {
	pub async fn fetch(
		self,
		chain_id: H256,
		client: &MidnightNodeClient,
		storage: impl FetchStorage,
		counters: &Arc<FetchCounters>,
	) -> FetchResult {
		match self {
			FetchTask::FetchBlocks { min, max } => {
				log::debug!("fetching blocks {min}..{max}");
				let cached_blocks = storage.get_block_data_range(chain_id, min..max).await;
				let uncached: Vec<u64> = (min..max)
					.zip(cached_blocks)
					.filter_map(|(i, b)| b.is_none().then_some(i))
					.collect();

				// The counters only drive the progress heartbeat: a retried job may
				// count some blocks twice, which the reporter clamps.
				let cached_count = (max - min) - uncached.len() as u64;
				counters.processed.fetch_add(cached_count, Ordering::Relaxed);

				let hashes = Self::fetch_block_hashes(client, &uncached).await?;

				let mut futs: FuturesOrdered<_> =
					hashes.into_iter().map(|hash| Self::fetch_block(client, hash)).collect();
				let mut blocks = Vec::new();
				while let Some(result) = futs.next().await {
					blocks.push(result?);
					counters.processed.fetch_add(1, Ordering::Relaxed);
					counters.fetched_rpc.fetch_add(1, Ordering::Relaxed);
				}
				log::debug!("fetching blocks {min}..{max}: complete");
				Ok(ComputeTask::ExtractBlockData { min, max, blocks })
			},
			FetchTask::NoOp => Ok(ComputeTask::NoOp),
		}
	}

	/// Fetch block hashes for a batch of block numbers in a single RPC call.
	pub(crate) async fn fetch_block_hashes(
		client: &MidnightNodeClient,
		block_numbers: &[u64],
	) -> Result<Vec<H256>, FetchTaskError> {
		if block_numbers.is_empty() {
			return Ok(Vec::new());
		}

		log::debug!("fetching {} block hashes in batch...", block_numbers.len());

		let backoff = ExponentialBackoff {
			max_elapsed_time: Some(BLOCK_FETCH_TIMEOUT),
			..ExponentialBackoff::default()
		};

		let hashes: Vec<H256> = retry(backoff, || async {
			client
				.rpc_client
				.request("chain_getBlockHash", rpc_params![block_numbers])
				.await
				.map_err(|e| {
					log::warn!("batch block hash fetch failed, retrying: {e}");
					backoff::Error::transient(e)
				})
		})
		.await?;

		if hashes.len() != block_numbers.len() {
			return Err(FetchTaskError::BlockHashMissing(block_numbers[hashes.len()]));
		}

		Ok(hashes)
	}

	pub(crate) async fn fetch_block(
		client: &MidnightNodeClient,
		block_hash: H256,
	) -> Result<FetchedBlock, FetchTaskError> {
		log::debug!("fetching block for hash {}...", block_hash.0.encode_hex::<String>());

		let backoff = ExponentialBackoff {
			max_elapsed_time: Some(BLOCK_FETCH_TIMEOUT),
			..ExponentialBackoff::default()
		};

		let block = retry(backoff, || async {
			client.api.at_block(block_hash).await.map_err(|e| {
				log::warn!("rpc fetch failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await?;

		let state_root = MidnightNodeClient::state_root_from(&block).await?;
		let raw_body: Vec<Vec<u8>> = block
			.extrinsics()
			.fetch()
			.await?
			.iter()
			.filter_map(|ext| ext.ok())
			.map(|ext| ext.bytes().to_vec())
			.collect();

		let header = block.block_header().await?;
		let state = if header.parent_hash.is_zero() {
			let system_properties = client.get_system_properties().await?;
			let genesis_state_value = system_properties
				.get("genesis_state")
				.expect("missing 'genesis_state' from system_properties");
			let genesis_state = hex::decode(
				genesis_state_value.as_str().expect("system_properties.genesis_state not str"),
			)
			.expect("system_properties.genesis_state not hex");

			Some(genesis_state)
		} else {
			None
		};

		// Fetched here so the compute stage never touches the network. Always
		// fetched, even for blocks that decode to a lone timestamp inherent: a
		// multi-block migration steps in `inherents_applied()` and can emit
		// `SystemTransactionApplied` there, and the block data derived from these
		// events is cached permanently behind the verified watermark.
		let key = [sp_crypto_hashing::twox_128(b"System"), sp_crypto_hashing::twox_128(b"Events")]
			.concat();
		let backoff = ExponentialBackoff {
			max_elapsed_time: Some(BLOCK_FETCH_TIMEOUT),
			..ExponentialBackoff::default()
		};
		let events = retry(backoff, || async {
			match block.storage().fetch_raw(key.clone()).await {
				Ok(bytes) => Ok(bytes),
				Err(subxt::error::StorageError::NoValueFound) => Ok(Vec::new()),
				Err(e) => {
					log::warn!("events fetch failed, retrying: {e}");
					Err(backoff::Error::transient(e))
				},
			}
		})
		.await?;

		Ok(FetchedBlock { block, header, raw_body, state_root, state, events })
	}
}

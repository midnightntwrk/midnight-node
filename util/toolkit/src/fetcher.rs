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

pub mod compute_task;
pub mod fetch_storage;
pub mod fetch_task;
pub mod runtimes;
pub mod trusted_deserialize;
pub mod wallet_state_cache;

use std::time::Duration;

use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;
use subxt::{OnlineClient, blocks::Block, ext::subxt_rpcs, utils::H256};
use tokio::task::JoinSet;

use crate::{
	client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig},
	fetcher::{
		compute_task::{ComputeError, ComputeTask},
		fetch_storage::FetchStorage,
		fetch_task::{FetchTask, FetchTaskError},
	},
};

pub type MidnightBlock = Block<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>;

/// Number of blocks to process per batch. Tuned for memory/parallelism tradeoff.
const BLOCKS_PER_JOB: u64 = 100;

/// Maximum time to wait for a block fetch before giving up.
pub const BLOCK_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	#[error("subxt error while fetching")]
	SubxtError(#[from] subxt::Error),
	#[error("subxt rpc error while fetching")]
	SubxtRpcError(#[from] subxt_rpcs::Error),
	#[error("error creating client")]
	NodeClientError(#[from] ClientError),
	#[error("block hash missing for block number {0}")]
	BlockHashMissing(u64),
	#[error("block missing {0}")]
	BlockMissing(u64),
	#[error("fetch task error")]
	FetchTaskError(#[from] FetchTaskError),
	#[error("compute task error")]
	ComputeTaskError(#[from] ComputeError),
	#[error("worker thread panicked")]
	WorkerPanic(String),
	#[error("no fetch workers could connect to the node")]
	NoWorkersConnected,
}

/// Identifies the type of task that completed in the join set.
enum TaskResult {
	JobPusher,
	FetchWorker,
	ComputeWorker,
}

/// Fetch a single block by number. Checks cache first, falls back to node RPC.
/// On cache miss, fetches from the node and stores the result in cache.
pub async fn fetch_single_block(
	chain_id: H256,
	block_number: u64,
	client: Option<&MidnightNodeClient>,
	storage: &(impl FetchStorage + Clone + 'static),
) -> Result<RawBlockData, FetchError> {
	if let Some(block) = storage.get_block_data(chain_id, block_number).await {
		return Ok(block);
	}
	let client = client.ok_or(FetchError::BlockMissing(block_number))?;
	let block_hash = FetchTask::fetch_block_hash(client, block_number).await?;
	let fetched = FetchTask::fetch_block(client, block_hash).await?;
	let raw = ComputeTask::extract_data(&fetched).await?;
	storage.insert_block_data(chain_id, block_number, raw.clone()).await;
	Ok(raw)
}

pub async fn read_blocks_from_cache(
	chain_id: H256,
	fetch_storage: impl FetchStorage + Clone + 'static,
) -> Result<Vec<RawBlockData>, FetchError> {
	let t = std::time::Instant::now();
	let max_height = fetch_storage.get_highest_verified_block(chain_id).await.unwrap_or(0);
	log::debug!("[perf] get_highest_verified_block took {:?}", t.elapsed());

	let t = std::time::Instant::now();
	let mut blocks: Vec<_> = fetch_storage
		.get_block_data_range(chain_id, (0..max_height + 1).into_iter())
		.await
		.into_iter()
		.enumerate()
		.map(|(i, b)| b.unwrap_or_else(|| panic!("missing block {i}")))
		.collect();
	log::debug!("[perf] get_block_data_range: {} blocks in {:?}", blocks.len(), t.elapsed());

	// Set last_block_time for all blocks
	// windows_mut() iterator does not exist - so we're indexing here
	let t = std::time::Instant::now();
	for i in 1..blocks.len() {
		blocks[i].last_block_time_secs = blocks[i - 1].tblock_secs;
	}
	log::debug!("[perf] last_block_time fixup: {} blocks in {:?}", blocks.len(), t.elapsed());

	Ok(blocks)
}

pub async fn fetch_all(
	url: &str,
	num_workers: usize,
	num_compute_workers: usize,
	fetch_only_cache: bool,
	fetch_storage: impl FetchStorage + Clone + 'static,
) -> Result<Vec<RawBlockData>, FetchError> {
	let client = MidnightNodeClient::new(&url, None).await?;
	let chain_id = client.get_block_one_hash().await.map_err(|e| Into::<FetchError>::into(e))?;
	if fetch_only_cache {
		let blocks = read_blocks_from_cache(chain_id, fetch_storage).await?;

		log::info!(
			"read {} blocks from cache, total transations: {}",
			blocks.len(),
			blocks.iter().fold(0, |acc, b| acc + b.transactions.len()),
		);

		Ok(blocks)
	} else {
		fetch_from_rpc(url, chain_id, num_workers, num_compute_workers, fetch_storage).await
	}
}

pub async fn fetch_from_rpc(
	url: &str,
	chain_id: H256,
	num_workers: usize,
	num_compute_workers: usize,
	fetch_storage: impl FetchStorage + Clone + 'static,
) -> Result<Vec<RawBlockData>, FetchError> {
	if std::env::var("MN_SYNC_CACHE").is_ok() {
		panic!(
			"Error: 'MN_SYNC_CACHE' is defined - please use 'MN_FETCH_CACHE' instead. See `--help` for more info."
		);
	}

	let t_rpc_total = std::time::Instant::now();
	let client = MidnightNodeClient::new(&url, None).await?;
	let finalized_height =
		client.get_finalized_height().await.map_err(|e| Into::<FetchError>::into(e))?;
	let max_height = finalized_height + 1;
	let min_height = fetch_storage.get_highest_verified_block(chain_id).await.unwrap_or(0);

	let blocks_per_job = if (max_height - min_height) < BLOCKS_PER_JOB * num_workers as u64 {
		(max_height - min_height).div_ceil(num_workers as u64).max(5)
	} else {
		BLOCKS_PER_JOB
	};

	let mut join_set: JoinSet<Result<TaskResult, FetchError>> = JoinSet::new();

	let (fetch_job_tx, fetch_job_rx) = async_channel::bounded(num_workers * 2);
	let (fetch_to_compute_tx, fetch_to_compute_rx) =
		async_channel::bounded(num_compute_workers * 2);
	// We use a separate unbounded channel here because compute workers produce recursive tasks
	let (compute_to_compute_tx, compute_to_compute_rx) = async_channel::unbounded();
	let (final_jobs_tx, final_jobs_rx) = async_channel::bounded(num_compute_workers * 2);

	// Push jobs into queue
	{
		let job_tx = fetch_job_tx.clone();
		let max_height = max_height;
		join_set.spawn(async move {
			for min in (min_height..max_height).step_by(blocks_per_job as usize) {
				let max = u64::min(min + blocks_per_job, max_height);
				log::debug!("pushing new fetch job {min} -> {max}...");
				job_tx
					.send(FetchTask::FetchBlocks { min, max })
					.await
					.expect("failed to push job on channel");
			}

			Ok(TaskResult::JobPusher)
		});
	}

	log::info!("spawning {num_workers} fetch workers");

	// Spawn fetch workers
	for worker_id in 0..num_workers {
		let job_rx = fetch_job_rx.clone();
		let work_job_tx = fetch_to_compute_tx.clone();
		let fetch_storage = fetch_storage.clone();
		let url = url.to_string();
		join_set.spawn(async move {
			let Ok(client) = MidnightNodeClient::new(&url, None).await else {
				log::warn!(
					"fetch worker {worker_id} could not connect to {url}, exiting. \
					 This may be due to connection limits on the remote node."
				);
				return Ok(TaskResult::FetchWorker);
			};

			log::debug!("fetch worker {worker_id} connected successfully");

			loop {
				let Ok(job) = job_rx.recv().await else {
					return Ok(TaskResult::FetchWorker);
				};

				log::debug!("worker {worker_id}: received new job...");

				let work_job = job.fetch(chain_id, &client, fetch_storage.clone()).await?;

				work_job_tx.send(work_job).await.expect("failed to push job on work queue");
				log::debug!("worker {worker_id}: completed job.");
			}
		});
	}

	log::info!("spawning {num_compute_workers} compute workers");

	// Spawn compute workers
	for _ in 0..num_compute_workers {
		let fetch_to_compute_rx = fetch_to_compute_rx.clone();
		let compute_to_compute_rx = compute_to_compute_rx.clone();
		let compute_to_compute_tx = compute_to_compute_tx.clone();
		let final_jobs_tx = final_jobs_tx.clone();
		let fetch_storage = fetch_storage.clone();
		join_set.spawn(async move {
			loop {
				// Receive from both channels - prioritize new work from fetch workers
				let job = tokio::select! {
					biased;

					job = fetch_to_compute_rx.recv() => {
						match job {
							Ok(job) => job,
							Err(_) => return Ok(TaskResult::ComputeWorker),
						}
					},
					job = compute_to_compute_rx.recv() => {
						match job {
							Ok(job) => job,
							Err(_) => return Ok(TaskResult::ComputeWorker),
						}
					},
				};

				log::debug!("received new work job...");

				let work_job = job.work(chain_id, fetch_storage.clone()).await?;

				match &work_job {
					ComputeTask::FinalVerify { .. } => {
						final_jobs_tx.send(work_job).await.expect("failed to push final job");
					},
					ComputeTask::NoOp => continue,
					_ => compute_to_compute_tx
						.send(work_job)
						.await
						.expect("failed to push job on work queue"),
				};
			}
		});
	}

	log::debug!("receive blocks");

	log::debug!("final verify step");
	// Receive final jobs
	let num_jobs = (max_height - min_height).div_ceil(blocks_per_job);
	let mut jobs = Vec::with_capacity(num_jobs as usize);
	let mut received = 0;
	let mut fetch_workers_exited = 0;
	while received < num_jobs {
		tokio::select! {
			Some(result) = join_set.join_next() => {
				match result {
					Ok(Ok(TaskResult::FetchWorker)) => {
						fetch_workers_exited += 1;
						if fetch_workers_exited == num_workers {
							log::error!("all fetch workers exited before completing all jobs ({received}/{num_jobs} received)");
							join_set.abort_all();
							return Err(FetchError::NoWorkersConnected);
						}
					},
					Ok(Ok(_)) => {}, // JobPusher or ComputeWorker exited normally
					Ok(Err(e)) => {
						join_set.abort_all();
						return Err(e);
					}
					Err(join_err) if join_err.is_panic() => {
						join_set.abort_all();
						return Err(FetchError::WorkerPanic(join_err.to_string()));
					}
					// Task was cancelled (expected after abort_all())
					Err(_) => {}
				}
			},
			job = final_jobs_rx.recv() => {
				jobs.push(job.expect("..."));
				received += 1;
				log::info!("fetch progress: {:.1}% of {} blocks complete", (received as f64 / num_jobs as f64) * 100f64, max_height - min_height);
			}
		}
	}

	log::debug!("finished loop");

	for job in jobs {
		job.work(chain_id, fetch_storage.clone()).await?;
	}
	log::info!("all blocks verified");

	// Close channels to exit workers
	fetch_job_rx.close();
	fetch_to_compute_rx.close();
	compute_to_compute_rx.close();
	final_jobs_rx.close();

	// Wait for all workers to fully exit so their Arc<Database> handles are dropped.
	// Without this, the JoinSet drop aborts tasks but doesn't synchronously release
	// resources, causing "DatabaseAlreadyOpen" when the DB is reopened.
	while let Some(result) = join_set.join_next().await {
		if let Err(join_err) = result {
			if join_err.is_panic() {
				log::warn!("Worker task panicked during cleanup: {}", join_err);
			}
		}
	}

	log::debug!("[perf] fetch_from_rpc RPC pipeline took {:?}", t_rpc_total.elapsed());

	// Set highest verified height for quicker fetch next time
	fetch_storage.set_highest_verified_block(chain_id, finalized_height).await;
	let t = std::time::Instant::now();
	let blocks = read_blocks_from_cache(chain_id, fetch_storage).await?;
	log::debug!("[perf] fetch_from_rpc read_blocks_from_cache took {:?}", t.elapsed());

	log::info!(
		"fetched {} blocks, read {} blocks from cache, total transations: {}",
		finalized_height - min_height,
		blocks.len() - (finalized_height - min_height) as usize,
		blocks.iter().fold(0, |acc, b| acc + b.transactions.len()),
	);

	Ok(blocks)
}

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

use std::{
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
	time::Duration,
};

use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;
use subxt::{client::OnlineClientAtBlock, rpcs, utils::H256};
use tokio::task::JoinSet;

use crate::{
	client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig},
	fetcher::{
		compute_task::{ComputeError, ComputeTask},
		fetch_storage::FetchStorage,
		fetch_task::{FetchCounters, FetchTask, FetchTaskError},
	},
};

pub type MidnightClientAtBlock = OnlineClientAtBlock<MidnightNodeClientConfig>;

/// Number of blocks to process per batch. Tuned for memory/parallelism tradeoff.
const BLOCKS_PER_JOB: u64 = 100;

/// Maximum time to wait for a block fetch before giving up.
pub const BLOCK_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Job retries per fetch worker before failing the whole fetch. A dropped
/// WebSocket permanently poisons the jsonrpsee client ("restart required"),
/// so each retry reconnects first.
const MAX_JOB_ATTEMPTS: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	#[error("subxt error while fetching")]
	SubxtError(#[from] subxt::Error),
	#[error("subxt rpc error while fetching")]
	SubxtRpcError(#[from] rpcs::Error),
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
	ProgressReporter,
}

const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(10);

/// EWMA smoothing factor for the fetch rate: each report contributes 30%,
/// giving a ~20s half-life - steady ETA despite batchy per-tick rates, yet
/// converging within a few ticks after an outage or cache burst.
const RATE_SMOOTHING_ALPHA: f64 = 0.3;

/// Below this rate (blocks/s) the ETA projection is meaningless (a stall would
/// print millennia-scale ETAs), so report "unknown" instead.
const ETA_MIN_RATE: f64 = 0.1;

fn format_eta(secs: u64) -> String {
	if secs >= 3600 {
		format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
	} else if secs >= 60 {
		format!("{}m{:02}s", secs / 60, secs % 60)
	} else {
		format!("{secs}s")
	}
}

/// Fetch a single block by hash. Checks cache first, falls back to node RPC.
/// On cache miss, fetches from the node and stores the result in cache.
pub async fn fetch_single_block(
	chain_id: H256,
	block_number: u64,
	block_hash: H256,
	client: Option<&MidnightNodeClient>,
	storage: &(impl FetchStorage + Clone + 'static),
) -> Result<RawBlockData, FetchError> {
	if let Some(block) = storage.get_block_data(chain_id, block_number).await {
		return Ok(block);
	}
	let client = client.ok_or(FetchError::BlockMissing(block_number))?;
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

	log::info!("loading {} blocks from local cache (this can take a while)...", max_height + 1);
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
			"read {} blocks from cache, total transactions: {}",
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
	log::info!("connecting to {url}...");
	let client = MidnightNodeClient::new(&url, None).await?;
	let finalized_height =
		client.get_finalized_height().await.map_err(|e| Into::<FetchError>::into(e))?;
	let max_height = finalized_height + 1;
	let min_height = fetch_storage.get_highest_verified_block(chain_id).await.unwrap_or(0);
	log::info!(
		"chain head (finalized): {finalized_height}, verified in cache: {min_height}, fetching {} blocks",
		max_height.saturating_sub(min_height)
	);

	// The cache watermark can be ahead of this node's finalized height (e.g. the
	// node lags behind the node the cache was built from); saturate to zero so we
	// serve from cache instead of underflowing.
	let fetch_span = max_height.saturating_sub(min_height);
	let blocks_per_job = if fetch_span < BLOCKS_PER_JOB * num_workers as u64 {
		fetch_span.div_ceil(num_workers as u64).max(5)
	} else {
		BLOCKS_PER_JOB
	};

	// Cap workers to the number of jobs to avoid unnecessary connections.
	let num_jobs = fetch_span.div_ceil(blocks_per_job);
	let num_workers = num_workers.min(num_jobs as usize).max(1);

	// Shared atomics: the tip-chasing pusher grows them while the progress
	// reporter and receive loop read them.
	let total_span = Arc::new(AtomicU64::new(fetch_span));
	let total_jobs = Arc::new(AtomicU64::new(num_jobs));
	let target_height = Arc::new(AtomicU64::new(finalized_height));

	let mut join_set: JoinSet<Result<TaskResult, FetchError>> = JoinSet::new();

	let (fetch_job_tx, fetch_job_rx) = async_channel::bounded(num_workers * 2);
	let (fetch_to_compute_tx, fetch_to_compute_rx) =
		async_channel::bounded(num_compute_workers * 2);
	// We use a separate unbounded channel here because compute workers produce recursive tasks
	let (compute_to_compute_tx, compute_to_compute_rx) = async_channel::unbounded();
	let (final_jobs_tx, final_jobs_rx) = async_channel::bounded(num_compute_workers * 2);

	// Push jobs into queue, then keep chasing the finalized tip until caught up
	{
		let job_tx = fetch_job_tx.clone();
		let url = url.to_string();
		let total_span = total_span.clone();
		let total_jobs = total_jobs.clone();
		let target_height = target_height.clone();
		join_set.spawn(async move {
			let mut next_min = min_height;
			let mut planned_to = max_height.max(min_height);
			loop {
				for min in (next_min..planned_to).step_by(blocks_per_job as usize) {
					let max = u64::min(min + blocks_per_job, planned_to);
					log::debug!("pushing new fetch job {min} -> {max}...");
					job_tx
						.send(FetchTask::FetchBlocks { min, max })
						.await
						.expect("failed to push job on channel");
				}
				next_min = planned_to;

				// Fresh client: a multi-hour sync outlives idle sockets.
				let new_finalized = match MidnightNodeClient::new(&url, None).await {
					Ok(client) => match client.get_finalized_height().await {
						Ok(h) => h,
						Err(e) => {
							log::warn!(
								"could not re-check chain tip ({e}); finishing sync at block {}",
								planned_to.saturating_sub(1)
							);
							break;
						},
					},
					Err(e) => {
						log::warn!(
							"could not reconnect to re-check chain tip ({e}); finishing sync at block {}",
							planned_to.saturating_sub(1)
						);
						break;
					},
				};

				let new_max = new_finalized + 1;
				let added_blocks = new_max.saturating_sub(planned_to);
				if added_blocks == 0 {
					log::info!("caught up with the tip");
					break;
				}
				// Publish new totals before queueing so the receive loop can't
				// observe completion of the old total first.
				total_span.fetch_add(added_blocks, Ordering::Relaxed);
				total_jobs.fetch_add(added_blocks.div_ceil(blocks_per_job), Ordering::Relaxed);
				target_height.store(new_finalized, Ordering::Relaxed);
				log::info!(
					"chain advanced to {new_finalized} during sync, fetching {added_blocks} more blocks..."
				);
				planned_to = new_max;
			}

			Ok(TaskResult::JobPusher)
		});
	}

	log::info!(
		"spawning {num_workers} fetch workers (capped from requested, {num_jobs} jobs); \
		 each worker connects and downloads runtime metadata first, which can take a while..."
	);

	let counters = Arc::new(FetchCounters::default());
	let jobs_verified = Arc::new(AtomicU64::new(0));
	let pipeline_done = Arc::new(AtomicBool::new(false));

	// Spawn fetch workers
	for worker_id in 0..num_workers {
		let job_rx = fetch_job_rx.clone();
		let work_job_tx = fetch_to_compute_tx.clone();
		let fetch_storage = fetch_storage.clone();
		let url = url.to_string();
		let counters = counters.clone();
		join_set.spawn(async move {
			let Ok(mut client) = MidnightNodeClient::new(&url, None).await else {
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

				let mut attempts = 0;
				let work_job = loop {
					attempts += 1;
					match job
						.clone()
						.fetch(chain_id, &client, fetch_storage.clone(), &counters)
						.await
					{
						Ok(work_job) => break work_job,
						Err(e) if attempts < MAX_JOB_ATTEMPTS => {
							let backoff_secs = (1u64 << attempts.min(6)).min(60);
							log::warn!(
								"worker {worker_id}: fetch job failed (attempt {attempts}/{MAX_JOB_ATTEMPTS}): {e:?}; reconnecting in {backoff_secs}s..."
							);
							tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
							match MidnightNodeClient::new(&url, None).await {
								Ok(new_client) => client = new_client,
								Err(ce) => log::warn!(
									"worker {worker_id}: reconnect to {url} failed: {ce}; will retry"
								),
							}
						},
						Err(e) => return Err(e.into()),
					}
				};

				work_job_tx.send(work_job).await.expect("failed to push job on work queue");
				log::debug!("worker {worker_id}: completed job.");
			}
		});
	}

	// Progress reporter, independent of pipeline stages so it can't be starved.
	{
		let counters = counters.clone();
		let jobs_verified = jobs_verified.clone();
		let pipeline_done = pipeline_done.clone();
		let extract_backlog = fetch_to_compute_rx.clone();
		let verify_backlog = compute_to_compute_rx.clone();
		let total_span = total_span.clone();
		let total_jobs = total_jobs.clone();
		join_set.spawn(async move {
			let mut last_fetched = 0u64;
			let mut last_tick = std::time::Instant::now();
			let mut ewma_rate: Option<f64> = None;
			loop {
				tokio::time::sleep(Duration::from_secs(1)).await;
				if pipeline_done.load(Ordering::Relaxed) {
					return Ok(TaskResult::ProgressReporter);
				}
				if last_tick.elapsed() < PROGRESS_REPORT_INTERVAL {
					continue;
				}
				let processed = counters.processed.load(Ordering::Relaxed);
				let fetched = counters.fetched_rpc.load(Ordering::Relaxed);
				let span = total_span.load(Ordering::Relaxed);
				let jobs = total_jobs.load(Ordering::Relaxed);
				let rate = (fetched - last_fetched) as f64 / last_tick.elapsed().as_secs_f64();
				last_fetched = fetched;
				last_tick = std::time::Instant::now();

				let smoothed = match ewma_rate {
					Some(prev) => prev * (1.0 - RATE_SMOOTHING_ALPHA) + rate * RATE_SMOOTHING_ALPHA,
					None => rate,
				};
				ewma_rate = Some(smoothed);

				let eta = if smoothed >= ETA_MIN_RATE {
					format_eta((span.saturating_sub(processed) as f64 / smoothed) as u64)
				} else {
					"unknown (stalled?)".to_string()
				};
				log::info!(
					"fetch progress: {processed}/{span} blocks ({:.1}%), {rate:.0} blocks/s, ETA {eta}, verified {}/{jobs} jobs, backlog: {} extract / {} verify jobs",
					processed as f64 / span as f64 * 100.0,
					jobs_verified.load(Ordering::Relaxed),
					extract_backlog.len(),
					verify_backlog.len(),
				);
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
				// No `biased`: preferring fresh fetch results starves the
				// recursive Verify tasks whenever fetch outpaces compute.
				let job = tokio::select! {
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
	// The job total grows while the pusher chases the tip.
	let mut jobs = Vec::with_capacity(total_jobs.load(Ordering::Relaxed) as usize);
	let mut received: u64 = 0;
	let mut fetch_workers_exited = 0;
	let mut pusher_done = false;
	while !(pusher_done && received >= total_jobs.load(Ordering::Relaxed)) {
		tokio::select! {
			Some(result) = join_set.join_next() => {
				match result {
					Ok(Ok(TaskResult::JobPusher)) => {
						pusher_done = true;
					},
					Ok(Ok(TaskResult::FetchWorker)) => {
						fetch_workers_exited += 1;
						if fetch_workers_exited == num_workers {
							log::error!("all fetch workers exited before completing all jobs ({received}/{} received)", total_jobs.load(Ordering::Relaxed));
							join_set.abort_all();
							return Err(FetchError::NoWorkersConnected);
						}
					},
					Ok(Ok(_)) => {}, // ComputeWorker or ProgressReporter exited normally
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
				jobs_verified.store(received, Ordering::Relaxed);
				log::debug!("verify progress: {received}/{} jobs", total_jobs.load(Ordering::Relaxed));
			}
		}
	}

	log::debug!("finished loop");
	pipeline_done.store(true, Ordering::Relaxed);

	log::info!("all blocks fetched, running final boundary verification...");
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

	// Set highest verified height for quicker fetch next time. Use the chased
	// target (the tip may have advanced during sync). Never lower it: when the
	// queried node lags the cache, blocks beyond its finalized height are
	// already verified, and read_blocks_from_cache bases its read range on
	// this value — lowering it would hide them.
	let final_height = target_height.load(Ordering::Relaxed);
	if final_height > min_height {
		fetch_storage.set_highest_verified_block(chain_id, final_height).await;
	}
	let t = std::time::Instant::now();
	let blocks = read_blocks_from_cache(chain_id, fetch_storage).await?;
	log::debug!("[perf] fetch_from_rpc read_blocks_from_cache took {:?}", t.elapsed());

	let final_span = total_span.load(Ordering::Relaxed);
	log::info!(
		"fetched {} blocks, read {} blocks from cache, total transactions: {}",
		final_span,
		blocks.len().saturating_sub(final_span as usize),
		blocks.iter().fold(0, |acc, b| acc + b.transactions.len()),
	);

	Ok(blocks)
}

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
pub mod ledger_arena;
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

use backoff::{ExponentialBackoff, future::retry_notify};
use midnight_node_ledger_helpers::fork::raw_block_data::{LedgerVersion, RawBlockData};
use subxt::{client::OnlineClientAtBlock, rpcs, utils::H256};
use tokio::task::JoinSet;

use crate::{
	client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig},
	fetcher::{
		compute_task::{ComputeError, ComputeTask},
		fetch_storage::{BlockReader, FetchStorage},
		fetch_task::{FetchCounters, FetchTask, FetchTaskError},
	},
};

pub type MidnightClientAtBlock = OnlineClientAtBlock<MidnightNodeClientConfig>;

/// Number of blocks to process per batch. Tuned for memory/parallelism tradeoff.
const BLOCKS_PER_JOB: u64 = 100;

/// Blocks read from the fetch cache per query when loading a window. Bounds the
/// transient memory of a read (rows plus decoded blocks) to one chunk.
pub const CACHE_READ_CHUNK: u64 = 10_000;

/// Maximum time to wait for a block fetch before giving up.
pub const BLOCK_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// A dropped WebSocket permanently poisons the jsonrpsee client ("restart
/// required"), so every job retry reconnects first.
const JOB_RETRY_MAX_ELAPSED: Duration = Duration::from_secs(10 * 60);
const JOB_RETRY_MAX_INTERVAL: Duration = Duration::from_secs(60);

const WORKER_CONNECT_MAX_ELAPSED: Duration = Duration::from_secs(30);

/// Tip re-checks after the initial span; each one only picks up blocks finalized
/// while the previous round was processed, so a few rounds reach the head.
const MAX_TIP_CHASE_ROUNDS: usize = 20;

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
}

const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(10);

/// Grown by the tip-chasing job pusher, read by the reporter and the receive loop.
struct SyncTotals {
	span: AtomicU64,
	jobs: AtomicU64,
	target_height: AtomicU64,
	truncated: AtomicBool,
}

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

/// Read blocks `from..=to` from the fetch cache in [`CACHE_READ_CHUNK`] pieces, with each
/// block's `last_block_time_secs` set from its predecessor (block `from - 1` is read for
/// that alone when `from > 0`). Returns an empty list when `from > to`.
pub async fn read_block_window(
	chain_id: H256,
	reader: &dyn BlockReader,
	from: u64,
	to: u64,
) -> Result<Vec<RawBlockData>, FetchError> {
	if from > to {
		return Ok(Vec::new());
	}
	let first = from.saturating_sub(1);
	let total = to - first + 1;
	log::debug!("loading blocks {from}..={to} from the fetch cache...");
	let t = std::time::Instant::now();
	let mut blocks: Vec<RawBlockData> = Vec::with_capacity(total as usize);
	let mut chunk_start = first;
	while chunk_start <= to {
		// One more than a window, so a window plus its predecessor block is one read.
		let chunk_end = chunk_start.saturating_add(CACHE_READ_CHUNK + 1).min(to + 1);
		let chunk = reader.read_block_range(chain_id, chunk_start, chunk_end).await;
		for (i, block) in chunk.into_iter().enumerate() {
			blocks.push(block.ok_or(FetchError::BlockMissing(chunk_start + i as u64))?);
		}
		chunk_start = chunk_end;
	}
	log::debug!("[perf] read_block_window: {} blocks in {:?}", blocks.len(), t.elapsed());

	// Set last_block_time for all blocks
	// windows_mut() iterator does not exist - so we're indexing here
	for i in 1..blocks.len() {
		blocks[i].last_block_time_secs = blocks[i - 1].tblock_secs;
	}
	if from > 0 && !blocks.is_empty() {
		blocks.remove(0);
	}
	Ok(blocks)
}

/// A synced chain whose blocks stay in the fetch cache until a replay asks for the
/// window it needs, instead of being materialised up front.
#[derive(Clone)]
pub struct CachedBlocks {
	pub chain_id: H256,
	/// Highest block held by the cache (inclusive).
	pub tip_height: u64,
	/// Ledger version of the genesis block.
	pub first_version: LedgerVersion,
	/// Ledger version of the tip block.
	pub tip_version: LedgerVersion,
	reader: Arc<dyn BlockReader>,
}

impl std::fmt::Debug for CachedBlocks {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("CachedBlocks")
			.field("chain_id", &self.chain_id)
			.field("tip_height", &self.tip_height)
			.field("first_version", &self.first_version)
			.field("tip_version", &self.tip_version)
			.finish_non_exhaustive()
	}
}

impl CachedBlocks {
	/// Describe the cached chain `chain_id` up to `tip_height`, reading only the genesis
	/// and tip blocks for their ledger versions.
	pub async fn open(
		chain_id: H256,
		reader: Arc<dyn BlockReader>,
		tip_height: u64,
	) -> Result<Self, FetchError> {
		let first_version = reader
			.read_block(chain_id, 0)
			.await
			.ok_or(FetchError::BlockMissing(0))?
			.ledger_version();
		let tip_version = if tip_height == 0 {
			first_version
		} else {
			reader
				.read_block(chain_id, tip_height)
				.await
				.ok_or(FetchError::BlockMissing(tip_height))?
				.ledger_version()
		};
		Ok(Self { chain_id, tip_height, first_version, tip_version, reader })
	}

	/// Ledger generation at `height`: the block's own, or the tip's for a height beyond the
	/// cache (a wallet cached from a node that was further ahead). `None` if the block is
	/// missing from the cache.
	pub async fn ledger_version_at(&self, height: u64) -> Option<LedgerVersion> {
		if height > self.tip_height {
			return Some(self.tip_version);
		}
		self.reader.read_block(self.chain_id, height).await.map(|b| b.ledger_version())
	}

	/// Load blocks `from..=to`; see [`read_block_window`].
	pub async fn load_range(&self, from: u64, to: u64) -> Result<Vec<RawBlockData>, FetchError> {
		read_block_window(self.chain_id, self.reader.as_ref(), from, to).await
	}

	/// Load blocks `from..=tip`; see [`read_block_window`].
	pub async fn load_from(&self, from: u64) -> Result<Vec<RawBlockData>, FetchError> {
		self.load_range(from, self.tip_height).await
	}

	/// Every cached block, genesis to tip.
	pub async fn load_all(&self) -> Result<Vec<RawBlockData>, FetchError> {
		self.load_from(0).await
	}
}

pub async fn fetch_all(
	url: &str,
	num_workers: usize,
	num_compute_workers: usize,
	fetch_only_cache: bool,
	fetch_storage: impl FetchStorage + Clone + 'static,
) -> Result<CachedBlocks, FetchError> {
	let client = MidnightNodeClient::new(&url, None).await?;
	let chain_id = client.get_block_one_hash().await.map_err(|e| Into::<FetchError>::into(e))?;
	if !fetch_only_cache {
		fetch_from_rpc(url, chain_id, num_workers, num_compute_workers, fetch_storage.clone())
			.await?;
	}
	let tip_height = fetch_storage.get_highest_verified_block(chain_id).await.unwrap_or(0);
	let cached = CachedBlocks::open(chain_id, Arc::new(fetch_storage), tip_height).await?;
	log::info!("fetch cache holds blocks 0..={tip_height} ({:?} at the tip)", cached.tip_version);
	Ok(cached)
}

/// Sync the fetch cache with the node's finalized chain, returning the synced height.
pub async fn fetch_from_rpc(
	url: &str,
	chain_id: H256,
	num_workers: usize,
	num_compute_workers: usize,
	fetch_storage: impl FetchStorage + Clone + 'static,
) -> Result<u64, FetchError> {
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

	let num_compute_workers = num_compute_workers.max(1);

	let totals = Arc::new(SyncTotals {
		span: AtomicU64::new(fetch_span),
		jobs: AtomicU64::new(num_jobs),
		target_height: AtomicU64::new(finalized_height),
		truncated: AtomicBool::new(false),
	});

	let mut join_set: JoinSet<Result<TaskResult, FetchError>> = JoinSet::new();
	let counters = Arc::new(FetchCounters::default());
	let jobs_verified = Arc::new(AtomicU64::new(0));

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
		let totals = totals.clone();
		let jobs_verified = jobs_verified.clone();
		let mut client = client;
		join_set.spawn(async move {
			let mut next_min = min_height;
			let mut planned_to = max_height.max(min_height);
			let mut rounds = 0;
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

				// Re-check only once the planned work is processed, so a round picks
				// up real lag rather than the time the re-check itself took.
				while jobs_verified.load(Ordering::Relaxed) < totals.jobs.load(Ordering::Relaxed) {
					tokio::time::sleep(Duration::from_millis(500)).await;
				}
				rounds += 1;
				if rounds > MAX_TIP_CHASE_ROUNDS {
					log::warn!(
						"chain keeps advancing faster than it is fetched; finishing sync at block {}",
						planned_to.saturating_sub(1)
					);
					totals.truncated.store(true, Ordering::Relaxed);
					break;
				}
				let new_finalized = match fetch_finalized_height(&mut client, &url).await {
					Ok(height) => height,
					Err(e) => {
						log::warn!(
							"could not re-check the chain tip ({e}); finishing sync at block {}",
							planned_to.saturating_sub(1)
						);
						totals.truncated.store(true, Ordering::Relaxed);
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
				totals.span.fetch_add(added_blocks, Ordering::Relaxed);
				totals.jobs.fetch_add(added_blocks.div_ceil(blocks_per_job), Ordering::Relaxed);
				totals.target_height.store(new_finalized, Ordering::Relaxed);
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

	// Spawn fetch workers
	for worker_id in 0..num_workers {
		let fetch_job_rx = fetch_job_rx.clone();
		let work_job_tx = fetch_to_compute_tx.clone();
		let fetch_storage = fetch_storage.clone();
		let url = url.to_string();
		let counters = counters.clone();
		join_set.spawn(async move {
			let Ok(client) = connect_with_retry(&url, worker_id).await else {
				return Ok(TaskResult::FetchWorker);
			};
			// `None` after a failed job: the client is poisoned, reconnect first.
			let client = tokio::sync::Mutex::new(Some(client));

			while let Ok(job) = fetch_job_rx.recv().await {
				log::debug!("worker {worker_id}: received new job...");

				let backoff = ExponentialBackoff {
					max_elapsed_time: Some(JOB_RETRY_MAX_ELAPSED),
					max_interval: JOB_RETRY_MAX_INTERVAL,
					..ExponentialBackoff::default()
				};
				let work_job = retry_notify(
					backoff,
					|| async {
						let mut guard = client.lock().await;
						if guard.is_none() {
							let reconnected = MidnightNodeClient::new(&url, None)
								.await
								.map_err(|e| backoff::Error::transient(FetchError::from(e)))?;
							*guard = Some(reconnected);
						}
						let result = job
							.clone()
							.fetch(
								chain_id,
								guard.as_ref().expect("connected above"),
								fetch_storage.clone(),
								&counters,
							)
							.await;
						result.map_err(|e| {
							*guard = None;
							backoff::Error::transient(FetchError::from(e))
						})
					},
					|e: FetchError, wait: Duration| {
						log::warn!(
							"worker {worker_id}: fetch job failed ({e}); reconnecting and retrying in {:.0}s...",
							wait.as_secs_f32()
						);
					},
				)
				.await?;

				work_job_tx.send(work_job).await.expect("failed to push job on work queue");
				log::debug!("worker {worker_id}: completed job.");
			}
			Ok(TaskResult::FetchWorker)
		});
	}

	// Aborted once all jobs are in; the drain loop below tolerates the cancellation.
	let reporter = {
		let counters = counters.clone();
		let jobs_verified = jobs_verified.clone();
		let extract_backlog = fetch_to_compute_rx.clone();
		let verify_backlog = compute_to_compute_rx.clone();
		let totals = totals.clone();
		join_set.spawn(async move {
			let started = std::time::Instant::now();
			let mut interval = tokio::time::interval(PROGRESS_REPORT_INTERVAL);
			interval.tick().await;
			let mut last_fetched = 0u64;
			loop {
				interval.tick().await;
				let span = totals.span.load(Ordering::Relaxed);
				let jobs = totals.jobs.load(Ordering::Relaxed);
				// A retried job can double count.
				let processed = counters.processed.load(Ordering::Relaxed).min(span);
				let fetched = counters.fetched_rpc.load(Ordering::Relaxed);
				let rate = fetched.saturating_sub(last_fetched) as f64
					/ PROGRESS_REPORT_INTERVAL.as_secs_f64();
				last_fetched = fetched;
				let eta = if fetched > 0 {
					let avg_rate = fetched as f64 / started.elapsed().as_secs_f64();
					format_eta(((span - processed) as f64 / avg_rate) as u64)
				} else {
					"unknown".to_string()
				};
				let percent = if span == 0 { 100.0 } else { processed as f64 / span as f64 * 100.0 };
				log::info!(
					"fetch progress: {processed}/{span} blocks ({percent:.1}%), {rate:.0} blocks/s, ETA {eta}, verified {}/{jobs} jobs, backlog: {} extract / {} verify jobs",
					jobs_verified.load(Ordering::Relaxed),
					extract_backlog.len(),
					verify_backlog.len(),
				);
			}
		})
	};

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
	let mut jobs = Vec::with_capacity(totals.jobs.load(Ordering::Relaxed) as usize);
	let mut received: u64 = 0;
	let mut fetch_workers_exited = 0;
	let mut pusher_done = false;
	// The pusher may add jobs after the last worker exited; nothing else wakes the loop then.
	let mut watchdog = tokio::time::interval(Duration::from_secs(1));
	loop {
		let total_jobs = totals.jobs.load(Ordering::Relaxed);
		if pusher_done && received >= total_jobs {
			break;
		}
		// A warm sync with nothing to fetch still spawns one worker that may fail to connect.
		if fetch_workers_exited == num_workers && received < total_jobs {
			log::error!(
				"all fetch workers exited before completing all jobs ({received}/{total_jobs} received)"
			);
			join_set.abort_all();
			return Err(FetchError::NoWorkersConnected);
		}
		tokio::select! {
			Some(result) = join_set.join_next() => {
				match result {
					Ok(Ok(TaskResult::JobPusher)) => pusher_done = true,
					Ok(Ok(TaskResult::FetchWorker)) => fetch_workers_exited += 1,
					Ok(Ok(TaskResult::ComputeWorker)) => {},
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
				log::debug!("verify progress: {received}/{total_jobs} jobs");
			},
			_ = watchdog.tick() => {},
		}
	}

	log::debug!("finished loop");
	reporter.abort();

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
	let final_height = totals.target_height.load(Ordering::Relaxed);
	if final_height > min_height {
		fetch_storage.set_highest_verified_block(chain_id, final_height).await;
	}

	let final_span = totals.span.load(Ordering::Relaxed);
	log::info!("fetched {final_span} blocks; synced to block {final_height}");
	if totals.truncated.load(Ordering::Relaxed) {
		log::warn!(
			"sync stopped at block {final_height} without confirming the chain tip (the tip re-check failed); \
			 results are as of that block - rerun to catch up"
		);
	}

	Ok(final_height.max(min_height))
}

/// The socket may have dropped while the round was processed; reconnect once.
async fn fetch_finalized_height(
	client: &mut MidnightNodeClient,
	url: &str,
) -> Result<u64, FetchError> {
	if let Ok(height) = client.get_finalized_height().await {
		return Ok(height);
	}
	*client = MidnightNodeClient::new(url, None).await?;
	Ok(client.get_finalized_height().await?)
}

async fn connect_with_retry(url: &str, worker_id: usize) -> Result<MidnightNodeClient, FetchError> {
	let backoff = ExponentialBackoff {
		max_elapsed_time: Some(WORKER_CONNECT_MAX_ELAPSED),
		..ExponentialBackoff::default()
	};
	retry_notify(
		backoff,
		|| async {
			MidnightNodeClient::new(url, None)
				.await
				.map_err(|e| backoff::Error::transient(FetchError::from(e)))
		},
		|e: FetchError, wait: Duration| {
			log::warn!(
				"fetch worker {worker_id} could not connect to {url} ({e}); retrying in {:.0}s...",
				wait.as_secs_f32()
			);
		},
	)
	.await
	.inspect_err(|e| {
		log::warn!(
			"fetch worker {worker_id} gave up connecting to {url}: {e}. \
			 This may be due to connection limits on the remote node."
		)
	})
}

#[cfg(test)]
mod tests {
	use super::format_eta;

	#[test]
	fn format_eta_branches() {
		assert_eq!(format_eta(0), "0s");
		assert_eq!(format_eta(59), "59s");
		assert_eq!(format_eta(60), "1m00s");
		assert_eq!(format_eta(3599), "59m59s");
		assert_eq!(format_eta(3600), "1h00m");
		assert_eq!(format_eta(3661), "1h01m");
	}
}

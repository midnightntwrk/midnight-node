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
	future::Future,
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
	task::{Context, Poll},
	time::Duration,
};

use backoff::{ExponentialBackoff, future::retry_notify};
use futures::{Stream, StreamExt, TryStreamExt, stream};
use midnight_node_ledger_helpers::fork::raw_block_data::RawBlockData;
use subxt::{client::OnlineClientAtBlock, rpcs, utils::H256};

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

/// A dropped WebSocket permanently poisons the jsonrpsee client ("restart
/// required"), so every job retry reconnects first.
const JOB_RETRY_MAX_ELAPSED: Duration = Duration::from_secs(10 * 60);
const JOB_RETRY_MAX_INTERVAL: Duration = Duration::from_secs(60);

const WORKER_CONNECT_MAX_ELAPSED: Duration = Duration::from_secs(30);

/// Tip re-checks after the initial span; each one only picks up blocks finalized
/// while the previous round was processed, so a few rounds reach the head.
const MAX_TIP_CHASE_ROUNDS: usize = 20;

/// How often the job planner re-checks whether the planned round has drained.
const TIP_CHASE_POLL: Duration = Duration::from_millis(500);

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

const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(10);

/// `span`/`jobs` are grown by the job planner as it chases the tip;
/// `fetched`/`verified` are advanced by the pipeline stages. The planner waits
/// on `verified` before each re-check, and the reporter reads all of them.
#[derive(Default)]
struct SyncTotals {
	span: AtomicU64,
	jobs: AtomicU64,
	fetched: AtomicU64,
	verified: AtomicU64,
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

/// Split `from..to` into `blocks_per_job`-sized fetch jobs.
fn plan_jobs(from: u64, to: u64, blocks_per_job: u64) -> Vec<FetchTask> {
	(from..to)
		.step_by(blocks_per_job as usize)
		.map(|min| FetchTask::FetchBlocks { min, max: u64::min(min + blocks_per_job, to) })
		.collect()
}

/// Streams the fetch jobs for `min_height..max_height`, then keeps chasing the
/// finalized tip. A round is only re-checked once the work it planned has been
/// verified, so each round picks up real lag rather than the time the re-check
/// itself took.
///
/// `recheck` returning `None` means the node stopped answering; like exhausting
/// `MAX_TIP_CHASE_ROUNDS`, that finishes the sync at what is already planned and
/// flags it truncated rather than failing it. Injecting `recheck` keeps the
/// tip-chasing logic testable without a node.
fn job_stream<F, Fut>(
	min_height: u64,
	max_height: u64,
	blocks_per_job: u64,
	totals: Arc<SyncTotals>,
	recheck: F,
) -> impl Stream<Item = FetchTask>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = Option<u64>>,
{
	// A `None` window end has to be re-read from the node. The first window is
	// already known, so it is handed out before any re-check happens.
	let init = (min_height, Some(max_height.max(min_height)), 0usize, recheck);
	stream::unfold(init, move |(from, known_to, rounds, mut recheck)| {
		let totals = totals.clone();
		async move {
			let (to, rounds) = match known_to {
				Some(to) => (to, rounds),
				None => {
					while totals.verified.load(Ordering::Relaxed)
						< totals.jobs.load(Ordering::Relaxed)
					{
						tokio::time::sleep(TIP_CHASE_POLL).await;
					}
					let rounds = rounds + 1;
					if rounds > MAX_TIP_CHASE_ROUNDS {
						log::warn!(
							"chain keeps advancing faster than it is fetched; finishing sync at block {}",
							from.saturating_sub(1)
						);
						totals.truncated.store(true, Ordering::Relaxed);
						return None;
					}
					let Some(finalized) = recheck().await else {
						log::warn!(
							"could not re-check the chain tip; finishing sync at block {}",
							from.saturating_sub(1)
						);
						totals.truncated.store(true, Ordering::Relaxed);
						return None;
					};
					let to = finalized + 1;
					if to <= from {
						log::info!("caught up with the tip");
						return None;
					}
					let added = to - from;
					// Publish new totals before queueing so the reporter can't
					// observe completion of the old total first.
					totals.span.fetch_add(added, Ordering::Relaxed);
					totals.jobs.fetch_add(added.div_ceil(blocks_per_job), Ordering::Relaxed);
					totals.target_height.store(finalized, Ordering::Relaxed);
					log::info!(
						"chain advanced to {finalized} during sync, fetching {added} more blocks..."
					);
					(to, rounds)
				},
			};
			Some((stream::iter(plan_jobs(from, to, blocks_per_job)), (to, None, rounds, recheck)))
		}
	})
	.flatten()
}

/// Connected clients, one checked out per in-flight fetch job.
///
/// `MidnightNodeClient::new` downloads runtime metadata and remote nodes cap
/// inbound connections, so clients are warmed once up front and reused rather
/// than created per job.
struct ClientPool {
	url: String,
	connected: usize,
	tx: async_channel::Sender<MidnightNodeClient>,
	rx: async_channel::Receiver<MidnightNodeClient>,
}

impl ClientPool {
	/// Connects up to `size` clients concurrently. A client that fails to
	/// connect is skipped and the pool simply runs narrower; if none connect
	/// there is nothing to fetch with.
	async fn connect(url: &str, size: usize) -> Result<Self, FetchError> {
		let (tx, rx) = async_channel::bounded(size.max(1));
		stream::iter(0..size)
			.for_each_concurrent(size, |id| {
				let tx = tx.clone();
				async move {
					if let Ok(client) = connect_with_retry(url, id).await {
						// Capacity is the pool size, so this never blocks.
						let _ = tx.try_send(client);
					}
				}
			})
			.await;

		let connected = rx.len();
		if connected == 0 {
			return Err(FetchError::NoWorkersConnected);
		}
		log::info!("connected {connected} of {size} fetch clients");
		Ok(Self { url: url.to_string(), connected, tx, rx })
	}

	async fn checkout(&self) -> MidnightNodeClient {
		self.rx.recv().await.expect("pool sender is held for the pool's lifetime")
	}

	fn give_back(&self, client: MidnightNodeClient) {
		// Capacity is the pool size and only checked-out clients come back.
		let _ = self.tx.try_send(client);
	}

	/// Clients currently checked out, i.e. fetch jobs in flight.
	fn in_flight(&self) -> usize {
		self.connected.saturating_sub(self.rx.len())
	}
}

/// A pipeline stage running on its own tokio task.
///
/// `buffer_unordered` polls everything it holds from a single task, so the
/// CPU-bound compute stage has to be spawned to reach more than one core.
/// Dropping the handle aborts the task instead of detaching it, so tearing the
/// pipeline down on error can't leave a task writing to storage behind our back
/// - the guarantee the old `JoinSet::abort_all` provided. Folding `JoinError`
/// into `FetchError` here is also what makes this usable with
/// `try_buffer_unordered`, which a bare `JoinHandle` is not.
struct Stage<T>(tokio::task::JoinHandle<Result<T, FetchError>>);

fn stage<T: Send + 'static>(
	fut: impl Future<Output = Result<T, FetchError>> + Send + 'static,
) -> Stage<T> {
	Stage(tokio::spawn(fut))
}

impl<T> Future for Stage<T> {
	type Output = Result<T, FetchError>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		Pin::new(&mut self.get_mut().0)
			.poll(cx)
			.map(|joined| joined.unwrap_or_else(|e| Err(FetchError::WorkerPanic(e.to_string()))))
	}
}

impl<T> Drop for Stage<T> {
	fn drop(&mut self) {
		self.0.abort();
	}
}

/// Fetches one job's blocks. A failed attempt poisons the client, so the next
/// one reconnects first.
async fn run_fetch_job(
	job: FetchTask,
	chain_id: H256,
	pool: &ClientPool,
	storage: impl FetchStorage + Clone + 'static,
	counters: &Arc<FetchCounters>,
	totals: &SyncTotals,
) -> Result<ComputeTask, FetchError> {
	let FetchTask::FetchBlocks { min, max } = job else { return Ok(ComputeTask::NoOp) };

	// `None` after a failed job: the client is poisoned, reconnect first.
	let client = tokio::sync::Mutex::new(Some(pool.checkout().await));
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
				let reconnected = MidnightNodeClient::new(&pool.url, None)
					.await
					.map_err(|e| backoff::Error::transient(FetchError::from(e)))?;
				*guard = Some(reconnected);
			}
			let result = (FetchTask::FetchBlocks { min, max })
				.fetch(
					chain_id,
					guard.as_ref().expect("connected above"),
					storage.clone(),
					counters,
				)
				.await;
			result.map_err(|e| {
				*guard = None;
				backoff::Error::transient(FetchError::from(e))
			})
		},
		|e: FetchError, wait: Duration| {
			log::warn!(
				"fetch job {min}..{max} failed ({e}); reconnecting and retrying in {:.0}s...",
				wait.as_secs_f32()
			);
		},
	)
	.await?;

	// The reconnected client is what goes back, so a retry heals the pool.
	if let Some(client) = client.into_inner() {
		pool.give_back(client);
	}
	totals.fetched.fetch_add(1, Ordering::Relaxed);
	Ok(work_job)
}

/// Drives one job's compute chain, which is strictly linear over the same range
/// (`ExtractBlockData` -> `Verify` -> `FinalVerify`). Returns the `FinalVerify`
/// step: it reads block `max`, owned by the next job, so it can only run once
/// every job's inserts have landed.
async fn run_compute_job(
	mut task: ComputeTask,
	chain_id: H256,
	storage: impl FetchStorage + Clone + 'static,
) -> Result<Option<ComputeTask>, FetchError> {
	loop {
		match task.work(chain_id, storage.clone()).await? {
			done @ ComputeTask::FinalVerify { .. } => return Ok(Some(done)),
			ComputeTask::NoOp => return Ok(None),
			next => task = next,
		}
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

	let num_compute_workers = num_compute_workers.max(1);

	let totals = Arc::new(SyncTotals {
		span: AtomicU64::new(fetch_span),
		jobs: AtomicU64::new(num_jobs),
		target_height: AtomicU64::new(finalized_height),
		..Default::default()
	});

	let counters = Arc::new(FetchCounters::default());
	let pool = Arc::new(ClientPool::connect(url, num_workers).await?);
	log::info!("computing with up to {num_compute_workers} concurrent workers");

	// Progress reporter, on its own task so pipeline stages can't starve it.
	let reporter = {
		let counters = counters.clone();
		let totals = totals.clone();
		let pool = pool.clone();
		tokio::spawn(async move {
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
				let jobs_fetched = totals.fetched.load(Ordering::Relaxed);
				let jobs_verified = totals.verified.load(Ordering::Relaxed);
				let rate = fetched.saturating_sub(last_fetched) as f64
					/ PROGRESS_REPORT_INTERVAL.as_secs_f64();
				last_fetched = fetched;
				let eta = if fetched > 0 {
					let avg_rate = fetched as f64 / started.elapsed().as_secs_f64();
					format_eta(((span - processed) as f64 / avg_rate) as u64)
				} else {
					"unknown".to_string()
				};
				let percent =
					if span == 0 { 100.0 } else { processed as f64 / span as f64 * 100.0 };
				log::info!(
					"fetch progress: {processed}/{span} blocks ({percent:.1}%), {rate:.0} blocks/s, ETA {eta}, verified {jobs_verified}/{jobs} jobs, in flight: {} fetch / {} compute jobs",
					pool.in_flight(),
					jobs_fetched.saturating_sub(jobs_verified),
				);
			}
		})
	};

	// The socket may drop while a round is processed, so the re-check reuses one
	// client and reconnects it once rather than building a fresh one each time.
	let recheck = {
		let url = url.to_string();
		let client = Arc::new(tokio::sync::Mutex::new(client));
		move || {
			let url = url.clone();
			let client = client.clone();
			async move {
				let mut guard = client.lock().await;
				fetch_finalized_height(&mut guard, &url)
					.await
					.inspect_err(|e| log::warn!("could not re-check the chain tip ({e})"))
					.ok()
			}
		}
	};

	let final_jobs = job_stream(min_height, max_height, blocks_per_job, totals.clone(), recheck)
		.map(|job| run_fetch_job(job, chain_id, &pool, fetch_storage.clone(), &counters, &totals))
		.buffer_unordered(num_workers)
		.map_ok(|task| stage(run_compute_job(task, chain_id, fetch_storage.clone())))
		.try_buffer_unordered(num_compute_workers)
		.inspect_ok(|_| {
			totals.verified.fetch_add(1, Ordering::Relaxed);
		})
		.try_collect::<Vec<Option<ComputeTask>>>()
		.await?;

	// `try_collect` resolved every stage, so no task holding a `fetch_storage`
	// clone outlives this point (see `Stage`) - which is what lets a later
	// `fetch_all` in the same process reopen the cache file. Only the reporter
	// is still running.
	reporter.abort();

	log::info!("all blocks fetched, running final boundary verification...");
	for job in final_jobs.into_iter().flatten() {
		job.work(chain_id, fetch_storage.clone()).await?;
	}
	log::info!("all blocks verified");

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
	let t = std::time::Instant::now();
	let blocks = read_blocks_from_cache(chain_id, fetch_storage).await?;
	log::debug!("[perf] fetch_from_rpc read_blocks_from_cache took {:?}", t.elapsed());

	let final_span = totals.span.load(Ordering::Relaxed);
	log::info!(
		"fetched {} blocks, read {} blocks from cache, total transactions: {}; synced to block {final_height}",
		final_span,
		blocks.len().saturating_sub(final_span as usize),
		blocks.iter().fold(0, |acc, b| acc + b.transactions.len()),
	);
	if totals.truncated.load(Ordering::Relaxed) {
		log::warn!(
			"sync stopped at block {final_height} without confirming the chain tip (the tip re-check failed); \
			 results are as of that block - rerun to catch up"
		);
	}

	Ok(blocks)
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

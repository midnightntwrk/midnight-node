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

pub mod compute_task;
pub mod fetch_storage;
pub mod fetch_task;
pub mod runtimes;

use backoff::{ExponentialBackoff, future::retry};
use midnight_node_ledger_helpers::{DB, ProofKind, SignatureKind, Tagged};
use subxt::{OnlineClient, blocks::Block, ext::subxt_rpcs};

use crate::{
	client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig},
	fetcher::{
		compute_task::{ComputeError, ComputeTask},
		fetch_storage::{BlockData, FetchStorage},
		fetch_task::{FetchTask, FetchTaskError},
	},
};

pub type MidnightBlock = Block<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>;

/// Number of blocks to process per batch. Tuned for memory/parallelism tradeoff.
const BLOCKS_PER_JOB: u64 = 100;

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
}

pub async fn new_client(url: &str) -> MidnightNodeClient {
	retry(ExponentialBackoff::default(), || async {
		MidnightNodeClient::new(&url).await.map_err(|e| {
			log::warn!("rpc fetch failed, retrying: {e}");
			backoff::Error::transient(e)
		})
	})
	.await
	.expect("failed to fetch from node after retrying")
}

pub async fn fetch_all<
	S: SignatureKind<D> + Tagged,
	P: ProofKind<D> + core::fmt::Debug,
	D: DB + Clone,
>(
	url: &str,
	num_workers: usize,
	fetch_storage: impl FetchStorage<S, P, D> + Clone + Send + Sync + 'static,
	height: usize,
) -> Result<Vec<BlockData<S, P, D>>, FetchError> {
	if std::env::var("SYNC_CACHE").is_ok() {
		panic!(
			"Error: 'SYNC_CACHE' is defined - please use 'FETCH_CACHE' instead. See `--help` for more info."
		);
	}

	let client = new_client(&url).await;
	// TODO: use full height
	let finalized_height =
		client.get_finalized_height().await.map_err(|e| Into::<FetchError>::into(e))?;
	// let finalized_height = height as u64;
	let chain_id = client.get_block_one_hash().await.map_err(|e| Into::<FetchError>::into(e))?;

	let num_cpu_workers = num_cpus::get();

	let (fetch_job_sender, fetch_job_receiver) = async_channel::bounded(num_workers * 2);
	let (compute_job_sender, compute_job_receiver) = async_channel::bounded(num_cpu_workers * 2);
	let (final_jobs_sender, final_jobs_receiver) = async_channel::bounded(num_cpu_workers * 2);

	// Push jobs into queue
	{
		let job_sender = fetch_job_sender.clone();
		let finalized_height = finalized_height;
		tokio::spawn(async move {
			for min in (0..finalized_height + 1).step_by(BLOCKS_PER_JOB as usize) {
				let max = u64::min(min + BLOCKS_PER_JOB, finalized_height + 1);
				log::info!("pushing new fetch job {min} -> {max}...");
				job_sender
					.send(FetchTask::FetchBlocks { min, max })
					.await
					.expect("failed to push job on channel");
			}
		});
	}

	log::info!("spawning {num_workers} fetch workers");

	// Spawn fetch workers
	for _ in 0..num_workers {
		let job_receiver = fetch_job_receiver.clone();
		let work_job_sender = compute_job_sender.clone();
		let fetch_storage = fetch_storage.clone();
		let url = url.to_string();
		tokio::spawn(async move {
			let client = new_client(&url).await;
			loop {
				let Ok(job) = job_receiver.recv().await else {
					return;
				};

				log::info!("received new job...");

				let work_job = job
					.fetch(chain_id, &client, fetch_storage.clone())
					.await
					.expect("failed to fetch from node after retrying");

				work_job_sender.send(work_job).await.expect("failed to push job on work queue");
			}
		});
	}

	log::info!("spawning {num_cpu_workers} compute workers");

	// Spawn compute workers
	for _ in 0..num_cpus::get() {
		let work_job_receiver = compute_job_receiver.clone();
		let work_job_sender = compute_job_sender.clone();
		let final_jobs_sender = final_jobs_sender.clone();
		let fetch_storage = fetch_storage.clone();
		tokio::spawn(async move {
			loop {
				let Ok(job) = work_job_receiver.recv().await else {
					return;
				};

				log::info!("received new work job...");

				let work_job = job
					.work(chain_id, fetch_storage.clone())
					.await
					.expect("failed to process work job");

				match &work_job {
					ComputeTask::FinalVerify { .. } => {
						final_jobs_sender.send(work_job).await.expect("failed to push final job");
					},
					ComputeTask::NoOp => continue,
					_ => work_job_sender
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
	let num_jobs = (finalized_height / BLOCKS_PER_JOB) + 1;
	let mut jobs = Vec::with_capacity(num_jobs as usize);
	for i in (0..finalized_height + 1).step_by(BLOCKS_PER_JOB as usize) {
		log::info!("job {i}/{finalized_height}");
		let job = final_jobs_receiver
			.recv()
			.await
			.expect("failed to receive final job from channel");
		jobs.push(job);
	}

	for job in jobs {
		job.work(chain_id, fetch_storage.clone()).await?;
	}
	log::info!("all blocks verified");

	// Close channels to exit workers
	fetch_job_receiver.close();
	compute_job_receiver.close();
	final_jobs_receiver.close();

	let blocks: Vec<_> = fetch_storage
		.get_block_data_range(chain_id, (0..finalized_height).into_iter())
		.await
		.into_iter()
		.map(|b| b.expect("missing block"))
		.collect();

	Ok(blocks)
}

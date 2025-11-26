use backoff::{ExponentialBackoff, future::retry};
use hex::ToHex;
use midnight_node_ledger_helpers::{
	BlockContext, DB, DefaultDB, HashOutput, Timestamp, midnight_serialize::tagged_deserialize,
};
use subxt::{
	backend::legacy::rpc_methods::ConsensusEngineId, blocks::ExtrinsicEvents,
	config::substrate::DigestItem, ext::subxt_rpcs, utils::H256,
};

use crate::{
	client::{ClientError, MidnightNodeClient},
	indexer::{
		fetch_storage::{self, BlockData, FetchStorage, FetchedBlock, FetchedTransaction},
		runtimes::{
			MidnightMetadata, MidnightMetadata0_17_0, MidnightMetadata0_17_1,
			MidnightMetadata0_18_0, MidnightMetadata0_18_1, RuntimeVersion, RuntimeVersionError,
		},
	},
};

type FetchResult = Result<WorkJob, FetchError>;
type WorkResult = Result<WorkJob, WorkError>;

#[derive(Debug, thiserror::Error)]
pub enum FetchWorkError {
	#[error("fetch error")]
	FetchError(#[from] FetchError),
	#[error("work error")]
	WorkError(#[from] WorkError),
}

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
}

#[derive(Debug, thiserror::Error)]
pub enum WorkError {
	#[error("subxt error while processing block")]
	SubxtError(#[from] subxt::Error),
	#[error("block missing {0}")]
	BlockMissing(u64),
	#[error("RuntimeVersionError: {0}")]
	RuntimeVersionError(#[from] RuntimeVersionError),
	#[error("ledger deserialization error")]
	LedgerDeserializationError(std::io::Error),
	#[error("verification failed, child block {0}")]
	ChildBlockVerificationFailed(u32),
}

pub enum FetchJob {
	FetchBlocks { min: u64, max: u64 },
	NoOp,
}

pub enum WorkJob {
	ExtractBlockData { min: u64, max: u64 },
	VerifyBlocks { min: u64, max: u64 },
	FinalVerify { min: u64, max: u64 },
	NoOp,
}

impl WorkJob {
	async fn work<D: DB + Clone>(
		self,
		chain_id: &[u8],
		storage: impl FetchStorage<D> + Send + Sync,
	) -> WorkResult {
		match self {
			WorkJob::ExtractBlockData { min, max } => {
				log::info!("extracting block data {min}..{max}");
				let block_data = storage.get_block_data_range(chain_id, min..max).await;
				let blocks_to_fetch = (min..max)
					.into_iter()
					.zip(block_data.iter())
					.filter_map(|(i, b)| if b.is_none() { Some(i) } else { None });
				let blocks =
					storage.get_block_range(chain_id, blocks_to_fetch.clone().into_iter()).await;

				let mut blocks_to_insert = Vec::new();
				for (i, b) in blocks_to_fetch.into_iter().zip(blocks.into_iter()) {
					let b = match b {
						Some(b) => b,
						None => return Err(WorkError::BlockMissing(i)),
					};
					let block_data = Self::extract_data(&b).await?;
					blocks_to_insert.push((i, block_data));
				}
				storage.insert_block_data_range(chain_id, blocks_to_insert.into_iter()).await;
				log::info!("extracting block data {min}..{max}: complete");
				Ok(WorkJob::VerifyBlocks { min, max })
			},
			WorkJob::VerifyBlocks { min, max } => {
				log::info!("verifying {min}..{max}");
				let blocks = storage.get_block_range(chain_id, (min..max).into_iter()).await;
				let some_failing_pair =
					blocks.iter().zip(blocks.iter().skip(1)).find(|(parent, child)| {
						if let Some(p) = parent {
							if let Some(c) = child {
								return p.block.hash() != c.block.header().parent_hash;
							}
						}
						true
					});

				if let Some((Some(_parent), Some(child))) = some_failing_pair {
					return Err(WorkError::ChildBlockVerificationFailed(child.block.number()));
				}

				log::info!("verifying {min}..{max}: complete");

				Ok(WorkJob::FinalVerify { min, max })
			},
			WorkJob::FinalVerify { min, max } => {
				log::info!("final verify {min} and {max}");
				// Check min
				let blocks = storage.get_block_range(chain_id, [min - 1, min].into_iter()).await;
				if let [Some(parent), Some(child)] = &blocks[..] {
					if child.block.header().parent_hash != parent.block.hash() {
						return Err(WorkError::ChildBlockVerificationFailed(child.block.number()));
					}
				}
				// Check max
				let blocks = storage.get_block_range(chain_id, [max, max + 1].into_iter()).await;
				if let [Some(parent), Some(child)] = &blocks[..] {
					if child.block.header().parent_hash != parent.block.hash() {
						return Err(WorkError::ChildBlockVerificationFailed(child.block.number()));
					}
				}

				log::info!("final verify {min} and {max}: complete");
				Ok(WorkJob::NoOp)
			},
			WorkJob::NoOp => Ok(WorkJob::NoOp),
		}
	}

	async fn extract_data<D: DB + Clone>(block: &FetchedBlock) -> Result<BlockData<D>, WorkError> {
		let version_number = block
			.block
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
				Self::process_block_with_protocol::<MidnightMetadata0_17_0, D>(block).await
			},
			RuntimeVersion::V0_17_1 => {
				Self::process_block_with_protocol::<MidnightMetadata0_17_1, D>(block).await
			},
			RuntimeVersion::V0_18_0 => {
				Self::process_block_with_protocol::<MidnightMetadata0_18_0, D>(block).await
			},
			RuntimeVersion::V0_18_1 => {
				Self::process_block_with_protocol::<MidnightMetadata0_18_1, D>(block).await
			},
		}
	}

	async fn process_block_with_protocol<M: MidnightMetadata, D: DB + Clone>(
		block: &FetchedBlock,
	) -> Result<BlockData<D>, WorkError> {
		let state_root = block.state_root.clone();
		let block_header = block.block.header();
		let parent_block_hash = block_header.parent_hash;

		let extrinsics = block
			.block
			.extrinsics()
			.await
			.unwrap_or_else(|err| panic!("Error while fetching the transactions: {}", err));
		let events = block
			.block
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
					.map_err(|err| WorkError::LedgerDeserializationError(err))?;
				transactions.push(FetchedTransaction::Midnight(tx));
			} else if let Some(bytes) = M::send_mn_system_transaction(&call) {
				let tx = tagged_deserialize(&mut bytes.as_slice())
					.map_err(|err| WorkError::LedgerDeserializationError(err))?;
				transactions.push(FetchedTransaction::System(tx));
			} else if M::check_for_events(&call) {
				let ext_events = ExtrinsicEvents::new(ext.hash(), ext.index(), events.clone());
				for ev in ext_events.iter().filter_map(Result::ok) {
					if let Some(event) = ev.as_event::<M::SystemTransactionAppliedEvent>()? {
						let bytes = M::system_transaction_applied(event);
						let tx = tagged_deserialize(&mut bytes.as_slice())
							.map_err(|err| WorkError::LedgerDeserializationError(err))?;
						transactions.push(FetchedTransaction::System(tx));
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
		Ok(BlockData { transactions, context, state_root })
	}
}

impl FetchJob {
	async fn fetch<D: DB + Clone>(
		&self,
		chain_id: &[u8],
		client: &MidnightNodeClient,
		storage: impl FetchStorage<D> + Send + Sync,
	) -> FetchResult {
		match self {
			FetchJob::FetchBlocks { min, max } => {
				log::info!("fetching blocks {min}..{max}");
				let blocks = storage.get_block_range(chain_id, *min..*max).await;
				let mut blocks_to_insert = Vec::new();
				for (i, b) in (*min..*max).into_iter().zip(blocks.into_iter()) {
					let block_hash = Self::fetch_block_hash(client, i).await?;
					let block = match b {
						Some(b) => b,
						None => Self::fetch_block(client, block_hash).await?,
					};
					blocks_to_insert.push((i, block));
				}
				storage.insert_block_range(chain_id, blocks_to_insert.into_iter()).await;
				log::info!("fetching blocks {min}..{max}: complete");
				Ok(WorkJob::ExtractBlockData { min: *min, max: *max })
			},
			FetchJob::NoOp => {
				todo!()
			},
		}
	}

	async fn fetch_block_hash(
		client: &MidnightNodeClient,
		block_number: u64,
	) -> Result<H256, FetchError> {
		log::debug!("fetching block hash for number {block_number}...");
		let block_hash = client
			.rpc
			.chain_get_block_hash(Some(subxt::backend::legacy::rpc_methods::NumberOrHex::Number(
				block_number,
			)))
			.await?
			.ok_or(FetchError::BlockHashMissing(block_number))?;

		Ok(block_hash)
	}

	async fn fetch_block(
		client: &MidnightNodeClient,
		block_hash: H256,
	) -> Result<FetchedBlock, FetchError> {
		log::debug!("fetching block for hash {}...", block_hash.0.encode_hex::<String>());

		let block = retry(ExponentialBackoff::default(), || async {
			client.api.blocks().at(block_hash).await.map_err(|e| {
				eprintln!("rpc fetch failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await?;

		let state_root = client.get_state_root_at(Some(block.hash())).await?;

		Ok(FetchedBlock { block, state_root })
	}
}

pub async fn new_client(url: &str) -> MidnightNodeClient {
	retry(ExponentialBackoff::default(), || async {
		MidnightNodeClient::new(&url).await.map_err(|e| {
			eprintln!("rpc fetch failed, retrying: {e}");
			backoff::Error::transient(e)
		})
	})
	.await
	.expect("failed to fetch from node after retrying")
}

pub async fn fetch_all(
	url: &str,
	num_workers: usize,
	height: usize,
) -> Result<Vec<FetchedBlock>, FetchWorkError> {
	let client = new_client(&url).await;
	// TODO: use full height
	let finalized_height =
		client.get_finalized_height().await.map_err(|e| Into::<FetchError>::into(e))?;
	let finalized_height = height as u64;
	let chain_id = client.get_block_one_hash().await.map_err(|e| Into::<FetchError>::into(e))?;
	let fetch_storage = fetch_storage::InMemory::<DefaultDB>::default();

	const STEP_SIZE: u64 = 100;

	let (fetch_job_sender, fetch_job_receiver) = async_channel::unbounded();
	let (work_job_sender, work_job_receiver) = async_channel::unbounded();
	let (final_jobs_sender, final_jobs_receiver) = async_channel::unbounded();

	// Push jobs into queue
	{
		let job_sender = fetch_job_sender.clone();
		let finalized_height = finalized_height;
		tokio::spawn(async move {
			for min in (0..finalized_height + 1).step_by(STEP_SIZE as usize) {
				let max = u64::min(min + STEP_SIZE, finalized_height + 1);
				log::info!("pushing new fetch job {min} -> {max}...");
				job_sender
					.send(FetchJob::FetchBlocks { min, max })
					.await
					.expect("failed to push job on channel");
			}
		});
	}

	println!("spawn workers");

	// Spawn fetch workers
	for _ in 0..num_workers {
		let job_receiver = fetch_job_receiver.clone();
		let work_job_sender = work_job_sender.clone();
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
					.fetch(&chain_id.0, &client, fetch_storage.clone())
					.await
					.expect("failed to fetch from node after retrying");

				work_job_sender.send(work_job).await.expect("failed to push job on work queue");
			}
		});
	}

	// Spawn work workers
	for _ in 0..num_cpus::get() {
		let work_job_receiver = work_job_receiver.clone();
		let work_job_sender = work_job_sender.clone();
		let final_jobs_sender = final_jobs_sender.clone();
		let fetch_storage = fetch_storage.clone();
		tokio::spawn(async move {
			loop {
				let Ok(job) = work_job_receiver.recv().await else {
					return;
				};

				log::info!("received new work job...");

				let work_job = job
					.work(&chain_id.0, fetch_storage.clone())
					.await
					.expect("failed to process work job");

				match &work_job {
					WorkJob::FinalVerify { .. } => {
						final_jobs_sender.send(work_job).await.expect("failed to push final job");
					},
					WorkJob::NoOp => continue,
					_ => work_job_sender
						.send(work_job)
						.await
						.expect("failed to push job on work queue"),
				};
			}
		});
	}

	println!("receive blocks");

	println!("final verify step");
	// Receive final jobs
	let num_jobs = ((finalized_height / STEP_SIZE) + finalized_height % STEP_SIZE) as usize;
	let mut jobs = Vec::with_capacity(num_jobs);
	for i in (0..finalized_height + 1).step_by(STEP_SIZE as usize) {
		println!("job {i}/{finalized_height}");
		let job = final_jobs_receiver
			.recv()
			.await
			.expect("failed to receive final job from channel");
		jobs.push(job);
	}

	for job in jobs {
		job.work(&chain_id.0, fetch_storage.clone()).await?;
	}
	println!("all blocks verified");

	// Close channels to exit workers
	fetch_job_receiver.close();

	let blocks: Vec<_> = fetch_storage
		.get_block_range(&chain_id.0, (0..finalized_height).into_iter())
		.await
		.into_iter()
		.map(|b| b.expect("missing block"))
		.collect();

	Ok(blocks)
}

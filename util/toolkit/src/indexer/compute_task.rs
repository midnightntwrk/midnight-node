use midnight_node_ledger_helpers::{
	BlockContext, DB, HashOutput, Timestamp, midnight_serialize::tagged_deserialize,
};
use subxt::{
	blocks::ExtrinsicEvents,
	config::substrate::{ConsensusEngineId, DigestItem},
};

use crate::indexer::{
	fetch_storage::{BlockData, FetchStorage, FetchedBlock, FetchedTransaction},
	runtimes::{
		MidnightMetadata, MidnightMetadata0_17_0, MidnightMetadata0_17_1, MidnightMetadata0_18_0,
		MidnightMetadata0_18_1, RuntimeVersion, RuntimeVersionError,
	},
};

type ComputeResult = Result<ComputeTask, ComputeError>;

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
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

pub enum ComputeTask {
	ExtractBlockData { min: u64, max: u64 },
	Verify { min: u64, max: u64 },
	FinalVerify { min: u64, max: u64 },
	NoOp,
}

impl ComputeTask {
	pub async fn work<D: DB + Clone>(
		self,
		chain_id: &[u8],
		storage: impl FetchStorage<D> + Send + Sync,
	) -> ComputeResult {
		match self {
			ComputeTask::ExtractBlockData { min, max } => {
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
						None => return Err(ComputeError::BlockMissing(i)),
					};
					let block_data = Self::extract_data(&b).await?;
					blocks_to_insert.push((i, block_data));
				}
				storage.insert_block_data_range(chain_id, blocks_to_insert.into_iter()).await;
				log::info!("extracting block data {min}..{max}: complete");
				Ok(ComputeTask::Verify { min, max })
			},
			ComputeTask::Verify { min, max } => {
				log::info!("verifying {min}..{max}");
				let blocks = storage.get_block_range(chain_id, (min..max).into_iter()).await;
				let blocks: Result<Vec<FetchedBlock>, ComputeError> = (min..max)
					.into_iter()
					.zip(blocks.into_iter())
					.map(|(i, b)| b.ok_or(ComputeError::BlockMissing(i)))
					.collect();
				let blocks = blocks?;
				let some_failing_pair =
					blocks.iter().zip(blocks.iter().skip(1)).find(|(parent, child)| {
						parent.block.hash() != child.block.header().parent_hash
					});

				if let Some((_parent, child)) = some_failing_pair {
					return Err(ComputeError::ChildBlockVerificationFailed(child.block.number()));
				}

				log::info!("verifying {min}..{max}: complete");

				Ok(ComputeTask::FinalVerify { min, max })
			},
			ComputeTask::FinalVerify { min, max } => {
				log::info!("final verify {min} and {max}");
				// Check min
				if min == 0 {
					let block = storage
						.get_block(chain_id, 0)
						.await
						.ok_or(ComputeError::BlockMissing(0))?;
					if block.block.header().parent_hash.is_zero() {
						return Err(ComputeError::ChildBlockVerificationFailed(0));
					}
				} else {
					let blocks =
						storage.get_block_range(chain_id, [min - 1, min].into_iter()).await;
					if let [Some(parent), Some(child)] = &blocks[..] {
						if child.block.header().parent_hash != parent.block.hash() {
							return Err(ComputeError::ChildBlockVerificationFailed(
								child.block.number(),
							));
						}
					}
				}
				// Check max
				let blocks = storage.get_block_range(chain_id, [max, max + 1].into_iter()).await;
				if let [Some(parent), Some(child)] = &blocks[..] {
					if child.block.header().parent_hash != parent.block.hash() {
						return Err(ComputeError::ChildBlockVerificationFailed(
							child.block.number(),
						));
					}
				}

				log::info!("final verify {min} and {max}: complete");
				Ok(ComputeTask::NoOp)
			},
			ComputeTask::NoOp => Ok(ComputeTask::NoOp),
		}
	}

	async fn extract_data<D: DB + Clone>(
		block: &FetchedBlock,
	) -> Result<BlockData<D>, ComputeError> {
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
	) -> Result<BlockData<D>, ComputeError> {
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
					.map_err(|err| ComputeError::LedgerDeserializationError(err))?;
				transactions.push(FetchedTransaction::Midnight(tx));
			} else if let Some(bytes) = M::send_mn_system_transaction(&call) {
				let tx = tagged_deserialize(&mut bytes.as_slice())
					.map_err(|err| ComputeError::LedgerDeserializationError(err))?;
				transactions.push(FetchedTransaction::System(tx));
			} else if M::check_for_events(&call) {
				let ext_events = ExtrinsicEvents::new(ext.hash(), ext.index(), events.clone());
				for ev in ext_events.iter().filter_map(Result::ok) {
					if let Some(event) = ev.as_event::<M::SystemTransactionAppliedEvent>()? {
						let bytes = M::system_transaction_applied(event);
						let tx = tagged_deserialize(&mut bytes.as_slice())
							.map_err(|err| ComputeError::LedgerDeserializationError(err))?;
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

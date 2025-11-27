use backoff::{ExponentialBackoff, future::retry};
use hex::ToHex as _;
use midnight_node_ledger_helpers::DB;
use subxt::{ext::subxt_rpcs, utils::H256};

use crate::{
	client::{ClientError, MidnightNodeClient},
	indexer::{
		compute_task::ComputeTask,
		fetch_storage::{FetchStorage, FetchedBlock},
	},
};

type FetchResult = Result<ComputeTask, FetchTaskError>;

#[derive(Debug, thiserror::Error)]
pub enum FetchTaskError {
	#[error("subxt error while fetching")]
	SubxtError(#[from] subxt::Error),
	#[error("subxt rpc error while fetching")]
	SubxtRpcError(#[from] subxt_rpcs::Error),
	#[error("node client error")]
	NodeClientError(#[from] ClientError),
	#[error("block hash missing for block number {0}")]
	BlockHashMissing(u64),
}

pub enum FetchTask {
	FetchBlocks { min: u64, max: u64 },
	NoOp,
}

impl FetchTask {
	pub async fn fetch<D: DB + Clone>(
		&self,
		chain_id: &[u8],
		client: &MidnightNodeClient,
		storage: impl FetchStorage<D> + Send + Sync,
	) -> FetchResult {
		match self {
			FetchTask::FetchBlocks { min, max } => {
				log::info!("fetching blocks {min}..{max}");
				let blocks = storage.get_block_range(chain_id, *min..*max).await;
				let mut blocks_to_insert = Vec::new();
				for (i, b) in (*min..*max).into_iter().zip(blocks.into_iter()) {
					let block = match b {
						Some(b) => b,
						None => {
							let block_hash = Self::fetch_block_hash(client, i).await?;
							Self::fetch_block(client, block_hash).await?
						},
					};
					blocks_to_insert.push((i, block));
				}
				storage.insert_block_range(chain_id, blocks_to_insert.into_iter()).await;
				log::info!("fetching blocks {min}..{max}: complete");
				Ok(ComputeTask::ExtractBlockData { min: *min, max: *max })
			},
			FetchTask::NoOp => Ok(ComputeTask::NoOp),
		}
	}

	async fn fetch_block_hash(
		client: &MidnightNodeClient,
		block_number: u64,
	) -> Result<H256, FetchTaskError> {
		log::debug!("fetching block hash for number {block_number}...");
		let block_hash = client
			.rpc
			.chain_get_block_hash(Some(subxt::backend::legacy::rpc_methods::NumberOrHex::Number(
				block_number,
			)))
			.await?
			.ok_or(FetchTaskError::BlockHashMissing(block_number))?;

		Ok(block_hash)
	}

	async fn fetch_block(
		client: &MidnightNodeClient,
		block_hash: H256,
	) -> Result<FetchedBlock, FetchTaskError> {
		log::debug!("fetching block for hash {}...", block_hash.0.encode_hex::<String>());

		let block = retry(ExponentialBackoff::default(), || async {
			client.api.blocks().at(block_hash).await.map_err(|e| {
				log::warn!("rpc fetch failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await?;

		let state_root = client.get_state_root_at(Some(block.hash())).await?;

		Ok(FetchedBlock { block, state_root })
	}
}

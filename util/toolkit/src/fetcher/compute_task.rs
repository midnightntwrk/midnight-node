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

use midnight_node_ledger_helpers::{
	BlockContext, DB, HashOutput, ProofKind, SignatureKind, Tagged, Timestamp,
	midnight_serialize::tagged_deserialize,
};
use subxt::{
	blocks::ExtrinsicEvents,
	config::substrate::{ConsensusEngineId, DigestItem},
	utils::H256,
};

use crate::fetcher::{
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
	ChildBlockVerificationFailed(u64),
}

pub enum ComputeTask {
	ExtractBlockData { min: u64, max: u64, blocks: Vec<FetchedBlock> },
	Verify { min: u64, max: u64 },
	FinalVerify { min: u64, max: u64 },
	NoOp,
}

impl ComputeTask {
	pub async fn work<
		S: SignatureKind<D> + Tagged,
		P: ProofKind<D> + core::fmt::Debug,
		D: DB + Clone,
	>(
		self,
		chain_id: H256,
		storage: impl FetchStorage<S, P, D> + Send + Sync,
	) -> ComputeResult {
		match self {
			ComputeTask::ExtractBlockData { min, max, blocks } => {
				log::info!("extracting block data {min}..{max}");
				let mut blocks_to_insert = Vec::new();
				for b in blocks {
					let block_data = Self::extract_data(&b).await?;
					blocks_to_insert.push((b.block.number() as u64, block_data));
				}
				storage.insert_block_data_range(chain_id, blocks_to_insert.into_iter()).await;
				log::info!("extracting block data {min}..{max}: complete");
				Ok(ComputeTask::Verify { min, max })
			},
			ComputeTask::Verify { min, max } => {
				log::info!("verifying {min}..{max}");
				let blocks = storage.get_block_data_range(chain_id, (min..max).into_iter()).await;
				let blocks: Result<Vec<BlockData<S, P, D>>, ComputeError> = (min..max)
					.into_iter()
					.zip(blocks.into_iter())
					.map(|(i, b)| b.ok_or(ComputeError::BlockMissing(i)))
					.collect();
				let blocks = blocks?;
				let some_failing_pair = blocks
					.iter()
					.zip(blocks.iter().skip(1))
					.find(|(parent, child)| parent.hash != child.parent_hash);

				if let Some((_parent, child)) = some_failing_pair {
					return Err(ComputeError::ChildBlockVerificationFailed(child.number));
				}

				log::info!("verifying {min}..{max}: complete");

				Ok(ComputeTask::FinalVerify { min, max })
			},
			ComputeTask::FinalVerify { min, max } => {
				log::info!("final verify {min} and {max}");

				// Check min - only for genesis block
				if min == 0 {
					let block = storage
						.get_block_data(chain_id, 0)
						.await
						.ok_or(ComputeError::BlockMissing(0))?;
					if !block.parent_hash.is_zero() {
						return Err(ComputeError::ChildBlockVerificationFailed(0));
					}
				}
				// For min > 0: previous batch's max check already verified this boundary

				// Check max - verify forward connection to next batch
				let blocks =
					storage.get_block_data_range(chain_id, [max - 1, max].into_iter()).await;
				if let [Some(parent), Some(child)] = &blocks[..] {
					if child.parent_hash != parent.hash {
						return Err(ComputeError::ChildBlockVerificationFailed(child.number));
					}
				}
				// If child (block `max`) doesn't exist, we're at the last batch - no forward check needed

				log::info!("final verify {min} and {max}: complete");
				Ok(ComputeTask::NoOp)
			},
			ComputeTask::NoOp => Ok(ComputeTask::NoOp),
		}
	}

	async fn extract_data<
		S: SignatureKind<D> + Tagged,
		P: ProofKind<D> + core::fmt::Debug,
		D: DB + Clone,
	>(
		block: &FetchedBlock,
	) -> Result<BlockData<S, P, D>, ComputeError> {
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
				Self::process_block_with_protocol::<MidnightMetadata0_17_0, S, P, D>(block).await
			},
			RuntimeVersion::V0_17_1 => {
				Self::process_block_with_protocol::<MidnightMetadata0_17_1, S, P, D>(block).await
			},
			RuntimeVersion::V0_18_0 => {
				Self::process_block_with_protocol::<MidnightMetadata0_18_0, S, P, D>(block).await
			},
			RuntimeVersion::V0_18_1 => {
				Self::process_block_with_protocol::<MidnightMetadata0_18_1, S, P, D>(block).await
			},
		}
	}

	async fn process_block_with_protocol<
		M: MidnightMetadata,
		S: SignatureKind<D> + Tagged,
		P: ProofKind<D> + core::fmt::Debug,
		D: DB + Clone,
	>(
		block: &FetchedBlock,
	) -> Result<BlockData<S, P, D>, ComputeError> {
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
		let hash = block.block.hash();
		let parent_hash = block.block.header().parent_hash;
		let number = block.block.number() as u64;
		Ok(BlockData { hash, parent_hash, number, transactions, context, state_root })
	}
}

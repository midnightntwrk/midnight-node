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

use midnight_node_ledger_helpers::fork::raw_block_data::{
	LedgerVersion, RawBlockData, RawTransaction,
};
use subxt::{
	config::substrate::{ConsensusEngineId, DigestItem},
	utils::H256,
};

use crate::{
	client::MidnightNodeClientConfig,
	fetcher::{
		fetch_storage::{FetchStorage, FetchedBlock},
		runtimes::{
			MidnightMetadata, MidnightMetadata0_21_0, MidnightMetadata0_22_0,
			MidnightMetadata1_0_0, MidnightMetadata2_0_0, MidnightMetadata2_1_0, RuntimeVersion,
			RuntimeVersionError,
		},
	},
};

type ComputeResult = Result<ComputeTask, ComputeError>;

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
	#[error("subxt error while processing block")]
	SubxtError(#[from] subxt::Error),
	#[error("events error: {0}")]
	EventsError(#[from] subxt::error::EventsError),
	#[error("block error: {0}")]
	BlockError(#[from] subxt::error::BlockError),
	#[error("block missing {0}")]
	BlockMissing(u64),
	#[error("RuntimeVersionError: {0}")]
	RuntimeVersionError(#[from] RuntimeVersionError),
	#[error("verification failed, child block {0}")]
	ChildBlockVerificationFailed(u64),
	#[error("spec version in block {0} doesn't have a defined ledger version mapping")]
	LedgerVersionMissing(u64),
}

pub enum ComputeTask {
	ExtractBlockData { min: u64, max: u64, blocks: Vec<FetchedBlock> },
	Verify { min: u64, max: u64 },
	FinalVerify { min: u64, max: u64 },
	NoOp,
}

impl ComputeTask {
	pub async fn work(self, chain_id: H256, storage: impl FetchStorage) -> ComputeResult {
		match self {
			ComputeTask::ExtractBlockData { min, max, blocks } => {
				log::debug!("extracting block data {min}..{max}");
				let mut blocks_to_insert = Vec::new();
				for b in blocks {
					let block_data = Self::extract_data(&b).await?;
					blocks_to_insert.push(block_data);
				}
				storage.insert_block_data_range(chain_id, blocks_to_insert.into_iter()).await;
				log::debug!("extracting block data {min}..{max}: complete");
				Ok(ComputeTask::Verify { min, max })
			},
			ComputeTask::Verify { min, max } => {
				log::debug!("verifying {min}..{max}");
				let blocks = storage.get_block_data_range(chain_id, (min..max).into_iter()).await;
				let blocks: Result<Vec<RawBlockData>, ComputeError> = (min..max)
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

				log::debug!("verifying {min}..{max}: complete");

				Ok(ComputeTask::FinalVerify { min, max })
			},
			ComputeTask::FinalVerify { min, max } => {
				log::debug!("final verify {min} and {max}");

				// Check min - only for genesis block
				if min == 0 {
					let block = storage
						.get_block_data(chain_id, 0)
						.await
						.ok_or(ComputeError::BlockMissing(0))?;
					if block.parent_hash != [0u8; 32] {
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

				log::debug!("final verify {min} and {max}: complete");
				Ok(ComputeTask::NoOp)
			},
			ComputeTask::NoOp => Ok(ComputeTask::NoOp),
		}
	}

	pub(crate) async fn extract_data(block: &FetchedBlock) -> Result<RawBlockData, ComputeError> {
		let header = block.block.block_header().await?;
		let spec_version = header
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
		match spec_version {
			RuntimeVersion::V0_21_0 => {
				Self::process_block_with_protocol::<MidnightMetadata0_21_0>(
					block,
					&header,
					spec_version,
				)
				.await
			},
			RuntimeVersion::V0_22_0 => {
				Self::process_block_with_protocol::<MidnightMetadata0_22_0>(
					block,
					&header,
					spec_version,
				)
				.await
			},
			RuntimeVersion::V1_0_0 => {
				Self::process_block_with_protocol::<MidnightMetadata1_0_0>(
					block,
					&header,
					spec_version,
				)
				.await
			},
			RuntimeVersion::V2_0_0 => {
				Self::process_block_with_protocol::<MidnightMetadata2_0_0>(
					block,
					&header,
					spec_version,
				)
				.await
			},
			RuntimeVersion::V2_1_0 => {
				Self::process_block_with_protocol::<MidnightMetadata2_1_0>(
					block,
					&header,
					spec_version,
				)
				.await
			},
		}
	}

	async fn process_block_with_protocol<M: MidnightMetadata>(
		block: &FetchedBlock,
		header: &<MidnightNodeClientConfig as subxt::Config>::Header,
		version: RuntimeVersion,
	) -> Result<RawBlockData, ComputeError> {
		let state_root = block.state_root.clone();
		let parent_block_hash = header.parent_hash;

		let mut timestamp_ms = None;
		let mut transactions = vec![];

		let block_number = block.block.block_number();

		// Decode extrinsics using the metadata from the ClientAtBlock, which
		// automatically resolves the correct schema for this block's spec version.
		let extrinsics = block.block.extrinsics().from_bytes(block.raw_body.clone()).await;

		let events = block
			.block
			.events()
			.fetch()
			.await
			.unwrap_or_else(|err| panic!("Error while fetching the events: {}", err));

		for ext in extrinsics.iter().filter_map(Result::ok) {
			let Ok(call) = ext.decode_call_data_as::<M::Call>() else {
				continue;
			};
			if let Some(ts) = M::timestamp_set(&call) {
				if timestamp_ms.is_some() {
					panic!("this block has two timestamps");
				}
				timestamp_ms = Some(ts);
			} else if let Some(bytes) = M::send_mn_transaction(&call) {
				transactions.push(RawTransaction::Midnight(bytes));
			} else if block_number == 0 {
				// Genesis block: extract system transactions from extrinsics directly
				// (genesis has no events since events are emitted during block execution)
				if let Some(bytes) = M::send_mn_system_transaction(&call) {
					transactions.push(RawTransaction::System(bytes));
				}
			}

			// For non-genesis blocks: extract system transactions from events.
			// This handles system transactions regardless of how they were triggered:
			// - Direct send_mn_system_transaction calls
			// - Governance-wrapped calls (FederatedAuthority::motion_dispatch)
			// - CNightObservation-triggered system transactions
			// - Any future wrapper patterns
			let ext_index = ext.index() as u32;
			for ev in events.iter().filter_map(Result::ok) {
				if ev.phase() != subxt::events::Phase::ApplyExtrinsic(ext_index) {
					continue;
				}
				if let Some(Ok(event)) = ev.decode_fields_as::<M::SystemTransactionAppliedEvent>() {
					let bytes = M::system_transaction_applied(event);
					transactions.push(RawTransaction::System(bytes));
				}
			}
		}

		let timestamp_ms = timestamp_ms.expect("failed to find a timestamp extrinsic in block");
		let tblock_secs = timestamp_ms / 1000;
		let hash = block.block.block_hash();
		let parent_hash = header.parent_hash;
		let number = block.block.block_number();
		Ok(RawBlockData {
			hash: hash.0,
			parent_hash: parent_hash.0,
			number,
			ledger_version: LedgerVersion::from_spec_version(version.to_spec_version())
				.ok_or_else(|| ComputeError::LedgerVersionMissing(number))?,
			transactions,
			tblock_secs,
			tblock_err: 30,
			parent_block_hash: parent_block_hash.0,
			last_block_time_secs: tblock_secs, // Fixed later in fetcher.rs
			state_root,
			state: block.state.clone(),
		})
	}
}

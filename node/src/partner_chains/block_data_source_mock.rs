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

use sidechain_domain::*;
use sp_timestamp::Timestamp;

pub struct BlockDataSourceMock {
	/// Duration of a mainchain epoch in milliseconds
	mc_epoch_duration_millis: u32,
}

impl BlockDataSourceMock {
	pub async fn get_latest_block_info(
		&self,
	) -> Result<MainchainBlock, Box<dyn std::error::Error + Send + Sync>> {
		Ok(self
			.get_latest_stable_block_for(Timestamp::new(BlockDataSourceMock::millis_now()))
			.await
			.unwrap()
			.unwrap())
	}

	pub async fn get_latest_stable_block_for(
		&self,
		reference_timestamp: Timestamp,
	) -> Result<Option<MainchainBlock>, Box<dyn std::error::Error + Send + Sync>> {
		let block_number = (reference_timestamp.as_millis() / 20000) as u32;
		let epoch = block_number / self.block_per_epoch();
		let mut hash_arr = [0u8; 32];
		hash_arr[..4].copy_from_slice(&block_number.to_be_bytes());
		Ok(Some(MainchainBlock {
			number: McBlockNumber(block_number),
			hash: McBlockHash(hash_arr),
			epoch: McEpochNumber(epoch),
			slot: McSlotNumber(block_number as u64),
			timestamp: reference_timestamp.as_millis(),
		}))
	}

	pub async fn get_stable_block_for(
		&self,
		_hash: McBlockHash,
		reference_timestamp: Timestamp,
	) -> Result<Option<MainchainBlock>, Box<dyn std::error::Error + Send + Sync>> {
		self.get_latest_stable_block_for(reference_timestamp).await
	}
}

impl BlockDataSourceMock {
	pub fn new(mc_epoch_duration_millis: u32) -> Self {
		Self { mc_epoch_duration_millis }
	}

	fn block_per_epoch(&self) -> u32 {
		self.mc_epoch_duration_millis / 20000
	}

	fn millis_now() -> u64 {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64
	}
}

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

#[cfg(feature = "std")]
use super::base_crypto_local::{hash::HashOutput, time::Timestamp};

use alloc::vec::Vec;
use scale_info::prelude::vec;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

/// A scale friendly version of mn_ledger::onchain_runtime::context::BlockContext
/// that can be used to pass across the host interface.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, Eq, PartialEq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockContext {
	pub tblock: u64,
	pub tblock_err: u32,
	pub parent_block_hash: Vec<u8>,
}

impl Default for BlockContext {
	fn default() -> Self {
		BlockContext { tblock: 0, tblock_err: 0, parent_block_hash: vec![0u8; 32] }
	}
}

#[cfg(feature = "std")]
impl From<super::onchain_runtime_local::context::BlockContext> for BlockContext {
	fn from(value: super::onchain_runtime_local::context::BlockContext) -> Self {
		Self {
			tblock: value.tblock.to_secs(),
			tblock_err: value.tblock_err,
			parent_block_hash: value.parent_block_hash.0.to_vec(),
		}
	}
}

#[cfg(feature = "std")]
impl TryFrom<BlockContext> for super::onchain_runtime_local::context::BlockContext {
	type Error = Vec<u8>;

	fn try_from(value: BlockContext) -> Result<Self, Self::Error> {
		let BlockContext { tblock, tblock_err, parent_block_hash } = value;

		let parent_block_hash: [u8; 32] = parent_block_hash.try_into()?;

		Ok(Self {
			tblock: Timestamp::from_secs(tblock),
			tblock_err,
			parent_block_hash: HashOutput(parent_block_hash),
		})
	}
}

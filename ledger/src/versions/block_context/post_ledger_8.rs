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
	pub last_block_time: u64,
}

impl Default for BlockContext {
	fn default() -> Self {
		BlockContext {
			tblock: 0,
			tblock_err: 0,
			parent_block_hash: vec![0u8; 32],
			last_block_time: 0,
		}
	}
}

#[cfg(feature = "std")]
impl From<super::onchain_runtime_local::context::BlockContext> for BlockContext {
	fn from(value: super::onchain_runtime_local::context::BlockContext) -> Self {
		Self {
			tblock: value.tblock.to_secs(),
			tblock_err: value.tblock_err,
			parent_block_hash: value.parent_block_hash.0.to_vec(),
			last_block_time: value.last_block_time.to_secs(),
		}
	}
}

#[cfg(feature = "std")]
impl TryFrom<BlockContext> for super::onchain_runtime_local::context::BlockContext {
	type Error = Vec<u8>;

	fn try_from(value: BlockContext) -> Result<Self, Self::Error> {
		let BlockContext { tblock, tblock_err, parent_block_hash, last_block_time } = value;

		let parent_block_hash: [u8; 32] = parent_block_hash.try_into()?;

		Ok(Self {
			tblock: Timestamp::from_secs(tblock),
			tblock_err,
			parent_block_hash: HashOutput(parent_block_hash),
			last_block_time: Timestamp::from_secs(last_block_time),
		})
	}
}

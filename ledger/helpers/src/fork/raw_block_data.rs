// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::ledger_8::BlockContext;
use serde::{Deserialize, Serialize};

/// Hex for human-readable formats (JSON), raw bytes for binary (postcard).
mod hex_or_bytes {
	use serde::{Deserializer, Serializer};

	pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
		if s.is_human_readable() {
			hex::serde::serialize(bytes, s)
		} else {
			serde_bytes::serialize(bytes, s)
		}
	}

	pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
		if d.is_human_readable() { hex::serde::deserialize(d) } else { serde_bytes::deserialize(d) }
	}
}

/// Same as `hex_or_bytes` but for fixed-size `[u8; 32]`.
mod hex_or_bytes_32 {
	use serde::{Deserializer, Serializer, de::Error};

	pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
		if s.is_human_readable() {
			hex::serde::serialize(bytes, s)
		} else {
			serde_bytes::serialize(bytes.as_slice(), s)
		}
	}

	pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
		if d.is_human_readable() {
			hex::serde::deserialize(d)
		} else {
			let bytes: &[u8] = serde_bytes::deserialize(d)?;
			bytes.try_into().map_err(|_| D::Error::custom("expected 32 bytes"))
		}
	}
}

/// Which ledger version a block was produced under.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerVersion {
	Ledger7,
	#[default]
	Ledger8,
}

/// A transaction stored as raw bytes, before version-specific deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawTransaction {
	/// Raw bytes from `send_mn_transaction` extrinsic
	Midnight(#[serde(with = "hex_or_bytes")] Vec<u8>),
	/// Raw bytes from system transaction events / extrinsics
	System(#[serde(with = "hex_or_bytes")] Vec<u8>),
}

impl RawTransaction {
	pub fn as_bytes(&self) -> &[u8] {
		match self {
			RawTransaction::Midnight(tx) => tx,
			RawTransaction::System(tx) => tx,
		}
	}
}

/// Version-agnostic block data that stores transactions as raw serialized bytes.
///
/// Deserialization into version-specific ledger types happens lazily in
/// `apply_block_7` / `apply_block_8`, which use the correct types for
/// the respective ledger version.
///
/// The `spec_version` field stores the raw runtime spec version number.
/// Use `LedgerVersion::from_spec_version()` to convert at point of use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawBlockData {
	pub hash: [u8; 32],
	pub parent_hash: [u8; 32],
	pub number: u64,
	pub ledger_version: LedgerVersion,
	pub transactions: Vec<RawTransaction>,
	/// Block timestamp in seconds
	pub tblock_secs: u64,
	/// Timestamp error margin (always 30)
	pub tblock_err: u32,
	/// Parent block hash (from block header)
	/// TODO: Remove this?! Duplicate of parent_hash
	pub parent_block_hash: [u8; 32],
	/// Previous block's timestamp in seconds (fixed up after fetch)
	pub last_block_time_secs: u64,
	/// State root (for verification)
	pub state_root: Option<Vec<u8>>,
	/// Genesis state bytes (only present for block 0)
	pub state: Option<Vec<u8>>,
}

impl LedgerVersion {
	/// Convert a raw spec version to a `LedgerVersion`.
	///
	/// Versions up to 0.21.x use Ledger7, version 0.22.0+ uses Ledger8.
	pub fn from_spec_version(spec_version: u32) -> Option<Self> {
		match spec_version {
			#[allow(clippy::zero_prefixed_literal)]
			000_017_000..=000_021_999 => Some(LedgerVersion::Ledger7),
			#[allow(clippy::zero_prefixed_literal)]
			000_022_000.. => Some(LedgerVersion::Ledger8),
			_ => None,
		}
	}
}

impl RawBlockData {
	/// Construct a new block with a timestamp
	pub fn new_from_timestamp(
		timestamp_s: u64,
		ledger_version: LedgerVersion,
		transactions: Vec<RawTransaction>,
	) -> RawBlockData {
		RawBlockData {
			hash: [0u8; 32],
			parent_hash: [0u8; 32],
			number: 0,
			ledger_version,
			transactions,
			tblock_secs: timestamp_s,
			tblock_err: 30,
			parent_block_hash: [0u8; 32],
			last_block_time_secs: 0,
			state_root: None,
			state: None,
		}
	}

	/// Get the ledger version for this block.
	pub fn ledger_version(&self) -> LedgerVersion {
		self.ledger_version
	}
}

/// A single serialized transaction ready for sending or file output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedTx {
	/// Serialized `Transaction` — the payload for `send_mn_transaction`.
	pub tx: RawTransaction,
	/// Serialized `BlockContext`
	pub context: BlockContext,
	/// Transaction hash for logging.
	#[serde(with = "hex_or_bytes_32")]
	pub tx_hash: [u8; 32],
}

impl SerializedTx {
	pub fn tx_byte_len(&self) -> usize {
		match &self.tx {
			RawTransaction::Midnight(tx) => tx.len(),
			RawTransaction::System(tx) => tx.len(),
		}
	}
}

/// Output of a builder — serialized transactions ready for sending.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedTxBatches {
	pub batches: Vec<Vec<SerializedTx>>,
}

impl SerializedTxBatches {
	pub fn get_context(batch: &[SerializedTx]) -> Result<BlockContext, String> {
		let mut context: Option<BlockContext> = None;
		for tx in batch {
			if let Some(ref context) = context {
				if context.tblock != tx.context.tblock {
					return Err(format!(
						"Internal error: Txs in the same batch have mismatched context: {context:?} != {:?}",
						tx.context
					));
				}
			} else {
				context = Some(tx.context.clone());
			}
		}

		context.ok_or("batch is empty, block context not found".to_string())
	}
}

#[cfg(feature = "can-panic")]
impl TryFrom<&SerializedTxBatches> for Vec<RawBlockData> {
	type Error = String;

	fn try_from(value: &SerializedTxBatches) -> Result<Self, Self::Error> {
		let mut blocks = Vec::new();
		let mut ledger_version = LedgerVersion::default();

		for batch in &value.batches {
			let context = SerializedTxBatches::get_context(batch)?;
			let transactions: Vec<_> = batch.iter().map(|t| t.tx.clone()).collect();

			if let Some((_, v)) = transactions
				.iter()
				.filter_map(|tx| {
					crate::fork::network_id_and_ledger_version_from_tx_bytes(tx.as_bytes()).ok()
				})
				.next()
			{
				ledger_version = v;
			}

			blocks.push(RawBlockData::new_from_timestamp(
				context.tblock.to_secs(),
				ledger_version,
				transactions,
			));
		}

		Ok(blocks)
	}
}

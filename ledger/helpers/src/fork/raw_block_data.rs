// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Which ledger version a block was produced under.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerVersion {
	Ledger7,
	#[default]
	Ledger8,
}

/// A transaction stored as raw bytes, before version-specific deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RawTransaction {
	/// Raw bytes from `send_mn_transaction` extrinsic
	Midnight(Vec<u8>),
	/// Raw bytes from system transaction events / extrinsics
	System(Vec<u8>),
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
/// `ForkAwareLedgerContext::update_from_block`, which knows the current
/// ledger version and uses the correct types.
///
/// The `spec_version` field stores the raw runtime spec version number.
/// Use `LedgerVersion::from_spec_version()` to convert at point of use.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
	pub parent_block_hash: [u8; 32],
	/// Previous block's timestamp in seconds (fixed up after fetch)
	pub last_block_time_secs: u64,
	/// State root (for verification)
	pub state_root: Option<Vec<u8>>,
	/// Genesis state bytes (only present for block 0)
	pub state: Option<Vec<u8>>,
}

impl PartialOrd for RawBlockData {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for RawBlockData {
	fn cmp(&self, other: &Self) -> Ordering {
		self.tblock_secs.cmp(&other.tblock_secs)
	}
}

impl PartialEq for RawBlockData {
	fn eq(&self, other: &Self) -> bool {
		self.tblock_secs == other.tblock_secs
	}
}

impl Eq for RawBlockData {}

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

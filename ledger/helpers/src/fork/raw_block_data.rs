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

use crate::ledger_9::BlockContext;
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
	Ledger8,
	#[default]
	Ledger9,
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
	/// Versions up to 0.21.x use Ledger7, 0.22.0..=1.x.y use Ledger8, 2.0.0+ uses Ledger9.
	pub fn from_spec_version(spec_version: u32) -> Option<Self> {
		match spec_version {
			#[allow(clippy::zero_prefixed_literal)]
			000_017_000..=000_021_999 => Some(LedgerVersion::Ledger7),
			#[allow(clippy::zero_prefixed_literal)]
			000_022_000..=001_999_999 => Some(LedgerVersion::Ledger8),
			#[allow(clippy::zero_prefixed_literal)]
			002_000_000.. => Some(LedgerVersion::Ledger9),
			_ => None,
		}
	}

	/// Determine a block's ledger version from its recorded state root.
	///
	/// The root is a `tagged_serialize`d `TypedArenaKey<Ledger>` whose tag embeds
	/// the `LedgerState` version (`storage-key(midnight:ledger-state[vN]:…)`), so
	/// it reports the version of the state the block actually left behind.
	///
	/// This is *more* authoritative than the block's runtime spec version across
	/// the ledger 8 -> 9 hardfork: the state translation is a multi-block
	/// migration serviced after each block's inherents, so the first block (or
	/// blocks) executed by the ledger-9 runtime still carry a ledger-8 state root.
	/// Partitioning a replay by spec version alone would hand those blocks to the
	/// ledger-9 context and fail its state-root check.
	///
	/// Returns `None` if the tag is unreadable or unrecognised.
	pub fn from_state_root(state_root: &[u8]) -> Option<Self> {
		use crate::ledger_9::ledger_storage::{arena::TypedArenaKey, db::DB, db::InMemoryDB as Db};
		use midnight_serialize::{Tagged, peek_tag};

		// The node wraps `LedgerState` in its own `Ledger` type, whose `Tagged`
		// impl forwards to `LedgerState`'s, so the two produce the same key tag.
		// The hasher parameter does not appear in the tag.
		fn key_tag<T: Tagged>() -> String {
			<TypedArenaKey<T, <Db as DB>::Hasher> as Tagged>::tag().into_owned()
		}

		let tag = peek_tag(&mut std::io::Cursor::new(state_root)).ok()?;
		// Compared against the live types' tags rather than string literals, so a
		// tag change upstream surfaces as an unknown version rather than a silent
		// mis-dispatch.
		if tag == key_tag::<crate::ledger_9::mn_ledger::structure::LedgerState<Db>>() {
			Some(LedgerVersion::Ledger9)
		} else if tag == key_tag::<crate::ledger_8::mn_ledger::structure::LedgerState<Db>>() {
			Some(LedgerVersion::Ledger8)
		} else {
			None
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

	/// The ledger version to replay this block under.
	///
	/// Prefers the version implied by the recorded state root, falling back to the
	/// runtime spec version recorded at fetch time. See
	/// [`LedgerVersion::from_state_root`] for why the state root wins: across the
	/// ledger 8 -> 9 hardfork the state translation is a multi-block migration, so
	/// the first block(s) under the ledger-9 runtime still leave a ledger-8 state
	/// root behind.
	pub fn ledger_version(&self) -> LedgerVersion {
		self.state_root
			.as_deref()
			.and_then(LedgerVersion::from_state_root)
			.unwrap_or(self.ledger_version)
	}

	/// Whether this block left the ledger state untouched, i.e. its recorded state
	/// root is identical to `previous_state_root`.
	///
	/// True for the blocks the ledger 8 -> 9 migration spans: `pallet_midnight`'s
	/// `on_finalize` skips the post-block ledger update while the translation is in
	/// flight, and `frame_executive` admits only inherents in those blocks, so
	/// nothing moves the state. A replay must skip them wholesale — applying a
	/// post-block update would advance its own state past the chain's.
	pub fn leaves_ledger_unchanged(&self, previous_state_root: Option<&Vec<u8>>) -> bool {
		match (self.state_root.as_ref(), previous_state_root) {
			(Some(root), Some(previous)) => root == previous,
			_ => false,
		}
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

#[cfg(test)]
mod tests {
	use super::*;

	/// Real `pallet_midnight::StateKey` values observed on a dev chain across the
	/// ledger 8 -> 9 hardfork.
	const V8_ROOT_HEX: &str = "6d69646e696768743a73746f726167652d6b6579286c65646765722d73746174655b7631335d293a000bb99ebc32c6ec259251c930a09f1c63ece3e8197cb9ce8b8d4246dd26914182";
	const V9_ROOT_HEX: &str = "6d69646e696768743a73746f726167652d6b6579286c65646765722d73746174655b7631385d293a00620d5ce98a447d2429b8cbb48ee9908af933023a256590b97f710d12a89f4edb";

	fn block(number: u64, spec_version: LedgerVersion, root: Option<&str>) -> RawBlockData {
		RawBlockData {
			number,
			ledger_version: spec_version,
			state_root: root.map(|hex| hex::decode(hex).unwrap()),
			..RawBlockData::new_from_timestamp(0, spec_version, Vec::new())
		}
	}

	#[test]
	fn ledger_version_from_state_root_tag() {
		let v8 = hex::decode(V8_ROOT_HEX).unwrap();
		let v9 = hex::decode(V9_ROOT_HEX).unwrap();
		assert_eq!(LedgerVersion::from_state_root(&v8), Some(LedgerVersion::Ledger8));
		assert_eq!(LedgerVersion::from_state_root(&v9), Some(LedgerVersion::Ledger9));
		assert_eq!(LedgerVersion::from_state_root(b"not a tagged root"), None);
	}

	/// The migration window: the ledger-9 runtime is live (spec version says
	/// `Ledger9`) but the state has not been translated yet. The replay must go by
	/// the state root, or it hands the block to a ledger-9 context that cannot
	/// reproduce a ledger-8 root.
	#[test]
	fn state_root_wins_over_spec_version() {
		assert_eq!(
			block(35, LedgerVersion::Ledger9, Some(V8_ROOT_HEX)).ledger_version(),
			LedgerVersion::Ledger8,
		);
		assert_eq!(
			block(36, LedgerVersion::Ledger9, Some(V9_ROOT_HEX)).ledger_version(),
			LedgerVersion::Ledger9,
		);
	}

	/// With no usable state root there is nothing better than the spec version.
	#[test]
	fn spec_version_is_the_fallback() {
		assert_eq!(block(1, LedgerVersion::Ledger8, None).ledger_version(), LedgerVersion::Ledger8);
		assert_eq!(block(1, LedgerVersion::Ledger7, None).ledger_version(), LedgerVersion::Ledger7);
	}

	#[test]
	fn leaves_ledger_unchanged_detects_a_paused_block() {
		let v8_root = hex::decode(V8_ROOT_HEX).unwrap();
		let v9_root = hex::decode(V9_ROOT_HEX).unwrap();
		let paused = block(35, LedgerVersion::Ledger9, Some(V8_ROOT_HEX));

		assert!(paused.leaves_ledger_unchanged(Some(&v8_root)), "same root as parent");
		assert!(!paused.leaves_ledger_unchanged(Some(&v9_root)), "different root from parent");
		assert!(!paused.leaves_ledger_unchanged(None), "no parent root to compare against");
		assert!(
			!block(35, LedgerVersion::Ledger9, None).leaves_ledger_unchanged(Some(&v8_root)),
			"no root of its own to compare",
		);
	}
}

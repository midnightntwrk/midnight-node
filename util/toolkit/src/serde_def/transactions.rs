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
	LedgerVersion, RawBlockData, RawTransaction, SerializedTx, SerializedTxBatches,
};
use midnight_node_ledger_helpers::*;
use std::{
	fmt::Debug,
	time::{SystemTime, UNIX_EPOCH},
};

/// Source transactions loaded from either the network or files.
///
/// Stores blocks as version-agnostic [`RawBlockData`] with raw serialized transaction bytes.
/// Deserialization of transactions happens lazily when building the ledger context.
#[derive(Clone, Debug)]
pub struct SourceTransactions {
	pub blocks: Vec<RawBlockData>,
	pub network_id: String,
}

impl SourceTransactions {
	/// Create a new SourceTransactions with pre-computed network_id.
	pub fn new(blocks: Vec<RawBlockData>, network_id: &str) -> Self {
		Self { blocks, network_id: network_id.to_string() }
	}

	/// Convert untyped transactions (from file loading) into RawBlockData.
	pub fn from_blocks(
		blocks: impl IntoIterator<Item = RawBlockData>,
		dust_warp: bool,
		network_id: Option<String>,
	) -> Self {
		let mut blocks: Vec<_> = blocks.into_iter().collect();
		if dust_warp {
			let now_secs = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("time has run backwards")
				.as_secs();
			blocks.push(RawBlockData::new_from_timestamp(
				now_secs,
				blocks.get(0).map(|b| b.ledger_version).unwrap_or_default(),
				Default::default(),
			));
		}

		let Some(_) = blocks.first() else {
			panic!("block list is empty");
		};

		// If the source has not figured out network id yet, then look for it in the blocks
		let network_id = network_id.unwrap_or_else(|| {
			for block in blocks.iter() {
				if let Some((network_id, _ledger_version)) = block
					.transactions
					.iter()
					.filter_map(|tx| {
						fork::network_id_and_ledger_version_from_tx_bytes(tx.as_bytes()).ok()
					})
					.next()
				{
					return network_id;
				}
			}
			panic!("Could not find transaction with 'network id' in given blocks");
		});

		Self { blocks, network_id }
	}

	/// Convert untyped transactions (from file loading) into RawBlockData.
	pub fn from_batches(
		batches: impl IntoIterator<Item = Vec<SerializedTx>>,
		dust_warp: bool,
		network_id: Option<String>,
	) -> Self {
		let mut blocks = Vec::new();
		let mut ledger_version = LedgerVersion::default();
		let mut network_id: Option<String> = network_id;
		for batch in batches {
			let context =
				SerializedTxBatches::get_context(&batch).expect("failed to get context for batch");
			let transactions: Vec<_> = batch.iter().map(|t| t.tx.clone()).collect();

			if let Some((new_network_id, new_ledger_version)) = transactions
				.iter()
				.filter_map(|tx| {
					fork::network_id_and_ledger_version_from_tx_bytes(tx.as_bytes()).ok()
				})
				.next()
			{
				ledger_version = new_ledger_version;
				network_id = Some(new_network_id);
			};

			let block = RawBlockData::new_from_timestamp(
				context.tblock.to_secs(),
				ledger_version,
				transactions,
			);
			blocks.push(block);
		}

		// Sort the blocks + set last block time
		blocks.sort_by_key(|b| b.tblock_secs);

		for i in 0..blocks.len() {
			// Set last_block_time for all blocks apart from genesis
			if i >= 1 {
				blocks[i].last_block_time_secs = blocks[i - 1].tblock_secs;
			}
		}

		Self::from_blocks(blocks, dust_warp, network_id)
	}

	/// Convert untyped transactions (from file loading) into RawBlockData.
	pub fn from_txs(
		txs: impl IntoIterator<Item = SerializedTx>,
		network_id: Option<String>,
	) -> Self {
		let now_secs = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("time has run backwards")
			.as_secs();

		let mut transactions = Vec::new();
		let mut network_id: Option<String> = network_id;
		let mut ledger_version: LedgerVersion = LedgerVersion::default();
		for tx in txs {
			if network_id.is_none()
				&& let SerializedTx { tx: RawTransaction::Midnight(ref tx), .. } = tx
			{
				let (new_network_id, new_ledger_version) =
					fork::network_id_and_ledger_version_from_tx_bytes(tx).unwrap();
				network_id = Some(new_network_id);
				ledger_version = new_ledger_version;
			}
			transactions.push(tx.tx);
		}
		let block = RawBlockData::new_from_timestamp(now_secs, ledger_version, transactions);
		let network_id = network_id
			.unwrap_or_else(|| panic!("Network id has not been given nor found in transactions"));
		Self { blocks: vec![block], network_id }
	}

	/// Derive a deterministic chain identity for wallet state cache keying.
	///
	/// Returns `None` when no block #1 is available (e.g. file-loaded datasets),
	/// which signals the caller to skip caching and avoid cross-dataset collisions.
	pub fn chain_id(&self) -> Option<subxt::utils::H256> {
		self.blocks
			.iter()
			.find(|b| b.number == 1)
			.map(|b| subxt::utils::H256::from(b.hash))
	}

	pub fn ledger_version(&self) -> LedgerVersion {
		self.blocks
			.first()
			.map(|b| b.ledger_version())
			.unwrap_or(LedgerVersion::default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Construct a minimal `RawBlockData` at a given height for tests.
	fn block_at(number: u64) -> RawBlockData {
		RawBlockData {
			hash: [0u8; 32],
			parent_hash: [0u8; 32],
			number,
			ledger_version: LedgerVersion::default(),
			transactions: Vec::new(),
			tblock_secs: number, // deterministic, non-zero past block 0
			tblock_err: 30,
			parent_block_hash: [0u8; 32],
			last_block_time_secs: number.saturating_sub(1),
			state_root: None,
			state: None,
		}
	}

	/// `from_blocks(_, dust_warp = true, _)` appends a synthetic
	/// timestamp-only block with `number = 0` to the end of `blocks`. This
	/// is the invariant that the cache-save logic in
	/// `tx_generator::builder::build_fork_aware_context_cached` relies on:
	/// it must compute the save height via `max_by_key(|b| b.number)`
	/// rather than `blocks.last()`, otherwise the cache is tagged with
	/// `block_height = 0` and subsequent runs panic on non-linear dust-tree
	/// insertion when they reload the snapshot.
	#[test]
	fn from_blocks_with_dust_warp_appends_synthetic_block_at_number_zero() {
		let real_blocks = vec![block_at(1), block_at(2), block_at(3)];
		let src = SourceTransactions::from_blocks(
			real_blocks.clone(),
			/* dust_warp = */ true,
			Some("test".to_string()),
		);

		assert_eq!(src.blocks.len(), real_blocks.len() + 1, "synthetic block appended");
		assert_eq!(src.blocks.last().unwrap().number, 0, "synthetic block is at number = 0");

		let max_by_number = src.blocks.iter().max_by_key(|b| b.number).expect("non-empty");
		assert_eq!(
			max_by_number.number, 3,
			"max_by_number must pick the highest real block, not the synthetic"
		);

		// And without dust_warp, last() and max_by_number agree.
		let src_no_warp = SourceTransactions::from_blocks(
			real_blocks,
			/* dust_warp = */ false,
			Some("test".to_string()),
		);
		assert_eq!(src_no_warp.blocks.last().unwrap().number, 3);
		assert_eq!(src_no_warp.blocks.iter().max_by_key(|b| b.number).unwrap().number, 3,);
	}
}

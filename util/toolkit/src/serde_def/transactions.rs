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
	pub fn from_blocks(blocks: impl IntoIterator<Item = RawBlockData>, dust_warp: bool) -> Self {
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

		let Some(block) = blocks.first() else {
			panic!("block list is empty");
		};

		let network_id_res = block
			.transactions
			.iter()
			.filter_map(|tx| fork::network_id_and_ledger_version_from_tx_bytes(tx.as_bytes()).ok())
			.next();

		let Some((network_id, _)) = network_id_res else {
			panic!("first block has no transactions that include a network id");
		};

		Self { blocks, network_id }
	}

	/// Convert untyped transactions (from file loading) into RawBlockData.
	pub fn from_batches(
		batches: impl IntoIterator<Item = Vec<SerializedTx>>,
		dust_warp: bool,
	) -> Self {
		let mut blocks = Vec::new();
		let mut ledger_version = LedgerVersion::default();
		for batch in batches {
			let context =
				SerializedTxBatches::get_context(&batch).expect("failed to get context for batch");
			let transactions: Vec<_> = batch.iter().map(|t| t.tx.clone()).collect();

			if let Some((_, new_ledger_version)) = transactions
				.iter()
				.filter_map(|tx| {
					fork::network_id_and_ledger_version_from_tx_bytes(tx.as_bytes()).ok()
				})
				.next()
			{
				ledger_version = new_ledger_version;
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

		Self::from_blocks(blocks, dust_warp)
	}

	/// Convert untyped transactions (from file loading) into RawBlockData.
	pub fn from_txs(txs: impl IntoIterator<Item = SerializedTx>) -> Self {
		let now_secs = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("time has run backwards")
			.as_secs();

		let mut transactions = Vec::new();
		let mut network_id: Option<String> = None;
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

		let network_id = network_id.expect("no transactions found, can't derive network id");
		Self { blocks: vec![block], network_id }
	}
}

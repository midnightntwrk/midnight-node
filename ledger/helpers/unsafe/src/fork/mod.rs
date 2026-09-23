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

use midnight_node_ledger_helpers::fork::raw_block_data::{
	LedgerVersion, RawBlockData, SerializedTxBatches,
};

pub mod fork_8_to_9;
pub mod fork_aware_context;

pub fn network_id_and_ledger_version_from_tx_bytes(
	tx_bytes: &[u8],
) -> Result<(String, LedgerVersion), std::io::Error> {
	let res9 = crate::ledger_9::network_id_from_transaction_bytes(tx_bytes);
	if let Ok(ref network_id) = res9 {
		return Ok((network_id.to_string(), LedgerVersion::Ledger9));
	}

	let network_id = crate::ledger_8::network_id_from_transaction_bytes(tx_bytes)?;
	Ok((network_id.to_string(), LedgerVersion::Ledger8))
}

/// Builds the display-friendly `RawBlockData` view of a `SerializedTxBatches` warp
/// transfer, one entry per block. Lives here (not as a `TryFrom` impl alongside the
/// type in `midnight-node-ledger-helpers`) because it dispatches through
/// [`network_id_and_ledger_version_from_tx_bytes`], which is toolkit-only.
pub fn raw_block_data_from_batches(
	value: &SerializedTxBatches,
) -> Result<Vec<RawBlockData>, String> {
	let mut blocks = Vec::new();
	let mut ledger_version = LedgerVersion::default();

	for batch in &value.batches {
		let context = SerializedTxBatches::get_context(batch)?;
		let transactions: Vec<_> = batch.iter().map(|t| t.tx.clone()).collect();

		if let Some((_, v)) = transactions
			.iter()
			.filter_map(|tx| network_id_and_ledger_version_from_tx_bytes(tx.as_bytes()).ok())
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

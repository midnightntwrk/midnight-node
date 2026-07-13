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

use super::ledger_helpers_local::{
	DefaultDB, HashOutput, ProofMarker, SerdeTransaction, Signature, Timestamp,
	TransactionWithContext, make_block_context, mn_ledger_serialize::tagged_deserialize,
};
use async_trait::async_trait;
use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;
use std::convert::Infallible;

use crate::{serde_def::SourceTransactions, tx_generator::builder::BuildTxs};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

pub struct DoNothingBuilder;

impl DoNothingBuilder {
	pub fn new() -> Self {
		Self
	}
}

#[async_trait]
impl BuildTxs for DoNothingBuilder {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		// Deserialize all raw blocks into typed transactions
		let mut all_txs: Vec<TransactionWithContext<Signature, ProofMarker, DefaultDB>> =
			Vec::new();
		for block in &received_tx.blocks {
			let block_context = make_block_context(
				Timestamp::from_secs(block.tblock_secs),
				HashOutput(block.parent_block_hash),
				Timestamp::from_secs(block.last_block_time_secs),
			);
			for raw_tx in &block.transactions {
				let serde_tx = match raw_tx {
					RawTransaction::Midnight(bytes) => {
						let tx = tagged_deserialize(bytes.as_slice())
							.expect("failed to deserialize midnight transaction");
						SerdeTransaction::Midnight(tx)
					},
					RawTransaction::System(bytes) => {
						let tx = tagged_deserialize(bytes.as_slice())
							.expect("failed to deserialize system transaction");
						SerdeTransaction::System(tx)
					},
				};
				all_txs.push(TransactionWithContext {
					tx: serde_tx,
					block_context: block_context.clone(),
				});
			}
		}

		let batches = if all_txs.is_empty() { vec![] } else { vec![all_txs] };

		Ok(super::tx_serialization::build_batched(batches))
	}
}

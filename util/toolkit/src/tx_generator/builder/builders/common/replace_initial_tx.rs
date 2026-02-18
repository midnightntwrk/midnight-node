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

use async_trait::async_trait;
use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;
use super::ledger_helpers_local::{
	DefaultDB, HashOutput, LedgerContext, ProofProvider, SerdeTransaction, Timestamp,
	TransactionWithContext, make_block_context, mn_ledger_serialize::tagged_deserialize,
};
use std::sync::Arc;
use thiserror::Error;

use crate::{
	ProofType, SignatureType,
	serde_def::{
		BuiltTransactions, DeserializedTransactionsWithContext,
		DeserializedTransactionsWithContextBatch, SourceTransactions,
	},
	tx_generator::builder::BuildTxs,
};

pub struct ReplaceInitialTxBuilder;

impl ReplaceInitialTxBuilder {
	pub fn new() -> Self {
		Self
	}
}

#[derive(Error, Debug)]
#[error("error building ReplaceInitialTx: {0}")]
pub struct ReplaceInitialTxError(String);

#[async_trait]
impl BuildTxs for ReplaceInitialTxBuilder {
	type Error = ReplaceInitialTxError;

	async fn build_txs_from(
		&self,
		mut received_tx: SourceTransactions,
		_context: Option<Arc<LedgerContext<DefaultDB>>>,
		_prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<BuiltTransactions, Self::Error> {
		// Skip the first block (genesis) and start from the second
		received_tx.blocks.remove(0);

		// Deserialize all remaining raw blocks into typed transactions
		let mut all_txs: Vec<TransactionWithContext<SignatureType, ProofType, DefaultDB>> =
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

		if all_txs.is_empty() {
			return Err(ReplaceInitialTxError("No batches available to migrate".to_string()));
		}

		let initial_tx = all_txs.remove(0);
		let batch = DeserializedTransactionsWithContextBatch { txs: all_txs };

		let typed = DeserializedTransactionsWithContext {
			initial_tx,
			batches: if batch.txs.is_empty() { vec![] } else { vec![batch] },
		};

		Ok(BuiltTransactions::from_typed(typed))
	}
}

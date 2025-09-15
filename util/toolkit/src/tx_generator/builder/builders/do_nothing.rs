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
use midnight_node_ledger_helpers::TransactionWithContext;
use std::{convert::Infallible, sync::Arc};

use crate::{
	builder::{
		BuildTxs, DefaultDB, DeserializedTransactionsWithContext, ProofProvider, ProofType,
		SignatureType,
	},
	serde_def::{DeserializedTransactionsWithContextBatch, SourceTransactions},
};

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
		mut received_tx: SourceTransactions<SignatureType, ProofType>,
		_prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		let initial_block = received_tx.blocks.first_mut().unwrap();
		let initial_tx = TransactionWithContext {
			tx: initial_block.transactions.remove(0),
			block_context: initial_block.context.clone(),
		};

		let batches = received_tx
			.blocks
			.into_iter()
			.map(|block| {
				let txs = block
					.transactions
					.into_iter()
					.map(|tx| TransactionWithContext { tx, block_context: block.context.clone() })
					.collect();
				DeserializedTransactionsWithContextBatch { txs }
			})
			.collect();

		Ok(DeserializedTransactionsWithContext { initial_tx, batches })
	}
}

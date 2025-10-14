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

use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use midnight_node_ledger_helpers::*;

#[derive(Clone, Debug)]
pub struct SourceTransactions<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub blocks: Vec<SourceBlockTransactions<S, P>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>> SourceTransactions<S, P>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub fn from_txs_with_context(
		txs: impl IntoIterator<Item = TransactionWithContext<S, P, DefaultDB>>,
	) -> Self {
		let mut blocks = vec![];
		let mut current_batch = vec![];
		let mut last_context: Option<BlockContext> = None;
		for tx in txs {
			if last_context.as_ref().is_some_and(|c| c.tblock != tx.block_context.tblock) {
				blocks.push(SourceBlockTransactions {
					transactions: std::mem::take(&mut current_batch),
					context: last_context.unwrap(),
					state_root: None,
				});
			}
			current_batch.push(tx.tx);
			last_context = Some(tx.block_context);
		}
		if let Some(context) = last_context {
			blocks.push(SourceBlockTransactions {
				transactions: current_batch,
				context,
				state_root: None,
			});
		}
		Self { blocks }
	}

	pub fn network(&self) -> &str {
		self.blocks
			.iter()
			.find_map(|b| b.transactions.iter().find_map(|tx| tx.network_id()))
			.expect("no transaction in this batch had a network")
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceBlockTransactions<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	#[serde(bound = "")]
	pub transactions: Vec<SerdeTransaction<S, P, DefaultDB>>,
	pub context: BlockContext,
	#[serde(default)]
	pub state_root: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct DeserializedTransactionsWithContextBatch<
	S: SignatureKind<DefaultDB>,
	P: ProofKind<DefaultDB>,
> where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub txs: Vec<TransactionWithContext<S, P, DefaultDB>>,
}

#[derive(Debug, Clone)]
pub struct DeserializedTransactionsWithContext<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>>
where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub initial_tx: TransactionWithContext<S, P, DefaultDB>,
	pub batches: Vec<DeserializedTransactionsWithContextBatch<S, P>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>
	DeserializedTransactionsWithContext<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub fn flat(self) -> Vec<TransactionWithContext<S, P, DefaultDB>> {
		let mut result =
			Vec::with_capacity(1 + self.batches.iter().map(|b| b.txs.len()).sum::<usize>());
		result.push(self.initial_tx); // Add initial_tx first
		for batch in self.batches {
			result.extend(batch.txs); // Append each batch's transactions
		}
		result
	}

	pub fn network(&self) -> &str {
		self.initial_tx
			.tx
			.network_id()
			.or_else(|| {
				self.batches.iter().find_map(|b| b.txs.iter().find_map(|t| t.tx.network_id()))
			})
			.expect("no transaction in this batch had a network")
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SerializedTransactionsWithContextBatch {
	pub txs: Vec<String>,
}

impl SerializedTransactionsWithContextBatch {
	fn new<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>(
		batch_txs: &[TransactionWithContext<S, P, DefaultDB>],
	) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
	where
		<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
		Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
	{
		let txs = batch_txs
			.iter()
			.map(|tx_with_context| {
				// Serialize TransactionWithContext to a string
				serde_json::to_string(&tx_with_context).map_err(|e| Box::new(e))
			})
			.collect::<Result<Vec<String>, Box<_>>>()?;

		Ok(SerializedTransactionsWithContextBatch { txs })
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SerializedTransactionsWithContext {
	pub initial_tx: String,
	pub batches: Vec<SerializedTransactionsWithContextBatch>,
}

impl SerializedTransactionsWithContext {
	pub fn new<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>(
		txs: &DeserializedTransactionsWithContext<S, P>,
	) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
	where
		<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
		Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
	{
		// Serialize initial_tx
		let initial_tx = serde_json::to_string(&txs.initial_tx).map_err(|e| Box::new(e))?;

		// Serialize each batch
		let batches = txs
			.batches
			.iter()
			.map(|batch| SerializedTransactionsWithContextBatch::new(&batch.txs))
			.collect::<Result<Vec<_>, Box<_>>>()?;

		Ok(SerializedTransactionsWithContext { initial_tx, batches })
	}
}

pub(crate) mod block_vec {
	use super::*;
	use serde::{Deserialize, Deserializer, Serialize, Serializer};

	pub(crate) fn serialize<SE, S, P>(
		txes: &Vec<SourceBlockTransactions<S, P>>,
		s: SE,
	) -> Result<SE::Ok, SE::Error>
	where
		SE: Serializer,
		S: SignatureKind<DefaultDB>,
		P: ProofKind<DefaultDB>,
		Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
	{
		// Delegate to Vec's default serialization
		Serialize::serialize(txes, s)
	}

	pub(crate) fn deserialize<'de, DE, S, P>(
		deserializer: DE,
	) -> Result<Vec<SourceBlockTransactions<S, P>>, DE::Error>
	where
		DE: Deserializer<'de>,
		S: SignatureKind<DefaultDB>,
		P: ProofKind<DefaultDB>,
		Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
	{
		// Delegate to Vec's default deserialization
		Deserialize::deserialize(deserializer)
	}
}

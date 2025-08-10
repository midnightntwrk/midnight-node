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
use std::{cell::RefCell, fmt::Debug};

use midnight_node_ledger_helpers::*;

#[derive(Clone, Debug)]
pub struct DeserializedTransactionsWithContextBatch<
	S: SignatureKind<DefaultDB>,
	P: ProofKind<DefaultDB>,
> {
	pub txs: Vec<TransactionWithContext<S, P, DefaultDB>>,
}

#[derive(Debug, Clone)]
pub struct DeserializedTransactionsWithContext<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB>>
{
	pub initial_tx: TransactionWithContext<S, P, DefaultDB>,
	pub batches: Vec<DeserializedTransactionsWithContextBatch<S, P>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>
	DeserializedTransactionsWithContext<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
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
}

thread_local! {
	pub static NETWORK_ID: RefCell<NetworkId> = RefCell::new(NetworkId::Undeployed);
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SerializedTransactionsWithContextBatch {
	pub txs: Vec<String>,
}

impl SerializedTransactionsWithContextBatch {
	fn new<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>(
		batch_txs: &[TransactionWithContext<S, P, DefaultDB>],
		network_id: NetworkId,
	) -> Result<Self, Box<dyn std::error::Error>>
	where
		<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	{
		let txs = batch_txs
			.iter()
			.map(|tx_with_context| {
				// Temporarily override NETWORK_ID for serialization
				NETWORK_ID.with(|id| {
					*id.borrow_mut() = network_id;
					// Serialize TransactionWithContext to a string
					serde_json::to_string(&tx_with_context)
						.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
				})
			})
			.collect::<Result<Vec<String>, Box<dyn std::error::Error>>>()?;

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
		network_id: NetworkId,
	) -> Result<Self, Box<dyn std::error::Error>>
	where
		<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	{
		// Serialize initial_tx
		let initial_tx = NETWORK_ID.with(|id| {
			*id.borrow_mut() = network_id;
			serde_json::to_string(&txs.initial_tx)
				.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
		})?;

		// Serialize each batch
		let batches = txs
			.batches
			.iter()
			.map(|batch| SerializedTransactionsWithContextBatch::new(&batch.txs, network_id))
			.collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

		Ok(SerializedTransactionsWithContext { initial_tx, batches })
	}
}

pub(crate) mod tx_vec {
	use super::*;
	use serde::{Deserialize, Deserializer, Serialize, Serializer};

	pub(crate) fn serialize<SE, S, P>(
		txes: &Vec<TransactionWithContext<S, P, DefaultDB>>,
		s: SE,
	) -> Result<SE::Ok, SE::Error>
	where
		SE: Serializer,
		S: SignatureKind<DefaultDB>,
		P: ProofKind<DefaultDB>,
	{
		// Delegate to Vec's default serialization
		txes.serialize(s)
	}

	pub(crate) fn deserialize<'de, DE, S, P>(
		deserializer: DE,
	) -> Result<Vec<TransactionWithContext<S, P, DefaultDB>>, DE::Error>
	where
		DE: Deserializer<'de>,
		S: SignatureKind<DefaultDB>,
		P: ProofKind<DefaultDB>,
	{
		// Delegate to Vec's default deserialization
		<Vec<midnight_node_ledger_helpers::TransactionWithContext<S, P, DefaultDB>> as Deserialize>::deserialize(deserializer)
	}
}

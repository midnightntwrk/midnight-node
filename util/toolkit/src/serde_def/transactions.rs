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

use midnight_node_ledger_helpers::fork::raw_block_data::{
	RawBlockData, RawTransaction,
};
use midnight_node_ledger_helpers::*;
use serde::{Deserialize, Serialize};
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
	network_id: String,
}

impl SourceTransactions {
	/// Create a new SourceTransactions with pre-computed network_id.
	pub fn new(blocks: Vec<RawBlockData>, network_id: String) -> Self {
		Self { blocks, network_id }
	}

	/// Get the network identifier (e.g. "undeployed", "preview").
	pub fn network(&self) -> &str {
		&self.network_id
	}

	/// Convert typed transactions (from file loading) into RawBlockData.
	///
	/// If `ignore_block_context` is true, all transactions are placed in a single block
	/// with the current timestamp.
	pub fn from_txs_with_context_ignored(
		txs_with_context: impl IntoIterator<
			Item = TransactionWithContext<Signature, ProofMarker, DefaultDB>,
		>,
	) -> Self {
		let now_secs = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("time has run backwards")
			.as_secs();

		let mut network_id = None;
		let mut raw_txs = Vec::new();

		for twc in txs_with_context {
			if network_id.is_none() {
				network_id = twc.tx.network_id().map(|s| s.to_string());
			}
			raw_txs.push(serde_tx_to_raw(&twc.tx));
		}

		let block = RawBlockData {
			hash: [0u8; 32],
			parent_hash: [0u8; 32],
			number: 0,
			spec_version: 000_022_000, // latest
			transactions: raw_txs,
			tblock_secs: now_secs,
			tblock_err: 30,
			parent_block_hash: [0u8; 32],
			last_block_time_secs: 0,
			state_root: None,
			state: None,
		};

		let nid = network_id.expect("no transaction had a network_id");
		Self { blocks: vec![block], network_id: nid }
	}

	/// Convert typed transactions (from file loading) into RawBlockData,
	/// grouping by block context.
	pub fn from_txs_with_context(
		txs: impl IntoIterator<Item = TransactionWithContext<Signature, ProofMarker, DefaultDB>>,
		dust_warp: bool,
	) -> Self {
		let mut blocks = vec![];
		let mut current_batch: Vec<RawTransaction> = vec![];
		let mut last_context: Option<BlockContext> = None;
		let mut number = 0u64;
		let mut network_id = None;

		for tx_with_ctx in txs {
			if network_id.is_none() {
				network_id = tx_with_ctx.tx.network_id().map(|s| s.to_string());
			}

			if last_context
				.as_ref()
				.is_some_and(|c| c.tblock != tx_with_ctx.block_context.tblock)
			{
				let ctx = last_context.unwrap();
				blocks.push(RawBlockData {
					hash: [0u8; 32],
					parent_hash: [0u8; 32],
					number,
					spec_version: 000_022_000,
					transactions: std::mem::take(&mut current_batch),
					tblock_secs: ctx.tblock.to_secs(),
					tblock_err: ctx.tblock_err,
					parent_block_hash: ctx.parent_block_hash.0,
					last_block_time_secs: ctx.last_block_time.to_secs(),
					state_root: None,
					state: None,
				});
				number += 1;
			}
			current_batch.push(serde_tx_to_raw(&tx_with_ctx.tx));
			last_context = Some(tx_with_ctx.block_context);
		}

		if let Some(ref ctx) = last_context {
			blocks.push(RawBlockData {
				hash: [0u8; 32],
				parent_hash: [0u8; 32],
				number,
				spec_version: 000_022_000,
				transactions: current_batch,
				tblock_secs: ctx.tblock.to_secs(),
				tblock_err: ctx.tblock_err,
				parent_block_hash: ctx.parent_block_hash.0,
				last_block_time_secs: ctx.last_block_time.to_secs(),
				state_root: None,
				state: None,
			});
		}

		if dust_warp {
			let now_secs = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("time has run backwards")
				.as_secs();
			let last_time = last_context.map(|c| c.tblock.to_secs()).unwrap_or(now_secs);
			blocks.push(RawBlockData {
				hash: [0u8; 32],
				parent_hash: [0u8; 32],
				number: 0,
				spec_version: 000_022_000,
				transactions: Vec::new(),
				tblock_secs: now_secs,
				tblock_err: 30,
				parent_block_hash: [0u8; 32],
				last_block_time_secs: last_time,
				state_root: None,
				state: None,
			});
		}

		let nid = network_id.unwrap_or_default();
		Self { blocks, network_id: nid }
	}
}

/// Convert a typed SerdeTransaction to a RawTransaction by re-serializing the inner type.
fn serde_tx_to_raw(
	serde_tx: &SerdeTransaction<Signature, ProofMarker, DefaultDB>,
) -> RawTransaction {
	let bytes = serde_tx.serialize_inner().expect("failed to serialize transaction");
	match serde_tx {
		SerdeTransaction::Midnight(_) => RawTransaction::Midnight(bytes),
		SerdeTransaction::System(_) => RawTransaction::System(bytes),
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

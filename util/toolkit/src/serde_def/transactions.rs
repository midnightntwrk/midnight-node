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
	LedgerVersion, RawBlockData, RawTransaction,
};
use midnight_node_ledger_helpers::*;
use serde::{Deserialize, Serialize};
use std::{
	fmt::Debug,
	time::{SystemTime, UNIX_EPOCH},
};

/// A single serialized transaction ready for sending or file output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedTx {
	/// Serialized `Transaction` — the payload for `send_mn_transaction`.
	pub tx: Vec<u8>,
	/// Serialized `BlockContext`
	pub context: BlockContext,
	/// Transaction hash for logging.
	pub tx_hash: [u8; 32],
}

/// Output of a builder — serialized transactions ready for sending.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuiltTransactions {
	pub batches: Vec<Vec<SerializedTx>>,
}

impl BuiltTransactions {
	/// Convert typed `DeserializedTransactionsWithContext<S, P>` into `BuiltTransactions`
	/// by serializing each transaction via `serialize_inner()` and extracting its hash.
	pub fn from_typed<S, P>(typed: DeserializedTransactionsWithContext<S, P>) -> Self
	where
		S: SignatureKind<DefaultDB>,
		P: ProofKind<DefaultDB> + Send + Sync + 'static,
		<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
		Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
	{
		let batches = typed
			.batches
			.iter()
			.map(|batch| batch.txs.iter().map(|twc| SerializedTx::from_serde_tx(&twc.tx)).collect())
			.collect();
		Self { batches }
	}

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

impl SerializedTx {
	fn from_serde_tx<S, P>(tx: &SerdeTransaction<S, P, DefaultDB>) -> Self
	where
		S: SignatureKind<DefaultDB>,
		P: ProofKind<DefaultDB> + Send + Sync + 'static,
		<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
		Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
	{
		let tx_bytes = tx.serialize_inner().expect("failed to serialize transaction");
		let tx_hash = tx.transaction_hash().0.0;
		Self { tx: tx_bytes, context: BlockContext::default(), tx_hash }
	}
}

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
		network_id: &str,
		dust_warp: bool,
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

		Self { blocks, network_id: network_id.to_string() }
	}

	/// Convert untyped transactions (from file loading) into RawBlockData.
	pub fn from_batches(
		batches: impl IntoIterator<Item = Vec<SerializedTx>>,
		dust_warp: bool,
	) -> Self {
		let mut blocks = Vec::new();
		let mut network_id: Option<String> = None;
		let mut ledger_version = LedgerVersion::default();
		for batch in batches {
			let context =
				BuiltTransactions::get_context(&batch).expect("failed to get context for batch");
			// block.transactions = '
			let transactions: Vec<_> =
				batch.iter().map(|t| RawTransaction::Midnight(t.tx.clone())).collect();

			if network_id.is_none() && !transactions.is_empty() {
				let (new_network_id, new_ledger_version) =
					fork::network_id_and_ledger_version_from_tx_bytes(transactions[0].as_bytes());
				network_id = Some(new_network_id);
				ledger_version = new_ledger_version;
			}

			let block = RawBlockData::new_from_timestamp(
				context.tblock.to_secs(),
				ledger_version,
				transactions,
			);
			blocks.push(block);
		}

		// Sort the blocks + set last block time
		blocks.sort();

		for i in 0..blocks.len() {
			// Set last_block_time for all blocks apart from genesis
			if i > 1 {
				blocks[i].last_block_time_secs = blocks[i - 1].tblock_secs;
			}
			blocks[i].ledger_version = ledger_version;
		}

		Self::from_blocks(
			blocks,
			&network_id.expect("no transactions found, can't derive network id"),
			dust_warp,
		)
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
			if network_id.is_none() {
				let (new_network_id, new_ledger_version) =
					fork::network_id_and_ledger_version_from_tx_bytes(&tx.tx);
				network_id = Some(new_network_id);
				ledger_version = new_ledger_version;
			}
			transactions.push(RawTransaction::Midnight(tx.tx));
		}
		let block = RawBlockData::new_from_timestamp(now_secs, ledger_version, transactions);

		let network_id = network_id.expect("no transactions found, can't derive network id");
		Self { blocks: vec![block], network_id }
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
		for batch in self.batches {
			result.extend(batch.txs); // Append each batch's transactions
		}
		result
	}

	pub fn network(&self) -> &str {
		self.batches
			.iter()
			.flat_map(|s| &s.txs)
			.next()
			.expect("all batches empty")
			.tx
			.network_id()
			.expect("no transaction in this batch had a network")
	}
}

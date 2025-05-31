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
use clap::Args;
use midnight_node_ledger_helpers::*;
use std::{fs::File, marker::PhantomData, sync::Arc};
use thiserror::Error;

use crate::{
	indexer::Indexer,
	serde_def::{
		DeserializedTransactionsWithContext, DeserializedTransactionsWithContextBatch,
		SerializedTransactionsWithContext,
	},
};

#[derive(Args)]
pub struct Source {
	/// RPC URL of node instance; Used to fetch existing transactions
	#[arg(long, short = 's', conflicts_with = "src_files", default_value = "ws://127.0.0.1:9944")]
	pub src_url: Option<String>,
	/// Number of threads to use when fetching transactions from a live network
	#[arg(long, conflicts_with = "src_files", default_value = "20")]
	pub fetch_concurrency: usize,
	/// Filename of genesis tx. Used as initial state for generated txs.
	#[arg(long, value_delimiter = ' ', num_args = 1.., conflicts_with = "src_url")]
	pub src_files: Option<Vec<String>>,
}

#[derive(Error, Debug)]
pub enum SourceError {
	#[error("failed to fetch transactions from indexer")]
	TransactionFetchError(#[from] crate::indexer::IndexerError),
	#[error("failed to read genesis transaction file")]
	TransactionReadIOError(#[from] std::io::Error),
	#[error("failed to decode genesis transaction")]
	TransactionReadDecodeError(#[from] hex::FromHexError),
	#[error("failed to fetch network id from rpc")]
	NetworkIdFetchError(#[from] subxt::Error),
}

#[async_trait]
pub trait GetTxs<
	S: SignatureKind<DefaultDB>,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static,
>
{
	async fn get_txs(
		&self,
	) -> Result<DeserializedTransactionsWithContext<S, P>, Box<dyn std::error::Error>>;
	fn network_id(&self) -> NetworkId;
}

pub struct GetTxsFromFile<S, P> {
	network_id: NetworkId,
	files: Vec<String>,
	extension: String,
	_marker_p: PhantomData<P>,
	_marker_s: PhantomData<S>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + 'static> GetTxsFromFile<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
{
	pub fn new(network_id: NetworkId, files: Vec<String>, extension: String) -> Self {
		Self { network_id, files, extension, _marker_p: PhantomData, _marker_s: PhantomData }
	}

	pub fn network_id(&self) -> NetworkId {
		self.network_id
	}

	fn txs_from_files(
		&self,
	) -> Result<DeserializedTransactionsWithContext<S, P>, Box<dyn std::error::Error>> {
		if self.extension == "json" {
			// For json extension, we only handle 1 file
			let file = File::open(&self.files[0])?;
			let loaded_txs: SerializedTransactionsWithContext = serde_json::from_reader(file)?;
			let initial_tx = serde_json::from_str(&loaded_txs.initial_tx)
				.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

			let mut batches = Vec::with_capacity(loaded_txs.batches.len());

			for batch in loaded_txs.batches {
				let mut txs = Vec::with_capacity(batch.txs.len());

				for tx in batch.txs {
					let tx = serde_json::from_str(&tx)
						.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
					txs.push(tx);
				}

				batches.push(DeserializedTransactionsWithContextBatch { txs });
			}

			Ok(DeserializedTransactionsWithContext { initial_tx, batches })
		} else {
			let initial_file = &self.files[0];
			let inital_bytes = std::fs::read(&initial_file)?;

			let initial_tx =
				mn_ledger_serialize::deserialize(inital_bytes.as_slice(), self.network_id)
					.or_else(|_err| {
						// Check if it failed because it is a `Transaction` and not `TransactionWithContext`
						// Useful for handling genesis tx
						let tx: Transaction<S, P, PedersenRandomness, DefaultDB> =
							mn_ledger_serialize::deserialize(
								inital_bytes.as_slice(),
								self.network_id,
							)?;
						let mut tx_with_context = TransactionWithContext::new(tx, None);
						tx_with_context.block_context.tblock = Timestamp::from_secs(0);
						Ok::<TransactionWithContext<S, P, DefaultDB>, std::io::Error>(
							tx_with_context,
						)
					})?;

			let mut txs = Vec::with_capacity(self.files.len());

			// In case there are more than one file
			if self.files.len() > 1 {
				for file in &self.files[1..] {
					let bytes = std::fs::read(&file)?;
					let tx = mn_ledger_serialize::deserialize(bytes.as_slice(), self.network_id)?;
					txs.push(tx)
				}
			}

			Ok(DeserializedTransactionsWithContext {
				initial_tx,
				batches: vec![DeserializedTransactionsWithContextBatch { txs }],
			})
		}
	}
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Send + Sync + 'static,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + Sync + 'static,
> GetTxs<S, P> for GetTxsFromFile<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
{
	async fn get_txs(
		&self,
	) -> Result<DeserializedTransactionsWithContext<S, P>, Box<dyn std::error::Error>> {
		let txs = self.txs_from_files()?;
		Ok(txs)
	}

	fn network_id(&self) -> NetworkId {
		self.network_id()
	}
}

pub struct GetTxsFromUrl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + 'static> {
	pub indexer: Arc<Indexer<S, P>>,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + 'static> GetTxsFromUrl<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
{
	pub fn new(indexer: Arc<Indexer<S, P>>) -> Self {
		Self { indexer }
	}

	pub fn network_id(&self) -> NetworkId {
		self.indexer.network_id
	}
}

#[async_trait]
impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static>
	GetTxs<S, P> for GetTxsFromUrl<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
{
	async fn get_txs(
		&self,
	) -> Result<DeserializedTransactionsWithContext<S, P>, Box<dyn std::error::Error>> {
		let indexer_handle = self.indexer.clone().start().await?;
		let txs = self.indexer.clone().get_txs().await;
		indexer_handle.stop().await?;

		Ok(DeserializedTransactionsWithContext {
			initial_tx: txs.first().expect("We know there is at leats 1 tx in Genesis").clone(),
			batches: vec![DeserializedTransactionsWithContextBatch { txs: txs[1..].to_vec() }],
		})
	}

	fn network_id(&self) -> NetworkId {
		self.network_id()
	}
}

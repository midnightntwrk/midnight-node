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
	client::ClientError,
	indexer::Indexer,
	serde_def::{SerializedTransactionsWithContext, SourceTransactions},
};

#[derive(Args, Debug)]
pub struct Source {
	/// Load input transactions/blocks from node instance using an RPC URL
	#[arg(
		long,
		short = 's',
		conflicts_with = "src_files",
		default_value = "ws://127.0.0.1:9944",
		global = true
	)]
	pub src_url: Option<String>,
	/// Number of threads to use when fetching transactions from a live network
	#[arg(long, conflicts_with = "src_files", default_value = "20", global = true)]
	pub fetch_concurrency: usize,
	/// Load input transactions/blocks from file(s). Used as initial state for transaction generator.
	#[arg(long = "src-file", value_delimiter = ' ', conflicts_with = "src_url", global = true)]
	pub src_files: Option<Vec<String>>,
}

#[derive(Error, Debug)]
pub enum SourceError {
	#[error("failed to fetch transactions from indexer")]
	TransactionFetchError(#[from] crate::indexer::IndexerError),
	#[error("failed to initialize midnight node client")]
	ClientInitializationError(#[from] ClientError),
	#[error("failed to read genesis transaction file")]
	TransactionReadIOError(#[from] std::io::Error),
	#[error("failed to decode genesis transaction")]
	TransactionReadDecodeError(#[from] hex::FromHexError),
	#[error("failed to deserialize transaction")]
	TransactionReadDeserialzeError(#[from] serde_json::Error),
	#[error("failed to fetch network id from rpc")]
	NetworkIdFetchError(#[from] subxt::Error),
	#[error("invalid source args")]
	InvalidSourceArgs(Source),
}

#[async_trait]
pub trait GetTxs<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static,
> where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static,
> GetTxs<S, P> for ()
{
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>> {
		Ok(SourceTransactions { blocks: vec![] })
	}
}

pub struct GetTxsFromFile<S, P> {
	files: Vec<String>,
	extension: String,
	_marker_p: PhantomData<P>,
	_marker_s: PhantomData<S>,
}

impl<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + Send + std::fmt::Debug + 'static,
> GetTxsFromFile<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub fn new(files: Vec<String>, extension: String) -> Self {
		Self { files, extension, _marker_p: PhantomData, _marker_s: PhantomData }
	}

	fn txs_from_files(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>> {
		if self.extension == "json" {
			// For json extension, we only handle 1 file
			let file = File::open(&self.files[0])?;
			let loaded_txs: SerializedTransactionsWithContext = serde_json::from_reader(file)?;
			let mut txs: Vec<TransactionWithContext<S, P, DefaultDB>> =
				vec![serde_json::from_str(&loaded_txs.initial_tx).map_err(|e| Box::new(e))?];
			for batch in loaded_txs.batches {
				for tx in batch.txs {
					txs.push(serde_json::from_str(&tx).map_err(|e| Box::new(e))?);
				}
			}
			Ok(SourceTransactions::from_txs_with_context(txs))
		} else {
			let mut txs = vec![];
			for file in &self.files {
				let bytes = std::fs::read(file)?;
				// files can either be one TransactionWithContext or many of them
				let mut file_txs = mn_ledger_serialize::tagged_deserialize(bytes.as_slice())
					.or_else(|_| {
						mn_ledger_serialize::tagged_deserialize(bytes.as_slice()).map(|tx| vec![tx])
					})?;
				txs.append(&mut file_txs);
			}
			Ok(SourceTransactions::from_txs_with_context(txs))
		}
	}
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged + Send + Sync + 'static,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + Sync + 'static,
> GetTxs<S, P> for GetTxsFromFile<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>> {
		let txs = self.txs_from_files()?;
		Ok(txs)
	}
}

pub struct GetTxsFromUrl<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + Send + 'static,
> where
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub indexer: Arc<Indexer<S, P>>,
}

impl<S: SignatureKind<DefaultDB> + Tagged, P: ProofKind<DefaultDB> + Send + 'static>
	GetTxsFromUrl<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub fn new(indexer: Arc<Indexer<S, P>>) -> Self {
		Self { indexer }
	}
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static,
> GetTxs<S, P> for GetTxsFromUrl<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>> {
		let indexer_handle = self.indexer.clone().start().await?;
		let blocks = self.indexer.clone().get_blocks().await;
		indexer_handle.stop().await?;

		Ok(SourceTransactions { blocks })
	}
}

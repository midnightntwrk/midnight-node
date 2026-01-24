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

use crate::{
	cli_parsers as cli,
	fetcher::{fetch_all, fetch_storage},
};
use async_trait::async_trait;
use clap::Args;
use midnight_node_ledger_helpers::*;
use std::{
	fs::File,
	marker::PhantomData,
	str::FromStr,
	time::{SystemTime, UNIX_EPOCH},
};
use subxt::utils::H256;
use thiserror::Error;

use crate::fetcher::fetch_storage::BlockData;
use crate::{
	client::ClientError,
	serde_def::{SerializedTransactionsWithContext, SourceTransactions},
};

#[derive(Clone, Debug)]
pub enum FetchCacheConfig {
	InMemory,
	Redb { filename: String },
	Postgres { database_url: String },
}

#[derive(Debug, thiserror::Error)]
pub enum FetchCacheConfigParseError {
	#[error("could not find delimited ':'")]
	MissingDelimiter,
	#[error("unknown prefix for fetch source: {0}")]
	UnknownPrefix(String),
}

impl FromStr for FetchCacheConfig {
	type Err = FetchCacheConfigParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let prefix: String;
		let opts: String;

		match s.split_once(":") {
			Some((s0, s1)) => {
				prefix = s0.to_string();
				opts = s1.to_string();
			},
			None => {
				prefix = s.to_string();
				opts = String::new();
			},
		}

		match prefix.as_str() {
			"redb" => {
				let filename = opts;
				Ok(Self::Redb { filename })
			},
			"inmemory" => Ok(Self::InMemory),
			"postgres" => Ok(Self::Postgres { database_url: s.to_string() }),
			_ => Err(FetchCacheConfigParseError::UnknownPrefix(prefix)),
		}
	}
}

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
	/// Ignore block context. Useful when using `send` subcommand
	#[arg(long, conflicts_with = "src_url", global = true)]
	pub ignore_block_context: bool,
	/// Spend DUST with timestamp as system time rather than the previous block timestamp. Useful
	/// if loading from a genesis file, but may result in invalid proofs when connected to a live
	/// chain
	#[arg(long, global = true)]
	pub dust_warp: bool,

	#[arg(
		long,
		global = true,
		value_parser = cli::fetch_cache_config,
		default_value = "redb:toolkit.db",
		env = "MN_FETCH_CACHE"
	)]
	/// Fetch cache config. Available options:
	/// - "inmemory" (i.e. no cache),
	/// - "redb:<filename>" (file-cache, single-writer)
	/// - "postgres://[user[:password]@][netloc][:port][/dbname][?param1=value1&...]" (external db, multi-writer)
	pub fetch_cache: FetchCacheConfig,
}

#[derive(Error, Debug)]
pub enum SourceError {
	#[error("failed to fetch transactions")]
	TransactionFetchError(#[from] crate::fetcher::FetchError),
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
	#[error(
		"toolkit only supports a single .json transaction as input - use `--to-bytes` and `.mn` format for multiple txs"
	)]
	TooManyJsonInputs,
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
	dust_warp: bool,
	ignore_block_context: bool,
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
	pub fn new(
		files: Vec<String>,
		extension: String,
		dust_warp: bool,
		ignore_block_context: bool,
	) -> Self {
		Self {
			files,
			extension,
			dust_warp,
			ignore_block_context,
			_marker_p: PhantomData,
			_marker_s: PhantomData,
		}
	}

	fn txs_from_files(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>> {
		if self.extension == "json" {
			// For json extension, we only handle 1 file
			if self.files.len() > 1 {
				return Err(Box::new(SourceError::TooManyJsonInputs));
			}
			let file = File::open(&self.files[0])?;
			let loaded_txs: SerializedTransactionsWithContext = serde_json::from_reader(file)?;
			let mut txs: Vec<TransactionWithContext<S, P, DefaultDB>> =
				vec![serde_json::from_str(&loaded_txs.initial_tx).map_err(|e| Box::new(e))?];
			for batch in loaded_txs.batches {
				for tx in batch.txs {
					txs.push(serde_json::from_str(&tx).map_err(|e| Box::new(e))?);
				}
			}
			Ok(SourceTransactions::from_txs_with_context(txs, self.dust_warp))
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
			if self.ignore_block_context {
				Ok(SourceTransactions::from_txs_with_context_ignored(txs))
			} else {
				Ok(SourceTransactions::from_txs_with_context(txs, self.dust_warp))
			}
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

pub struct GetTxsFromUrl {
	pub rpc_url: String,
	pub num_fetch_workers: usize,
	pub dust_warp: bool,
	pub fetch_cache_config: FetchCacheConfig,
}

impl GetTxsFromUrl {
	pub fn new(
		rpc_url: &str,
		num_fetch_workers: usize,
		dust_warp: bool,
		fetch_cache_config: FetchCacheConfig,
	) -> Self {
		Self { rpc_url: rpc_url.to_string(), num_fetch_workers, dust_warp, fetch_cache_config }
	}
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged,
	P: ProofKind<DefaultDB> + std::fmt::Debug + Send + 'static,
> GetTxs<S, P> for GetTxsFromUrl
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions<S, P>, Box<dyn std::error::Error + Send + Sync>> {
		let mut blocks = match &self.fetch_cache_config {
			FetchCacheConfig::InMemory => {
				fetch_all(&self.rpc_url, self.num_fetch_workers, fetch_storage::InMemory::default())
					.await?
			},
			FetchCacheConfig::Redb { filename } => {
				fetch_all(
					&self.rpc_url,
					self.num_fetch_workers,
					fetch_storage::redb_backend::RedbBackend::new(filename),
				)
				.await?
			},
			FetchCacheConfig::Postgres { database_url } => {
				fetch_all(
					&self.rpc_url,
					self.num_fetch_workers,
					fetch_storage::postgres_backend::PostgresBackend::new(&database_url).await,
				)
				.await?
			},
		};

		if self.dust_warp {
			// Add an empty block with a now() as a block_context
			let now = Timestamp::from_secs(
				SystemTime::now()
					.duration_since(UNIX_EPOCH)
					.expect("time has run backwards")
					.as_secs(),
			);
			let context =
				BlockContext { tblock: now, tblock_err: 30, parent_block_hash: Default::default() };
			blocks.push(BlockData {
				hash: H256::zero(),
				parent_hash: H256::zero(),
				number: 0,
				transactions: Vec::new(),
				context,
				state_root: None,
			});
		}

		Ok(SourceTransactions { blocks })
	}
}

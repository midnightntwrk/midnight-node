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

use crate::{
	cli_parsers as cli,
	fetcher::{fetch_all, fetch_storage},
};
use async_trait::async_trait;
use clap::Args;
use midnight_node_ledger_helpers::fork::raw_block_data::{SerializedTx, SerializedTxBatches};
use std::{fs::File, str::FromStr};
use thiserror::Error;

use crate::{client::ClientError, serde_def::SourceTransactions};

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

#[derive(Args, Clone, Debug)]
pub struct Source {
	/// Load input transactions/blocks from the node instance using an RPC URL
	#[arg(
		long,
		short = 's',
		conflicts_with = "src_files",
		default_value = "ws://127.0.0.1:9944",
		env = "MN_SRC_URL",
		global = true
	)]
	pub src_url: Option<String>,
	/// Read transactions from the cache only - don't fetch anything from RPC
	#[arg(long, global = true)]
	pub fetch_only_cached: bool,
	/// Number of threads to use when fetching transactions from a live network
	#[arg(long, conflicts_with = "src_files", default_value = "20", global = true)]
	pub fetch_concurrency: usize,
	/// Number of threads to use for compute operations when fetching from a live network.
	/// Defaults to number of CPU cores if not specified.
	#[arg(long, conflicts_with = "src_files", global = true)]
	pub fetch_compute_concurrency: Option<usize>,
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
	/// Fetch cache config. Caches both block data and wallet state.
	/// Available options:
	/// - "inmemory" (RAM-only, no persistence),
	/// - "redb:<filename>" (file-cache, single-writer)
	/// - "postgres://[user[:password]@][netloc][:port][/dbname][?param1=value1&...]" (external db, multi-writer)
	///
	/// When using redb or postgres backends, wallet state is also cached to speed up subsequent runs.
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
pub trait GetTxs: Send + Sync {
	async fn get_txs(&self)
	-> Result<SourceTransactions, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl GetTxs for () {
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions, Box<dyn std::error::Error + Send + Sync>> {
		Ok(SourceTransactions::new(vec![], "undeployed"))
	}
}

pub struct GetTxsFromFile {
	files: Vec<String>,
	dust_warp: bool,
	ignore_block_context: bool,
}

impl GetTxsFromFile {
	pub fn new(files: Vec<String>, dust_warp: bool, ignore_block_context: bool) -> Self {
		Self { files, dust_warp, ignore_block_context }
	}

	pub fn load_single(filename: &str) -> Result<SerializedTx, std::io::Error> {
		let file = File::open(filename)?;
		let tx = serde_json::from_reader(file)?;
		Ok(tx)
	}

	pub fn load_multiple(filename: &str) -> Result<SerializedTxBatches, std::io::Error> {
		let file = File::open(filename)?;
		Ok(serde_json::from_reader(file)?)
	}

	pub fn load_single_or_multiple(filename: &str) -> Result<SerializedTxBatches, std::io::Error> {
		if let Ok(loaded) = Self::load_single(filename) {
			return Ok(SerializedTxBatches { batches: vec![vec![loaded]] });
		};
		log::debug!("failed to load {} as single tx, loading as multiple...", filename);
		return Self::load_multiple(filename);
	}

	fn txs_from_files(
		&self,
	) -> Result<SourceTransactions, Box<dyn std::error::Error + Send + Sync>> {
		if let [filename] = self.files.as_slice() {
			let built_txs = Self::load_single_or_multiple(filename)?;

			if self.ignore_block_context {
				let txs: Vec<SerializedTx> = built_txs.batches.into_iter().flatten().collect();
				Ok(SourceTransactions::from_txs(txs))
			} else {
				Ok(SourceTransactions::from_batches(built_txs.batches, self.dust_warp))
			}
		} else {
			// Load from multiple files
			let res: Result<Vec<SerializedTxBatches>, _> =
				self.files.iter().map(|f| Self::load_single_or_multiple(f)).collect();
			let batches: Vec<Vec<SerializedTx>> =
				res?.into_iter().flat_map(|b| b.batches).collect();

			if self.ignore_block_context {
				Ok(SourceTransactions::from_txs(batches.into_iter().flatten()))
			} else {
				Ok(SourceTransactions::from_batches(batches, self.dust_warp))
			}
		}
	}
}

#[async_trait]
impl GetTxs for GetTxsFromFile {
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions, Box<dyn std::error::Error + Send + Sync>> {
		let txs = self.txs_from_files()?;
		Ok(txs)
	}
}

pub struct GetTxsFromUrl {
	pub rpc_url: String,
	pub num_fetch_workers: usize,
	pub num_compute_workers: usize,
	pub dust_warp: bool,
	pub fetch_only_cache: bool,
	pub fetch_cache_config: FetchCacheConfig,
}

impl GetTxsFromUrl {
	pub fn new(
		rpc_url: &str,
		num_fetch_workers: usize,
		num_compute_workers: usize,
		dust_warp: bool,
		fetch_only_cache: bool,
		fetch_cache_config: FetchCacheConfig,
	) -> Self {
		Self {
			rpc_url: rpc_url.to_string(),
			num_fetch_workers,
			num_compute_workers,
			dust_warp,
			fetch_only_cache,
			fetch_cache_config,
		}
	}
}

#[async_trait]
impl GetTxs for GetTxsFromUrl {
	async fn get_txs(
		&self,
	) -> Result<SourceTransactions, Box<dyn std::error::Error + Send + Sync>> {
		let blocks = match &self.fetch_cache_config {
			FetchCacheConfig::InMemory => {
				fetch_all(
					&self.rpc_url,
					self.num_fetch_workers,
					self.num_compute_workers,
					self.fetch_only_cache,
					fetch_storage::InMemory::default(),
				)
				.await?
			},
			FetchCacheConfig::Redb { filename } => {
				fetch_all(
					&self.rpc_url,
					self.num_fetch_workers,
					self.num_compute_workers,
					self.fetch_only_cache,
					fetch_storage::redb_backend::RedbBackend::new(filename),
				)
				.await?
			},
			FetchCacheConfig::Postgres { database_url } => {
				fetch_all(
					&self.rpc_url,
					self.num_fetch_workers,
					self.num_compute_workers,
					self.fetch_only_cache,
					fetch_storage::postgres_backend::PostgresBackend::new(&database_url).await,
				)
				.await?
			},
		};

		Ok(SourceTransactions::from_blocks(blocks, self.dust_warp))
	}
}

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

use core::fmt::Display;

use crate::{
	cli_parsers as cli,
	client::MidnightNodeClient,
	fetcher::{self, fetch_storage},
	tx_generator::source::{FetchCacheConfig, GetTxsFromFile},
	utils::format_timestamp_utc,
};
use clap::Args;
use midnight_node_ledger_helpers::fork::raw_block_data::{LedgerVersion, RawBlockData};
use serde::{Deserialize, Serialize};

#[derive(Args)]
pub struct ShowBlockArgs {
	/// Block number to inspect (from node/cache)
	#[arg(short, long, required_unless_present = "src_file")]
	block_number: Option<u64>,
	/// Load block data from a file (e.g. genesis .mn file)
	#[arg(long, conflicts_with_all = ["block_number", "src_url", "fetch_cache", "fetch_only_cached"])]
	src_file: Option<String>,
	/// Output as JSON
	#[arg(long)]
	json: bool,
	/// Node RPC URL
	#[arg(long, short = 's', default_value = "ws://127.0.0.1:9944", env = "MN_SRC_URL")]
	src_url: String,
	/// Fetch cache config
	#[arg(
		long,
		value_parser = cli::fetch_cache_config,
		default_value = "redb:toolkit_cache/fetch_cache.db",
		env = "MN_FETCH_CACHE"
	)]
	fetch_cache: FetchCacheConfig,
	/// Only read from cache, don't fetch from node
	#[arg(long)]
	fetch_only_cached: bool,
	/// Dry-run - don't connect to a node, just print out settings
	#[arg(long)]
	dry_run: bool,
}

#[derive(Debug)]
pub enum ShowBlockValue {
	Human(Vec<ShowBlockJson>),
	Json(Vec<ShowBlockJson>),
	DryRun(()),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShowBlockJson {
	pub number: u64,
	#[serde(with = "hex")]
	pub hash: [u8; 32],
	#[serde(with = "hex")]
	pub parent_hash: [u8; 32],
	pub ledger_version: LedgerVersion,
	pub timestamp_secs: u64,
	pub timestamp_utc: String,
	pub timestamp_err_secs: u32,
	pub last_block_time_secs: u64,
	#[serde(with = "hex")]
	pub state_root: Vec<u8>,
	pub transactions: Vec<ShowBlockTransaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShowBlockTransaction {
	pub index: usize,
	pub tx_type: String,
	pub size_bytes: usize,
	#[serde(with = "hex")]
	pub hash: [u8; 32],
	pub debug_str: String,
}

impl ShowBlockJson {
	pub fn new(raw_block: &RawBlockData) -> Result<Self, std::io::Error> {
		let transactions = deserialize_transactions(raw_block)?;
		Ok(Self {
			number: raw_block.number,
			hash: raw_block.hash,
			parent_hash: raw_block.parent_hash,
			ledger_version: raw_block.ledger_version,
			timestamp_secs: raw_block.tblock_secs,
			timestamp_utc: format_timestamp_utc(raw_block.tblock_secs),
			timestamp_err_secs: raw_block.tblock_err,
			last_block_time_secs: raw_block.last_block_time_secs,
			state_root: raw_block.state_root.as_ref().cloned().unwrap_or(Vec::new()),
			transactions,
		})
	}
}

impl Display for ShowBlockJson {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(f, "Block #{}", self.number)?;
		writeln!(f, "  Hash:            0x{}", hex::encode(self.hash))?;
		writeln!(f, "  Parent Hash:     0x{}", hex::encode(self.parent_hash))?;
		writeln!(f, "  Ledger Version:  {:?}", self.ledger_version)?;
		writeln!(
			f,
			"  Timestamp:       {} ({}, err: {}s)",
			self.timestamp_secs, self.timestamp_secs, self.timestamp_utc
		)?;
		writeln!(f, "  Parent Block Timestamp:       {}", self.last_block_time_secs,)?;
		writeln!(f, "  State Root:      0x{}", hex::encode(&self.state_root))?;
		writeln!(f, "  Transactions:    {}", self.transactions.len())?;

		for tx in &self.transactions {
			writeln!(f,)?;
			writeln!(f, "{}", tx)?;
		}
		Ok(())
	}
}

impl Display for ShowBlockTransaction {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(
			f,
			"  [{}] {} ({} bytes) hash: 0x{}",
			self.index,
			self.tx_type,
			self.size_bytes,
			hex::encode(self.hash)
		)?;
		for line in self.debug_str.lines() {
			writeln!(f, "    {line}")?;
		}
		Ok(())
	}
}

pub fn deserialize_transactions(
	block: &RawBlockData,
) -> Result<Vec<ShowBlockTransaction>, std::io::Error> {
	block
		.transactions
		.iter()
		.enumerate()
		.map(|(i, raw)| match block.ledger_version {
			LedgerVersion::Ledger8 => {
				use crate::commands::fork::ledger_8::show_transaction::ShowTransaction;
				let ShowTransaction { tx_type, size_bytes, hash, debug_str } = raw.try_into()?;
				Ok(ShowBlockTransaction { index: i, tx_type, size_bytes, hash, debug_str })
			},
			LedgerVersion::Ledger7 => {
				use crate::commands::fork::ledger_7::show_transaction::ShowTransaction;
				let ShowTransaction { tx_type, size_bytes, hash, debug_str } = raw.try_into()?;
				Ok(ShowBlockTransaction { index: i, tx_type, size_bytes, hash, debug_str })
			},
		})
		.collect()
}

fn blocks_from_file(
	path: &str,
) -> Result<Vec<RawBlockData>, Box<dyn std::error::Error + Send + Sync>> {
	let batches = GetTxsFromFile::load_single_or_multiple(path)?;
	let blocks: Vec<RawBlockData> = (&batches).try_into()?;
	Ok(blocks)
}

pub async fn execute(
	args: ShowBlockArgs,
) -> Result<ShowBlockValue, Box<dyn std::error::Error + Send + Sync>> {
	if args.dry_run {
		if let Some(ref src_file) = args.src_file {
			log::info!("Dry-run: show-block from file {:?}", src_file);
		} else {
			log::info!("Dry-run: show-block #{}", args.block_number.unwrap());
			log::info!("Dry-run: source url: {:?}", args.src_url);
			log::info!("Dry-run: fetch cache: {:?}", args.fetch_cache);
			log::info!("Dry-run: fetch only cached: {}", args.fetch_only_cached);
		}
		log::info!("Dry-run: json output: {}", args.json);
		return Ok(ShowBlockValue::DryRun(()));
	}

	// Fetch from file
	if let Some(ref src_file) = args.src_file {
		let blocks = blocks_from_file(src_file)?;
		let values_res: Result<Vec<ShowBlockJson>, _> =
			blocks.iter().map(ShowBlockJson::new).collect();
		let value = if args.json {
			ShowBlockValue::Json(values_res?)
		} else {
			ShowBlockValue::Human(values_res?)
		};
		return Ok(value);
	}

	// Fetch from RPC
	let block_number = args.block_number.unwrap();
	let client = MidnightNodeClient::new(&args.src_url, None).await?;
	let chain_id = client.get_block_one_hash().await?;

	let fetch_client = if args.fetch_only_cached { None } else { Some(&client) };

	let block = match &args.fetch_cache {
		FetchCacheConfig::InMemory => {
			let storage = fetch_storage::InMemory::default();
			fetcher::fetch_single_block(chain_id, block_number, fetch_client, &storage).await?
		},
		FetchCacheConfig::Redb { filename } => {
			let storage = fetch_storage::redb_backend::RedbBackend::new(filename);
			fetcher::fetch_single_block(chain_id, block_number, fetch_client, &storage).await?
		},
		FetchCacheConfig::Postgres { database_url } => {
			let storage = fetch_storage::postgres_backend::PostgresBackend::new(database_url).await;
			fetcher::fetch_single_block(chain_id, block_number, fetch_client, &storage).await?
		},
	};

	let block = ShowBlockJson::new(&block)?;
	let value = if args.json {
		ShowBlockValue::Json(vec![block])
	} else {
		ShowBlockValue::Human(vec![block])
	};
	Ok(value)
}

#[cfg(test)]
mod test {
	use crate::{
		commands::show_block::{ShowBlockArgs, ShowBlockValue},
		tx_generator::source::FetchCacheConfig,
	};

	#[tokio::test]
	async fn test_show_block_from_file() {
		let result = super::execute(ShowBlockArgs {
			src_file: Some("../../res/test-tx-deserialize/serialized_tx.mn".to_string()),
			block_number: None,
			json: true,
			src_url: "".to_string(),
			fetch_cache: FetchCacheConfig::InMemory,
			fetch_only_cached: false,
			dry_run: false,
		})
		.await
		.unwrap();

		let ShowBlockValue::Json(value) = result else {
			panic!("result is not json");
		};
		assert!(!value.is_empty());
	}
}

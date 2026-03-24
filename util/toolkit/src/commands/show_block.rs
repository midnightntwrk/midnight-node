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
	client::MidnightNodeClient,
	fetcher::{self, fetch_storage},
	tx_generator::source::FetchCacheConfig,
};
use clap::Args;
use midnight_node_ledger_helpers::fork::raw_block_data::{
	LedgerVersion, RawBlockData, RawTransaction,
};

#[derive(Args)]
pub struct ShowBlockArgs {
	/// Block number to inspect
	#[arg(short, long)]
	block_number: u64,
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

struct DeserializedTx {
	index: usize,
	tx_type: &'static str,
	size_bytes: usize,
	hash: [u8; 32],
	debug_str: String,
}

fn deserialize_transactions(
	block: &RawBlockData,
) -> Result<Vec<DeserializedTx>, Box<dyn std::error::Error + Send + Sync>> {
	block
		.transactions
		.iter()
		.enumerate()
		.map(|(i, raw)| {
			let (debug_str, size, hash) = match block.ledger_version {
				LedgerVersion::Ledger8 => {
					crate::commands::fork::ledger_8::show_transaction::deserialize_raw_transaction(
						raw,
					)?
				},
				LedgerVersion::Ledger7 => {
					crate::commands::fork::ledger_7::show_transaction::deserialize_raw_transaction(
						raw,
					)?
				},
			};
			let tx_type = match raw {
				RawTransaction::Midnight(_) => "Midnight",
				RawTransaction::System(_) => "System",
			};
			Ok(DeserializedTx { index: i, tx_type, size_bytes: size, hash, debug_str })
		})
		.collect()
}

fn format_timestamp_utc(epoch_secs: u64) -> String {
	const SECS_PER_DAY: u64 = 86400;
	let days = epoch_secs / SECS_PER_DAY;
	let day_secs = epoch_secs % SECS_PER_DAY;
	let h = day_secs / 3600;
	let m = (day_secs % 3600) / 60;
	let s = day_secs % 60;

	// Civil date from days since 1970-01-01 (algorithm from Howard Hinnant)
	let z = days as i64 + 719468;
	let era = z.div_euclid(146097);
	let doe = z.rem_euclid(146097) as u64;
	let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
	let y = yoe as i64 + era * 400;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let d = doy - (153 * mp + 2) / 5 + 1;
	let mo = if mp < 10 { mp + 3 } else { mp - 9 };
	let y = if mo <= 2 { y + 1 } else { y };

	format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn print_human_readable(block: &RawBlockData, txs: &[DeserializedTx]) {
	println!("Block #{}", block.number);
	println!("  Hash:            0x{}", hex::encode(block.hash));
	println!("  Parent Hash:     0x{}", hex::encode(block.parent_hash));
	println!("  Ledger Version:  {:?}", block.ledger_version);
	println!(
		"  Timestamp:       {} ({}, err: {}s)",
		block.tblock_secs,
		format_timestamp_utc(block.tblock_secs),
		block.tblock_err
	);
	if let Some(ref sr) = block.state_root {
		println!("  State Root:      0x{}", hex::encode(sr));
	}
	println!("  Transactions:    {}", block.transactions.len());

	for tx in txs {
		println!();
		println!(
			"  [{}] {} ({} bytes) hash: 0x{}",
			tx.index,
			tx.tx_type,
			tx.size_bytes,
			hex::encode(tx.hash)
		);
		for line in tx.debug_str.lines() {
			println!("    {line}");
		}
	}
}

fn to_json(block: &RawBlockData, txs: &[DeserializedTx]) -> serde_json::Value {
	let tx_values: Vec<serde_json::Value> = txs
		.iter()
		.map(|tx| {
			serde_json::json!({
				"index": tx.index,
				"type": tx.tx_type,
				"size_bytes": tx.size_bytes,
				"hash": format!("0x{}", hex::encode(tx.hash)),
				"deserialized": tx.debug_str,
			})
		})
		.collect();

	serde_json::json!({
		"number": block.number,
		"hash": format!("0x{}", hex::encode(block.hash)),
		"parent_hash": format!("0x{}", hex::encode(block.parent_hash)),
		"ledger_version": format!("{:?}", block.ledger_version),
		"timestamp_secs": block.tblock_secs,
		"timestamp_utc": format_timestamp_utc(block.tblock_secs),
		"timestamp_err_secs": block.tblock_err,
		"state_root": block.state_root.as_ref().map(|sr| format!("0x{}", hex::encode(sr))),
		"transactions": tx_values,
	})
}

pub async fn execute(args: ShowBlockArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	if args.dry_run {
		log::info!("Dry-run: show-block #{}", args.block_number);
		log::info!("Dry-run: source url: {:?}", args.src_url);
		log::info!("Dry-run: fetch cache: {:?}", args.fetch_cache);
		log::info!("Dry-run: fetch only cached: {}", args.fetch_only_cached);
		log::info!("Dry-run: json output: {}", args.json);
		return Ok(());
	}

	let client = MidnightNodeClient::new(&args.src_url, None).await?;
	let chain_id = client.get_block_one_hash().await?;

	let fetch_client = if args.fetch_only_cached { None } else { Some(&client) };

	let block = match &args.fetch_cache {
		FetchCacheConfig::InMemory => {
			let storage = fetch_storage::InMemory::default();
			fetcher::fetch_single_block(chain_id, args.block_number, fetch_client, &storage).await?
		},
		FetchCacheConfig::Redb { filename } => {
			let storage = fetch_storage::redb_backend::RedbBackend::new(filename);
			fetcher::fetch_single_block(chain_id, args.block_number, fetch_client, &storage).await?
		},
		FetchCacheConfig::Postgres { database_url } => {
			let storage = fetch_storage::postgres_backend::PostgresBackend::new(database_url).await;
			fetcher::fetch_single_block(chain_id, args.block_number, fetch_client, &storage).await?
		},
	};

	let txs = deserialize_transactions(&block)?;

	if args.json {
		let json = to_json(&block, &txs);
		println!("{}", serde_json::to_string_pretty(&json)?);
	} else {
		print_human_readable(&block, &txs);
	}

	Ok(())
}

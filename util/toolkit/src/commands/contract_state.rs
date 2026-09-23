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

use super::super::tx_generator::{TxGenerator, source::Source};
use crate::cli_parsers as cli;
use crate::commands::fork::{ledger_8, ledger_9};
use crate::tx_generator::builder::build_fork_aware_context_cached;
use crate::tx_generator::source::create_file_wallet_cache;
use clap::Args;
use midnight_ledger_unsafe_helpers::{
	ContractAddress, fork::fork_aware_context::ForkAwareLedgerContext,
};
use std::{fs, path::Path};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Args)]
pub struct ContractStateArgs {
	#[command(flatten)]
	pub source: Source,
	/// Contract Address
	#[arg(long, value_parser = cli::contract_address_decode)]
	pub contract_address: ContractAddress,
	/// Destination file to save the state
	#[arg(long, short)]
	pub dest_file: Option<String>,
	/// Dry-run - don't fetch anything, just print out the settings
	#[arg(long)]
	pub dry_run: bool,
}

pub async fn execute(args: ContractStateArgs) -> Result<(), BoxError> {
	#[cfg(not(feature = "indexer-client"))]
	args.source
		.reject_indexer("contract-state", crate::tx_generator::source::NO_INDEXER_CLIENT)?;

	if args.dry_run {
		TxGenerator::source(args.source, true).await?;
		log::info!("Dry-run: fetch contract state for address: {:?}", args.contract_address);
		log::info!("Dry-run: write contract state to file: {:?}", args.dest_file);
		return Ok(());
	}

	let serialized_state = contract_state(args.source, args.contract_address).await?;

	if let Some(dest_file) = &args.dest_file {
		let full_path = Path::new(dest_file);
		if let Some(directory) = full_path.parent() {
			fs::create_dir_all(directory).expect("failed to create directories");
		}

		fs::write(full_path, serialized_state).expect("failed to create file");
	}

	Ok(())
}

async fn contract_state(source: Source, address: ContractAddress) -> Result<Vec<u8>, BoxError> {
	#[cfg(feature = "indexer-client")]
	if let Some(indexer_url) = source.indexer_url.as_deref() {
		use crate::tx_generator::indexer::{IndexerLedgerContext, connect_indexer};
		return match connect_indexer(&source, indexer_url).await?.0 {
			IndexerLedgerContext::Ledger8(ctx) => {
				ledger_8::contract_state::get_contract_state(ctx.as_ref(), address).await
			},
			IndexerLedgerContext::Ledger9(ctx) => {
				ledger_9::contract_state::get_contract_state(ctx.as_ref(), address).await
			},
		};
	}

	let ledger_state_db = source.ledger_state_db.clone();
	let fetch_cache = source.fetch_cache.clone();
	let replay_checkpoint_interval = source.replay_checkpoint_interval;
	let blocks = TxGenerator::source(source, false).await?.get_txs().await?;
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	match build_fork_aware_context_cached(
		&[],
		&blocks,
		wallet_cache.as_deref(),
		replay_checkpoint_interval,
	)
	.await
	{
		ForkAwareLedgerContext::Ledger8(ctx) => {
			ledger_8::contract_state::get_contract_state(&ctx, address).await
		},
		ForkAwareLedgerContext::Ledger9(ctx) => {
			ledger_9::contract_state::get_contract_state(&ctx, address).await
		},
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::tx_generator::source::FetchCacheConfig;

	fn res(path: &str) -> String {
		format!("{}/../../res/{path}", env!("CARGO_MANIFEST_DIR"))
	}

	fn replay_source() -> Source {
		Source {
			src_url: None,
			fetch_concurrency: 1,
			fetch_compute_concurrency: None,
			src_files: Some(vec![
				res("genesis/genesis_block_undeployed.mn"),
				res("test-contract/contract_tx_1_deploy_undeployed.mn"),
			]),
			dust_warp: true,
			ignore_block_context: false,
			fetch_only_cached: false,
			fetch_cache: FetchCacheConfig::InMemory,
			ledger_state_db: String::new(),
			replay_checkpoint_interval: 0,
			indexer_url: None,
			network: "undeployed".to_string(),
			#[cfg(feature = "indexer-client")]
			indexer_concurrency:
				midnight_ledger_unsafe_helpers::indexer_client::DEFAULT_WALLET_SYNC_CONCURRENCY,
		}
	}

	#[tokio::test]
	async fn reads_deployed_contract_and_errors_on_unknown() {
		let deployed = cli::contract_address_decode(include_str!(
			"../../../../res/test-contract/contract_address_undeployed.mn"
		))
		.unwrap();
		let state = contract_state(replay_source(), deployed).await.expect("deployed contract");
		assert!(!state.is_empty());

		let unknown = cli::contract_address_decode(&"00".repeat(32)).unwrap();
		let err = contract_state(replay_source(), unknown).await.unwrap_err();
		assert!(err.to_string().contains("does not exist"), "{err}");
	}
}

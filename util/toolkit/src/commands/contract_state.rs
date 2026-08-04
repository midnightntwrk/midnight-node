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
use crate::tx_generator::builder::build_fork_aware_context_cached;
use crate::tx_generator::source::create_file_wallet_cache;
use clap::Args;
use midnight_node_ledger_helpers::ContractAddress;
use std::{fs, path::Path};

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

pub async fn execute(
	args: ContractStateArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let ledger_state_db = args.source.ledger_state_db.clone();
	let fetch_cache = args.source.fetch_cache.clone();
	let source = TxGenerator::source(args.source, args.dry_run)
		.await
		.expect("failed to init tx source");

	if args.dry_run {
		log::info!("Dry-run: fetch contract state for address: {:?}", args.contract_address);
		log::info!("Dry-run: write contract state to file: {:?}", args.dest_file);
		return Ok(());
	}

	let blocks = source.get_txs().await?;
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	let fork_ctx = build_fork_aware_context_cached(&[], &blocks, wallet_cache.as_deref()).await;

	let serialized_state = fork_ctx.dispatch(
		|ctx| {
			crate::commands::fork::ledger_7::contract_state::get_contract_state(
				&ctx,
				args.contract_address,
			)
		},
		|ctx| {
			crate::commands::fork::ledger_8::contract_state::get_contract_state(
				&ctx,
				args.contract_address,
			)
		},
		|ctx| {
			crate::commands::fork::ledger_9::contract_state::get_contract_state(
				&ctx,
				args.contract_address,
			)
		},
	)?;

	if let Some(dest_file) = &args.dest_file {
		let full_path = Path::new(dest_file);
		if let Some(directory) = full_path.parent() {
			fs::create_dir_all(directory).expect("failed to create directories");
		}

		fs::write(full_path, serialized_state).expect("failed to create file");
	}

	Ok(())
}

#[cfg(test)]
mod test {
	// TODO
}

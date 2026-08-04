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

use crate::tx_generator::builder::build_fork_aware_context_cached;
use crate::tx_generator::source::{GetTxs, GetTxsFromUrl, create_file_wallet_cache};
use crate::{
	WalletSeed,
	cli_parsers::{self as cli},
	serde_def::SourceTransactions,
	source::Source,
};
use clap::Args;

#[derive(Args)]
pub struct FetchArgs {
	#[command(flatten)]
	src: Source,
	/// Wallet seeds to pre-cache during fetch. Accepts multiple values.
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seeds: Option<Vec<WalletSeed>>,
}

pub async fn execute(args: FetchArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let FetchArgs { src, seeds } = args;

	if src.src_files.is_some() {
		panic!("error: fetch command doesn't work with '--src-files'");
	}

	let ledger_state_db = src.ledger_state_db.clone();
	let fetch_cache = src.fetch_cache.clone();

	let start = std::time::Instant::now();
	let txs: SourceTransactions = GetTxsFromUrl::new(
		&src.src_url.unwrap(),
		src.fetch_concurrency,
		src.fetch_compute_concurrency
			.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get())),
		src.dust_warp,
		src.fetch_only_cached,
		src.fetch_cache,
	)
	.get_txs()
	.await?;
	log::info!("fetched {} blocks in {:.3} s", txs.blocks.len(), start.elapsed().as_secs_f32());

	if let Some(seeds) = seeds {
		let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);
		let t = std::time::Instant::now();
		let _ctx = build_fork_aware_context_cached(&seeds, &txs, wallet_cache.as_deref()).await;
		log::info!(
			"built wallet state cache for {} seeds in {:.3} s",
			seeds.len(),
			t.elapsed().as_secs_f32()
		);
	}

	Ok(())
}

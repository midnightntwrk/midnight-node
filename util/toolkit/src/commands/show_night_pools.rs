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

//! Dump the NIGHT pools (Reserved / Locked / Unlocked) held in a network's `LedgerState`.

use crate::commands::fork::{ledger_7, ledger_8, ledger_9};
use crate::source::Source;
use crate::tx_generator::builder::build_fork_aware_context_cached;
use crate::tx_generator::source::create_file_wallet_cache;
use crate::{TxGenerator, WalletSeed};

use clap::Args;

const STARS_PER_NIGHT: u128 = 1_000_000;

/// NIGHT pools and balances, all in atomic Stars (1 NIGHT = 1_000_000 Stars).
/// Shared, version-agnostic shape; the per-version extractors in
/// `commands::fork::*::night_pools` fill it in.
#[derive(Debug, Clone, Copy)]
pub struct NightPools {
	pub reserve: u128,
	pub locked: u128,
	pub treasury: u128,
	pub block_reward: u128,
	pub unclaimed: u128,
	pub utxo: u128,
	pub contract: u128,
}

#[derive(Args)]
pub struct ShowNightPoolsArgs {
	#[command(flatten)]
	pub source: Source,
}

/// Render an amount in atomic Stars as a human-readable NIGHT value.
fn night(stars: u128) -> String {
	format!("{}.{:06} NIGHT", stars / STARS_PER_NIGHT, stars % STARS_PER_NIGHT)
}

pub async fn execute(
	args: ShowNightPoolsArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let ledger_state_db = args.source.ledger_state_db.clone();
	let fetch_cache = args.source.fetch_cache.clone();

	let src = TxGenerator::source(args.source, false).await?;
	let source_txs = src.get_txs().await?;
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	// We track no wallet, but an empty seed list makes `build_fork_aware_context_cached`
	// skip the snapshot cache and replay from genesis every run, so we pass one throwaway
	// seed to keep `--ledger-state-db` effective. The dummy wallet never affects the global
	// pools we read, so an all-zero seed is safe despite the `Default` note on `WalletSeed`.
	let cache_seed = [WalletSeed::Medium([0u8; 32])];
	let fork_ctx =
		build_fork_aware_context_cached(&cache_seed, &source_txs, wallet_cache.as_deref()).await;

	let night_pools = fork_ctx.dispatch(
		|ctx| ledger_7::night_pools::night_pools(&ctx),
		|ctx| ledger_8::night_pools::night_pools(&ctx),
		|ctx| ledger_9::night_pools::night_pools(&ctx),
	)?;

	let total = night_pools.reserve
		+ night_pools.locked
		+ night_pools.treasury
		+ night_pools.block_reward
		+ night_pools.unclaimed
		+ night_pools.utxo
		+ night_pools.contract;

	println!("reserve_pool   (Reserved): {}", night(night_pools.reserve));
	println!("locked_pool    (Locked):   {}", night(night_pools.locked));
	println!("treasury NIGHT (Unlocked): {}", night(night_pools.treasury));
	println!("--- full supply breakdown ---");
	println!("block_reward_pool:         {}", night(night_pools.block_reward));
	println!("unclaimed_block_rewards:   {}", night(night_pools.unclaimed));
	println!("utxos:                     {}", night(night_pools.utxo));
	println!("contracts:                 {}", night(night_pools.contract));
	println!("TOTAL (should be 24B):     {}", night(total));

	Ok(())
}

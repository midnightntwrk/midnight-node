use std::collections::HashMap;

use crate::tx_generator::builder::build_fork_aware_context_cached;
use crate::tx_generator::source::create_file_wallet_cache;
use crate::{TxGenerator, WalletSeed, source::Source};
use crate::{
	cli_parsers::{self as cli},
	serde_def::{DustGenerationInfoSer, QualifiedDustOutputSer},
};
use clap::Args;

#[derive(Args)]
pub struct DustBalanceArgs {
	#[command(flatten)]
	pub source: Source,
	/// The seed of the wallet to show wallet state for, including private state
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	pub seed: WalletSeed,
	/// Dry-run - don't fetch wallet state, just print out settings
	#[arg(long)]
	pub dry_run: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct GenerationInfoPair {
	pub dust_output: QualifiedDustOutputSer,
	pub generation_info: Option<DustGenerationInfoSer>,
}

#[derive(Debug, serde::Serialize)]
pub struct DustBalanceJson {
	pub generation_infos: Vec<GenerationInfoPair>,
	pub source: HashMap<String, u128>,
	pub total: u128,
	pub capacity: u128,
}

pub enum DustBalanceResult {
	Json(DustBalanceJson),
	DryRun(()),
}

pub async fn execute(
	args: DustBalanceArgs,
) -> Result<DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
	let ledger_state_db = args.source.ledger_state_db.clone();
	let fetch_cache = args.source.fetch_cache.clone();
	let src = TxGenerator::source(args.source, args.dry_run).await?;

	if args.dry_run {
		log::info!("Dry-run: fetching wallet for seed {:?}", args.seed);
		return Ok(DustBalanceResult::DryRun(()));
	}

	let source_blocks = src.get_txs().await?;
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	let fork_ctx =
		build_fork_aware_context_cached(&[args.seed], &source_blocks, wallet_cache.as_deref())
			.await;

	let json = fork_ctx.dispatch(
		|ctx| {
			let seed_v7 =
				crate::tx_generator::builder::builders::ledger_7::type_convert::convert_wallet_seed(
					args.seed,
				);
			crate::commands::fork::ledger_7::dust_balance::dust_balance(&ctx, seed_v7)
		},
		|ctx| crate::commands::fork::ledger_8::dust_balance::dust_balance(&ctx, args.seed),
	)?;

	Ok(DustBalanceResult::Json(json))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tx_generator::source::FetchCacheConfig;
	use test_case::test_case;

	/// Test data
	fn td(filepath: &str) -> String {
		[env!("CARGO_MANIFEST_DIR"), "/test-data/", &filepath].concat().to_string()
	}

	#[test_case("0000000000000000000000000000000000000000000000000000000000000001", vec![td("genesis/genesis_block_undeployed.mn")]; "when using seed 01")]
	#[tokio::test]
	async fn check_balance_non_zero(seed: &str, src_files: Vec<String>) {
		let seed = WalletSeed::try_from_hex_str(seed).unwrap();
		let args = DustBalanceArgs {
			source: Source {
				src_url: None,
				fetch_concurrency: 1,
				fetch_compute_concurrency: None,
				src_files: Some(src_files),
				dust_warp: true,
				ignore_block_context: false,
				fetch_only_cached: false,
				fetch_cache: FetchCacheConfig::InMemory,
				ledger_state_db: String::new(),
			},
			seed,
			dry_run: false,
		};

		let res = execute(args).await.expect("result was not Ok");

		assert!(
			matches!(res, DustBalanceResult::Json( DustBalanceJson { total, .. }) if total > 0 )
		);
	}
}

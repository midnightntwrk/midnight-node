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

/// Batched form of [`DustBalanceArgs`]. See [`execute_many`].
pub struct DustBalanceManyArgs {
	pub source: Source,
	pub seeds: Vec<WalletSeed>,
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

/// Single-seed entry point. Thin wrapper around [`execute_many`] — see that
/// function for the batched form that amortises chain replay across multiple
/// seeds in one pass.
pub async fn execute(
	args: DustBalanceArgs,
) -> Result<DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
	let many =
		DustBalanceManyArgs { source: args.source, seeds: vec![args.seed], dry_run: args.dry_run };
	let mut results = execute_many(many).await?;
	// `execute_many` returns one result per input seed; we passed exactly one.
	Ok(results.pop().expect("execute_many with one seed must return one result").1)
}

/// Batched dust-balance: run one shared block replay across all `seeds` and
/// return per-seed results in input order.
///
/// One batched call is the only way to amortise the genesis-replay cost across
/// multiple uncached seeds — calling [`execute`] N times always replays from
/// genesis N times when all seeds are uncached, because
/// `build_fork_aware_context_cached` only short-circuits when *every* requested
/// seed has a cached wallet snapshot.
///
/// Empty `seeds` returns `Ok(vec![])` without touching the source. `dry_run`
/// returns one [`DustBalanceResult::DryRun`] per seed and performs no fetch.
pub async fn execute_many(
	args: DustBalanceManyArgs,
) -> Result<Vec<(WalletSeed, DustBalanceResult)>, Box<dyn std::error::Error + Send + Sync>> {
	if args.seeds.is_empty() {
		return Ok(Vec::new());
	}

	// Construct the source eagerly so that source-argument validation
	// (`SourceError::InvalidSourceArgs`) runs *before* the dry-run short
	// circuit. Without this, `dry_run = true` would silently succeed even
	// with an unusable source config, defeating dry-run as a preflight
	// check.
	let ledger_state_db = args.source.ledger_state_db.clone();
	let fetch_cache = args.source.fetch_cache.clone();
	let src = TxGenerator::source(args.source, args.dry_run).await?;

	if args.dry_run {
		log::info!("Dry-run: fetching wallet state for {} seed(s)", args.seeds.len());
		return Ok(args.seeds.into_iter().map(|s| (s, DustBalanceResult::DryRun(()))).collect());
	}

	let source_blocks = src.get_txs().await?;
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	let fork_ctx =
		build_fork_aware_context_cached(&args.seeds, &source_blocks, wallet_cache.as_deref()).await;

	// `dispatch` consumes the context, so iterate the seeds *inside* the
	// closure: one context, N per-seed balance extractions.
	let seeds = args.seeds;
	let jsons: Vec<DustBalanceJson> = fork_ctx.dispatch(
		|ctx| {
			seeds
				.iter()
				.map(|seed| {
					let seed_v7 = crate::tx_generator::builder::builders::ledger_7::type_convert::convert_wallet_seed(
						seed.clone(),
					);
					crate::commands::fork::ledger_7::dust_balance::dust_balance(&ctx, seed_v7)
				})
				.collect::<Result<Vec<_>, _>>()
		},
		|ctx| {
			seeds
				.iter()
				.map(|seed| {
					crate::commands::fork::ledger_8::dust_balance::dust_balance(&ctx, seed.clone())
				})
				.collect::<Result<Vec<_>, _>>()
		},
	)?;

	Ok(seeds
		.into_iter()
		.zip(jsons)
		.map(|(seed, json)| (seed, DustBalanceResult::Json(json)))
		.collect())
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

	fn source_for(src_files: Vec<String>) -> Source {
		Source {
			src_url: None,
			fetch_concurrency: 1,
			fetch_compute_concurrency: None,
			src_files: Some(src_files),
			dust_warp: true,
			ignore_block_context: false,
			fetch_only_cached: false,
			fetch_cache: FetchCacheConfig::InMemory,
			ledger_state_db: String::new(),
		}
	}

	#[test_case("0000000000000000000000000000000000000000000000000000000000000001", vec![td("genesis/genesis_block_undeployed.mn")]; "when using seed 01")]
	#[tokio::test]
	async fn check_balance_non_zero(seed: &str, src_files: Vec<String>) {
		let seed = WalletSeed::try_from_hex_str(seed).unwrap();
		let args = DustBalanceArgs { source: source_for(src_files), seed, dry_run: false };

		let res = execute(args).await.expect("result was not Ok");

		assert!(matches!(res, DustBalanceResult::Json(DustBalanceJson { total, .. }) if total > 0));
	}

	#[tokio::test]
	async fn check_balance_many_returns_in_input_order() {
		// Two seeds against the same genesis fixture. The first is the
		// known funded seed (matches `check_balance_non_zero`); the second
		// has no balance. We assert that:
		//   - the result vector length matches the input,
		//   - results come back in the order the seeds were supplied,
		//   - the funded seed reports a non-zero balance.
		let seeds = vec![
			WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000001",
			)
			.unwrap(),
			WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000002",
			)
			.unwrap(),
		];
		let args = DustBalanceManyArgs {
			source: source_for(vec![td("genesis/genesis_block_undeployed.mn")]),
			seeds: seeds.clone(),
			dry_run: false,
		};

		let res = execute_many(args).await.expect("result was not Ok");

		assert_eq!(res.len(), 2, "expected one result per input seed");
		assert_eq!(&res[0].0, &seeds[0], "results should be in input order");
		assert_eq!(&res[1].0, &seeds[1], "results should be in input order");
		assert!(
			matches!(&res[0].1, DustBalanceResult::Json(DustBalanceJson { total, .. }) if *total > 0),
			"seed 01 should have a non-zero balance"
		);
	}

	#[tokio::test]
	async fn check_balance_many_empty_seeds_is_noop() {
		let args = DustBalanceManyArgs {
			source: source_for(vec![td("genesis/genesis_block_undeployed.mn")]),
			seeds: Vec::new(),
			dry_run: false,
		};
		let res = execute_many(args).await.expect("result was not Ok");
		assert!(res.is_empty(), "empty seeds should yield empty result without touching source");
	}

	#[tokio::test]
	async fn check_balance_many_dry_run_skips_fetch() {
		let seeds = vec![
			WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000001",
			)
			.unwrap(),
			WalletSeed::try_from_hex_str(
				"0000000000000000000000000000000000000000000000000000000000000002",
			)
			.unwrap(),
		];
		let args = DustBalanceManyArgs {
			source: source_for(vec![td("genesis/genesis_block_undeployed.mn")]),
			seeds: seeds.clone(),
			dry_run: true,
		};
		let res = execute_many(args).await.expect("result was not Ok");
		assert_eq!(res.len(), seeds.len(), "dry-run yields one DryRun per input seed");
		for (i, (seed, result)) in res.iter().enumerate() {
			assert_eq!(seed, &seeds[i], "dry-run preserves input order");
			assert!(
				matches!(result, DustBalanceResult::DryRun(())),
				"dry-run must not produce a Json result"
			);
		}
	}
}

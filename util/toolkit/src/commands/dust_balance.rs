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
    				let seed_v8 = crate::tx_generator::builder::builders::ledger_8::type_convert::convert_wallet_seed(
    					seed.clone(),
    				);
					crate::commands::fork::ledger_8::dust_balance::dust_balance(&ctx, seed_v8)
				})
				.collect::<Result<Vec<_>, _>>()
		},
		|ctx| {
		    seeds
				.iter()
				.map(|seed| {
				    crate::commands::fork::ledger_9::dust_balance::dust_balance(&ctx, seed.clone())
				})
				.collect::<Result<Vec<_>, _>>()
		}
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

	/// Regression test for the multi-seed (and dust_warp) cache save bug.
	///
	/// Before the fix: `build_fork_aware_context_cached` used `blocks.last()`
	/// to determine the height to save the wallet/ledger snapshot at, but
	/// `SourceTransactions::from_blocks(_, dust_warp = true, _)` appends a
	/// synthetic timestamp-only block with `number = 0` at the end. The
	/// cache was therefore saved with `block_height = 0` even when the
	/// inner state had been replayed all the way to the chain head. On the
	/// next run the snapshot was loaded and replayed starting at block 0,
	/// re-inserting dust events into an already-full generation tree and
	/// panicking with a non-linear-insertion error.
	///
	/// The test drives `build_fork_aware_context_cached` directly (not via
	/// `execute`) because file-loaded `SourceTransactions` go through
	/// `from_batches` -> `new_from_timestamp`, which hardcodes
	/// `RawBlockData::number = 0` for every block. With every block at
	/// height 0, `SourceTransactions::chain_id()` returns `None` and the
	/// builder short-circuits to the non-caching `_raw` path before any
	/// snapshot save / load runs. Renumbering the real blocks 1..N here
	/// makes `chain_id()` resolve and forces both calls through the cache
	/// path that the fix is meant to protect.
	///
	/// Pins five invariants:
	///   1. After call 1, the wallet cache holds a snapshot tagged at the
	///      real chain head (`max(block.number)`), not at the synthetic
	///      dust-warp block's `number = 0`.
	///   2. No snapshot is ever saved at `block_height = 0` — the negative
	///      version of (1), since under the old bug that's where the
	///      snapshot lived.
	///   3. After call 2 (warm restore), the in-memory
	///      `latest_block_context.tblock` is wall-clock "now" — the
	///      synthetic dust-warp block must be re-applied after warm restore
	///      so downstream callers (`register_dust_address`, batch builders)
	///      don't read a stale timestamp out of the restored snapshot.
	///   4. The persisted snapshot's `latest_block_context.tblock_secs`
	///      equals the *real-head block's* `tblock_secs`, NOT the synthetic
	///      dust-warp block's wall-clock value. Persisting wall-clock-now
	///      under the real-head height would surface as a silent warp-leak
	///      on a later `dust_warp = false` run against the same
	///      `ledger_state_db` (the restored snapshot would feed
	///      wall-clock-now to downstream callers even though warping was
	///      disabled).
	///   5. The warm-restore second call must not re-persist the warp
	///      either — re-checking the snapshot after the second call
	///      confirms the post-save warp re-apply stays in-memory only.
	#[tokio::test]
	async fn check_balance_caches_at_real_head_with_dust_warp() {
		// `WalletStateCaching` methods (`get_latest_ledger_height`,
		// `get_ledger_snapshot`) come into scope via the `&dyn
		// WalletStateCaching` trait object type that `create_file_wallet_cache`
		// returns — no explicit `use` import needed for method dispatch.
		use midnight_node_ledger_helpers::fork::fork_aware_context::ForkAwareLedgerContext;
		use std::time::{SystemTime, UNIX_EPOCH};

		let tempdir = tempfile::tempdir().expect("create tempdir for cache");
		let seed_hex = "0000000000000000000000000000000000000000000000000000000000000001";
		let src_files = vec![td("genesis/genesis_block_undeployed.mn")];
		let fetch_cache_path = tempdir.path().join("fetch_cache.db");
		let ledger_state_db = tempdir.path().join("ledger_cache_db");
		let fetch_cache_cfg =
			FetchCacheConfig::Redb { filename: fetch_cache_path.to_string_lossy().into_owned() };
		let ledger_state_db_str = ledger_state_db.to_string_lossy().into_owned();

		let source = Source {
			src_url: None,
			fetch_concurrency: 1,
			fetch_compute_concurrency: None,
			src_files: Some(src_files.clone()),
			dust_warp: true,
			ignore_block_context: false,
			fetch_only_cached: false,
			fetch_cache: fetch_cache_cfg.clone(),
			ledger_state_db: ledger_state_db_str.clone(),
		};
		let src = TxGenerator::source(source, false).await.expect("build source");
		let mut source_blocks = src.get_txs().await.expect("get_txs");

		// The fixture is loaded with dust_warp=true so the last block is the
		// synthetic warp marker. Renumber the real blocks 1..N (leaving the
		// synthetic at number=0) so chain_id() returns Some and both
		// execute() calls go through the cache path.
		assert!(!source_blocks.blocks.is_empty(), "fixture must produce >= 1 block");
		let real_count = source_blocks.blocks.len() - 1;
		assert!(
			real_count >= 1,
			"fixture must produce >= 1 real block alongside the dust-warp synthetic",
		);
		for (i, block) in source_blocks.blocks.iter_mut().take(real_count).enumerate() {
			block.number = (i + 1) as u64;
			// `chain_id()` keys on `blocks[0].hash`; give every real block a
			// stable non-zero hash so the cache key is deterministic across
			// the two calls.
			block.hash = [(i + 1) as u8; 32];
		}
		let chain_id = source_blocks.chain_id().expect("chain_id should resolve after renumber");
		let real_head =
			source_blocks.blocks.iter().map(|b| b.number).max().expect("blocks non-empty");
		assert_eq!(real_head, real_count as u64, "real head must be N (the synthetic stays at 0)");

		let seeds = vec![WalletSeed::try_from_hex_str(seed_hex).unwrap()];

		let wallet_cache = create_file_wallet_cache(&ledger_state_db_str, &fetch_cache_cfg);
		let storage = wallet_cache.as_deref().expect("file wallet cache must be Some");

		// First call: cold cache, full replay, must persist a snapshot at
		// the real chain head.
		let test_start_secs =
			SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_secs();
		let _ = build_fork_aware_context_cached(&seeds, &source_blocks, Some(storage)).await;

		// Invariant (1) + (2): snapshot tagged at real head, never at 0.
		let latest = storage.get_latest_ledger_height(chain_id).await;
		assert_eq!(
			latest,
			Some(real_head),
			"snapshot must be tagged at the real chain head ({}), not at the synthetic \
			 dust-warp block's number=0",
			real_head,
		);
		assert!(
			storage.get_ledger_snapshot(chain_id, 0).await.is_none(),
			"no snapshot must be stored at block_height = 0",
		);

		// Invariant (4): the persisted snapshot must carry the real-head
		// block's `tblock_secs`, NOT the synthetic dust-warp block's
		// wall-clock tblock. Persisting the warp under the real-head
		// height would surface as a silent warp-leak on a later
		// `dust_warp = false` run against the same `ledger_state_db`:
		// the restored snapshot would feed wall-clock-now to
		// `register_dust_address` / batch builders even though warping
		// was disabled.
		let snapshot = storage
			.get_ledger_snapshot(chain_id, real_head)
			.await
			.expect("snapshot must exist at real head");
		let real_head_block = source_blocks
			.blocks
			.iter()
			.find(|b| b.number == real_head)
			.expect("real head block must be in source");
		assert_eq!(
			snapshot.latest_block_context.tblock_secs, real_head_block.tblock_secs,
			"snapshot.latest_block_context.tblock_secs must equal the real-head \
			 block's tblock_secs (the dust-warp synthetic must NOT be applied \
			 before save); test_start={test_start_secs}",
		);
		assert!(
			snapshot.latest_block_context.tblock_secs < test_start_secs,
			"snapshot tblock_secs ({}) should pre-date the test start \
			 ({test_start_secs}) — the fixture's chain timestamps are historic; \
			 a value >= test_start would mean the dust-warp synthetic leaked \
			 into the persisted snapshot",
			snapshot.latest_block_context.tblock_secs,
		);

		// Second call: warm restore, must not panic and must apply the
		// warp in-memory only.
		let fork_ctx_2 =
			build_fork_aware_context_cached(&seeds, &source_blocks, Some(storage)).await;

		// Invariant (3): in-memory warp re-applied on warm restore. The
		// warm-path filter drops the synthetic (number=0 ≤ start_height),
		// so without the post-save re-apply step the in-memory tblock
		// would be the real-head block's historic tblock instead of
		// wall-clock-now.
		match fork_ctx_2 {
			ForkAwareLedgerContext::Ledger9(ctx) => {
				let ctx_tblock = ctx.latest_block_context().tblock.to_secs();
				assert!(
					ctx_tblock >= test_start_secs,
					"in-memory latest_block_context.tblock should be >= test start \
					 ({test_start_secs}) after warm restore with dust_warp=true; \
					 got {ctx_tblock}",
				);
			},
			ForkAwareLedgerContext::Ledger7(_) | ForkAwareLedgerContext::Ledger8(_) => {
				panic!("post-fork context should be on Ledger9 after replay")
			},
		}

		// Invariant (5): the second call must not have re-persisted the
		// warp under the real-head height. Re-check the snapshot after
		// the warm-restore path runs end-to-end.
		let snapshot_after_warm = storage
			.get_ledger_snapshot(chain_id, real_head)
			.await
			.expect("snapshot must still exist after warm restore");
		assert_eq!(
			snapshot_after_warm.latest_block_context.tblock_secs, real_head_block.tblock_secs,
			"snapshot tblock_secs must remain at real-head value after warm \
			 restore — the post-save warp re-apply must not leak into the \
			 persisted state",
		);
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

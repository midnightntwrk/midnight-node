use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;

use crate::wait_before_deploying;

/// PR367-TC-0003-03 E2E: Valid Transaction Succeeds
///
/// Confirms no regression - valid transactions should still be accepted.
/// Note: This test requires a fresh node state where the contract hasn't been deployed.
#[e2e_test]
#[ignore = "Requires fresh node state - run manually with cargo test-e2e-local"]
async fn valid_deploy_transaction_succeeds_via_rpc() {
    use midnight_node_res::undeployed::transactions::DEPLOY_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    tracing::info!("=== PR367-TC-0003-03 E2E: Valid Transaction Test ===");
    let _deploy_guard = wait_before_deploying().await;
    tracing::info!("Submitting valid DEPLOY_TX...");

    let result = client.submit_expecting_success(DEPLOY_TX.to_vec()).await;

    assert!(
        result.is_ok(),
        "Valid DEPLOY_TX should be accepted, but was rejected: {:?}",
        result.err()
    );

    tracing::info!(
        "✓ PR367-TC-0003-03 E2E PASSED: Valid transaction accepted and included in block"
    );
}

/// One-shot cleanup task: consolidates fragmented UTXOs at the funded faucet
/// address. Useful when the faucet has accumulated many small change UTXOs
/// from prior runs. Opt-in via `cargo test --ignored consolidate_faucet`.
#[e2e_test]
#[ignore = "operational task: opt-in with `cargo test --ignored consolidate_faucet`"]
async fn consolidate_faucet() {
    let settings = Settings::default();
    let faucet = CardanoClient::new_from_funded(settings.ogmios_client, settings.constants).await;
    let tx_ids = faucet
        .consolidate_utxos(50)
        .await
        .expect("Failed to consolidate faucet UTXOs");
    tracing::info!("Submitted {} consolidation transactions", tx_ids.len());
}

/// Wiring smoke test for the Postgres-backed toolkit fetch cache.
/// Calls dust_balance for a deterministic seed (32-zero-bytes + 1) against
/// the qanet RPC, using `crate::fetch_cache_config()` so the Postgres
/// branch is exercised. Expects `Ok(_)` — that proves the Postgres
/// connection, fetch+cache write path, and dust derivation all work.
/// Opt-in via `cargo test --ignored dust_balance_smoke`.
#[cfg(feature = "qanet")]
#[e2e_test(flavor = "multi_thread", worker_threads = 16)]
#[ignore = "wiring smoke test for Postgres-backed fetch cache; \
            opt-in with `cargo test --ignored dust_balance_smoke`"]
async fn dust_balance_smoke() {
    use midnight_node_ledger_helpers::WalletSeed;
    use midnight_node_toolkit::commands::dust_balance::{self, DustBalanceArgs};
    use midnight_node_toolkit::tx_generator::source::Source;

    let settings = Settings::default();
    let seed = WalletSeed::try_from_hex_str(
        "0000000000000000000000000000000000000000000000000000000000000001",
    )
    .expect("seed parses");

    let args = DustBalanceArgs {
        source: Source {
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            fetch_compute_concurrency: None,
            src_files: None,
            dust_warp: true,
            ignore_block_context: false,
            fetch_only_cached: false,
            fetch_cache: crate::fetch_cache_config(),
            ledger_state_db: String::new(),
        },
        seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args).await;
    assert!(result.is_ok(), "dust_balance failed: {:?}", result.err());
    tracing::info!("dust_balance returned Ok — Postgres cache wiring is healthy");
}

/// Wiring smoke test for `dust_balance::execute_many` against qanet.
///
/// Runs one shared replay across 3 deterministic seeds, asserts the
/// result vector matches input order, and writes wallet snapshots into
/// `toolkit_cache/ledger_cache_db/`. A second back-to-back invocation
/// should return in seconds via the warm cache — that's the
/// end-to-end regression case for issue #1573 (toolkit cache tagged at
/// `block_height = 0` under `dust_warp = true`, fixed in PR #1574).
///
/// Uses `flavor = "multi_thread"` so the toolkit's tokio fetcher pipeline
/// gets the same multi-threaded runtime as the CLI's `tokio::main`.
/// `#[tokio::test]`'s default `current_thread` flavor visibly bottlenecks
/// the I/O side on a single OS thread.
///
/// Opt-in via `cargo test --ignored dust_balance_smoke_many`.
#[cfg(feature = "qanet")]
#[e2e_test(flavor = "multi_thread", worker_threads = 16)]
#[ignore = "wiring smoke test for batched dust_balance; \
            opt-in with `cargo test --ignored dust_balance_smoke_many`"]
async fn dust_balance_smoke_many() {
    use midnight_node_ledger_helpers::WalletSeed;
    use midnight_node_toolkit::commands::dust_balance::{
        self, DustBalanceJson, DustBalanceManyArgs, DustBalanceResult,
    };
    use midnight_node_toolkit::tx_generator::source::Source;

    let settings = Settings::default();
    let seeds: Vec<WalletSeed> = [
        "0000000000000000000000000000000000000000000000000000000000000001",
        "0000000000000000000000000000000000000000000000000000000000000002",
        "0000000000000000000000000000000000000000000000000000000000000003",
    ]
    .iter()
    .map(|s| WalletSeed::try_from_hex_str(s).expect("seed parses"))
    .collect();

    // Anchor to the e2e crate dir so the cache lives at a stable absolute
    // path regardless of cargo's CWD (cargo test default is the package
    // dir; manual runs from the repo root would otherwise land at a
    // different relative path).
    let ledger_state_db = format!(
        "{}/toolkit_cache/ledger_cache_db",
        env!("CARGO_MANIFEST_DIR")
    );

    let args = DustBalanceManyArgs {
        source: Source {
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            fetch_compute_concurrency: None,
            src_files: None,
            // false for live-chain use: per the CLI help, dust_warp
            // "may result in invalid proofs when connected to a live
            // chain". Concretely it appends a system-time block at
            // chain end that mutates state in a way that doesn't match
            // the actual block 1088189's state root, so any subsequent
            // run that restores from the saved snapshot fails
            // `StateRootMismatch` when applying the next delta block.
            // dust_warp is only meaningful when sourcing from a genesis
            // file fixture where the chain has no further blocks to
            // accumulate time naturally.
            dust_warp: false,
            ignore_block_context: false,
            fetch_only_cached: false,
            fetch_cache: crate::fetch_cache_config(),
            ledger_state_db,
        },
        seeds: seeds.clone(),
        dry_run: false,
    };

    let started = std::time::Instant::now();
    let results = dust_balance::execute_many(args)
        .await
        .expect("execute_many failed");
    let elapsed = started.elapsed();

    assert_eq!(
        results.len(),
        seeds.len(),
        "expected one result per input seed"
    );
    for (i, (seed, result)) in results.iter().enumerate() {
        assert_eq!(seed, &seeds[i], "result {i} out of order");
        assert!(
            matches!(result, DustBalanceResult::Json(DustBalanceJson { .. })),
            "result {i} is not Json"
        );
    }
    tracing::info!(
        "execute_many: {} seed(s), shared replay in {:?} — Postgres wiring healthy",
        seeds.len(),
        elapsed
    );
}

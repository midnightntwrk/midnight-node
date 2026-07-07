use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;

/// PR367-TC-0003-03 E2E: Valid Transaction Succeeds
///
/// A well-formed contract deploy, built dynamically against the live chain and
/// funded by the bridge-funded dev wallet (0x..01), is accepted at the RPC and
/// included in a block.
#[e2e_test]
async fn valid_deploy_transaction_succeeds_via_rpc() {
    use midnight_node_e2e::api::midnight::MidnightClient;

    // Funded + DUST-registered at runtime by init-mnight-faucet; wait until ready.
    crate::ensure_dev_wallet_funded().await;
    // Coordinate with the pre-deploy quiescence gate (this submits a deploy) and
    // serialize against the other deploy tests that share dev wallet 0x..01.
    let _deploy_guard = crate::wait_before_deploying().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client.clone()).await;
    let url = settings.node_client.base_url.clone();

    // Builds a fresh deploy against current settled state and submits it,
    // rebuilding on transient shared-wallet DUST contention. Panics if it never
    // lands — proving a well-formed deploy is accepted + included (PR367-TC-0003-03).
    crate::deploy_and_confirm(&client, &url).await;
    tracing::info!("✓ valid DEPLOY_TX accepted and included");
}

/// Regression guard: the toolkit must not hang on exit when sending with
/// multiple `--dest-url` options (ported from the old
/// scripts/tests/toolkit-multi-dest-e2e.sh, which assumed the unfunded
/// undeployed genesis). Builds one unshielded self-transfer from the
/// bridge-funded dev wallet (0x..01) and sends it via `generate-txs send` with
/// several destination URLs, asserting the send completes within a timeout (a
/// hang would blow the timeout) and returns Ok.
///
/// The URLs deliberately repeat the one node RPC: the sender opens one client
/// per URL and must tear all of them down on exit — that N-client setup+teardown
/// is where the hang lived, and it's exercised regardless of how many txs flow.
/// One tx keeps it conflict-free (multiple dev-wallet txs sent concurrently would
/// collide on the wallet's single DUST UTxO).
#[e2e_test]
async fn toolkit_multi_dest_send_does_not_hang() {
    use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs};
    use midnight_node_toolkit::tx_generator::builder::Builder;
    use midnight_node_toolkit::tx_generator::destination::Destination;
    use midnight_node_toolkit::tx_generator::source::Source;
    use std::time::Duration;
    use tokio::time::timeout;

    crate::ensure_dev_wallet_funded().await;
    // Submits a real tx from 0x..01 — serialize against the other deploy tests
    // sharing that wallet (and wait out the pre-deploy quiescence gate).
    let _deploy_guard = crate::wait_before_deploying().await;

    let settings = Settings::default();
    let url = settings.node_client.base_url.clone();

    const N_DEST_URLS: usize = 3;
    const ATTEMPTS: u8 = 4;
    for attempt in 1..=ATTEMPTS {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let tx_file = tempdir.path().join("selftx.mn");
        // Rebuilt each attempt so the DUST spend is fresh against current state.
        crate::build_unshielded_self_transfer(&url, &tx_file).await;

        let send = generate_txs::execute(GenerateTxsArgs {
            builder: Builder::Send,
            source: Source {
                src_url: None,
                src_files: Some(vec![tx_file.to_string_lossy().into_owned()]),
                fetch_concurrency: crate::fetch_concurrency(),
                fetch_compute_concurrency: None,
                dust_warp: false,
                ignore_block_context: false,
                fetch_only_cached: false,
                fetch_cache: crate::fetch_cache_config(),
                ledger_state_db: String::new(),
            },
            destination: Destination {
                dest_urls: vec![url.clone(); N_DEST_URLS],
                rate: 2.0,
                dest_file: None,
                no_watch_progress: false,
            },
            proof_server: None,
            dry_run: false,
        });

        // A genuine hang blows the per-attempt timeout — fail fast on that, it's
        // the regression this test guards (never retried).
        let outcome = timeout(Duration::from_secs(120), send)
            .await
            .expect("toolkit `send` hung with multiple --dest-url (regression)");
        match outcome {
            Ok(()) => {
                tracing::info!("✓ multi-dest send completed without hanging");
                return;
            }
            // `send` returns a generic "destination tasks failed" that hides the
            // ledger code, so we can't classify the error — but the only expected
            // failure here is transient shared-wallet DUST contention, which a
            // rebuild against fresh state clears. Retry; surface the last error if
            // it never succeeds.
            Err(e) => {
                assert!(
                    attempt < ATTEMPTS,
                    "multi-dest send failed on all {ATTEMPTS} attempts: {e}"
                );
                tracing::warn!(
                    "multi-dest send attempt {attempt}/{ATTEMPTS} failed ({e}); \
                     rebuilding + resending after settle"
                );
                tokio::time::sleep(Duration::from_secs(8)).await;
            }
        }
    }
    unreachable!("multi-dest send loop returns or panics");
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

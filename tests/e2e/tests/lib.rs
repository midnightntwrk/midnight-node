use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::faucet::FaucetManager;
use midnight_node_ledger_helpers::WalletSeed;
use midnight_node_toolkit::commands::dust_balance;
use midnight_node_toolkit::tx_generator::source::{FetchCacheConfig, Source};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard, OnceCell};
use tokio::time::sleep;

// ============================================================================
// Pre-deploy / deploy ordering gate
// ============================================================================
//
// Some tests in this suite assert behaviour that depends on the test contract
// NOT being deployed yet ("pre-deploy" tests — e.g. RPC rejection on
// ContractNotPresent). The "deploy" tests actually submit DEPLOY_TX, after
// which the chain is permanently changed for everyone. Therefore every
// pre-deploy test must finish before any deploy test starts.
//
// Mechanism: each pre-deploy test holds a `PreDeployGuard` for its duration.
// The deploy gate (`wait_before_deploying`) waits until the entered/completed
// counters are at parity AND no counter change has happened for
// `PRE_DEPLOY_QUIESCENCE`, then proceeds. The gate adapts naturally to
// subset runs (`cargo test ... contract_state::`) where
// fewer pre-deploy tests are scheduled — we never hard-code the count.
//
// The gate refuses to open on `entered == 0`: there is no in-process way
// to distinguish "no pre-deploy tests in this run" from "pre-deploy tests
// are scheduled but haven't started yet" (e.g. under tight `--test-threads`
// or a reordered run). Opening on a timeout would be unsound — a deploy
// test could race ahead and mutate chain state before the pre-deploy
// tests assert against it.
//
// `E2E_SKIP_DEPLOY_GATE=1` is the explicit opt-out for subset runs that
// intentionally select only deploy tests — use it when you know no
// pre-deploy test is being scheduled.

/// Wait for `entered == completed` to stay stable for this long before
/// declaring pre-deploy tests done. Short enough to keep full runs snappy.
const PRE_DEPLOY_QUIESCENCE: Duration = Duration::from_secs(5);

/// Polling interval while a deploy test waits for the gate.
const PRE_DEPLOY_POLL: Duration = Duration::from_millis(200);

static PRE_DEPLOY_ENTERED: AtomicUsize = AtomicUsize::new(0);
static PRE_DEPLOY_COMPLETED: AtomicUsize = AtomicUsize::new(0);
static LAST_CHANGE_AT: Mutex<Option<Instant>> = Mutex::new(None);

// Deploy tests submit the same DEPLOY_TX, so concurrent submissions race in
// the txpool: one wins, the other gets "already imported", and pre_dispatch
// failures on the loser can ban the tx, leaving no live deployment.
// Serialize deploy tests behind this mutex so each runs to completion before
// the next starts.
static DEPLOY_SERIAL: LazyLock<AsyncMutex<()>> = LazyLock::new(|| AsyncMutex::new(()));

/// Marker held by a pre-deploy test for the duration of its body. Increments
/// `PRE_DEPLOY_ENTERED` on construction and `PRE_DEPLOY_COMPLETED` on drop,
/// so the gate's quiescence check sees the test arrive and leave even if
/// the body panics (Drop still runs during unwind).
///
/// ```ignore
/// #[e2e_test]
/// async fn my_pre_deploy_test() {
///     let _pre_deploy_guard = PreDeployGuard::new();
///     // ... assertions that depend on contract NOT being deployed ...
/// }
/// ```
#[must_use]
pub(crate) struct PreDeployGuard;

impl PreDeployGuard {
    pub(crate) fn new() -> Self {
        PRE_DEPLOY_ENTERED.fetch_add(1, Ordering::SeqCst);
        bump_last_change();
        Self
    }
}

impl Drop for PreDeployGuard {
    fn drop(&mut self) {
        PRE_DEPLOY_COMPLETED.fetch_add(1, Ordering::SeqCst);
        bump_last_change();
    }
}

fn bump_last_change() {
    *LAST_CHANGE_AT.lock().unwrap() = Some(Instant::now());
}

/// Block until every pre-deploy test in this run has finished, then take
/// the deploy serial mutex. See the module-level comment for semantics.
pub(crate) async fn wait_before_deploying() -> MutexGuard<'static, ()> {
    if std::env::var_os("E2E_SKIP_DEPLOY_GATE").is_none() {
        wait_for_pre_deploy_quiescence().await;
    }
    DEPLOY_SERIAL.lock().await
}

async fn wait_for_pre_deploy_quiescence() {
    loop {
        let entered = PRE_DEPLOY_ENTERED.load(Ordering::SeqCst);
        let completed = PRE_DEPLOY_COMPLETED.load(Ordering::SeqCst);
        let last_change = *LAST_CHANGE_AT.lock().unwrap();

        if entered > 0 && entered == completed {
            if let Some(t) = last_change {
                if Instant::now().duration_since(t) >= PRE_DEPLOY_QUIESCENCE {
                    tracing::info!(
                        "Deploy gate: {entered}/{entered} pre-deploy test(s) complete; proceeding",
                    );
                    return;
                }
            }
        }

        sleep(PRE_DEPLOY_POLL).await;
    }
}

// ============================================================================
// Toolkit wallet-cache warmup
// ============================================================================
//
// Every cNIGHT observation test ends with a per-seed `dust_balance::execute`
// against the live chain. With an empty `ledger_state_db` that call replays
// from genesis — ~1 h on Cardano Preview per seed. N tests × ~1 h serially
// blows past the GH Actions ceiling.
//
// The warmup amortises one shared replay across all test seeds, runs it in
// a dedicated background thread while the cNIGHT mints + stability +
// midnight-observation waits are happening, and writes the resulting
// wallet snapshots into a shared `toolkit_cache/ledger_cache_db/`. Each
// test's later `dust_balance::execute` reads from that path and restores
// from the warm snapshot in seconds.
//
// **v1 simplifying assumption: `cargo test --test-threads >= N`** (where
// N is the number of cNIGHT observation tests). All tests must start
// roughly in parallel so they register their seeds before the warmup
// quiesces; serial execution would mean the warmup fires after only the
// first seed registered, leaving the rest cold. The quiescence wait
// (`WARMUP_QUIESCENCE`) gives slow-starting tests a window to catch up.
// If you see "warmup: completed for K seed(s)" with K < your test count,
// raise `--test-threads` and/or `WARMUP_QUIESCENCE`.
//
// The warmup runs on a dedicated OS thread with its own current-thread
// tokio runtime. Each `#[tokio::test]` has its own per-test runtime that
// gets dropped at test end, which would kill any task spawned from inside
// it — the dedicated thread + runtime owns the warmup independently.

const WARMUP_QUIESCENCE: Duration = Duration::from_secs(30);
const WARMUP_POLL: Duration = Duration::from_secs(5);

static WARMUP_STATE: LazyLock<Mutex<WarmupState>> = LazyLock::new(|| {
    Mutex::new(WarmupState {
        seeds: Vec::new(),
        last_registration_at: None,
    })
});
static WARMUP_THREAD_STARTED: AtomicBool = AtomicBool::new(false);

struct WarmupState {
    seeds: Vec<WalletSeed>,
    last_registration_at: Option<Instant>,
}

/// Path the warmup task writes wallet snapshots to. The same path is
/// passed via `DustBalanceArgs::ledger_state_db` in each cNIGHT
/// observation test's post-stability `dust_balance::execute`, so per-test
/// calls restore from the warm cache instead of replaying from genesis.
pub(crate) fn warmup_ledger_state_db() -> String {
    format!(
        "{}/toolkit_cache/ledger_cache_db",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Register a wallet seed for the background warmup. Idempotent across
/// re-registrations of the same seed in the same process. Spawns the
/// warmup background thread on first call.
pub(crate) fn register_test_seed(seed: WalletSeed) {
    {
        let mut state = WARMUP_STATE.lock().expect("warmup state lock poisoned");
        state.seeds.push(seed);
        state.last_registration_at = Some(Instant::now());
        tracing::info!(
            "warmup: registered seed (total {} so far); warmup will fire \
             after {:?} of no new registrations",
            state.seeds.len(),
            WARMUP_QUIESCENCE,
        );
    }
    ensure_warmup_thread_started();
}

fn ensure_warmup_thread_started() {
    if WARMUP_THREAD_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::thread::Builder::new()
            .name("e2e-warmup".into())
            .spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build warmup runtime");
                rt.block_on(run_warmup());
            })
            .expect("spawn warmup thread");
    }
}

async fn run_warmup() {
    // Phase 1: wait for quiescence — all expected tests have registered
    // their seeds and no new ones have arrived for `WARMUP_QUIESCENCE`.
    loop {
        tokio::time::sleep(WARMUP_POLL).await;
        let state = WARMUP_STATE.lock().expect("warmup state lock poisoned");
        if let Some(t) = state.last_registration_at {
            if t.elapsed() >= WARMUP_QUIESCENCE {
                break;
            }
        }
    }

    let seeds = {
        let state = WARMUP_STATE.lock().expect("warmup state lock poisoned");
        state.seeds.clone()
    };
    if seeds.is_empty() {
        tracing::warn!("warmup: quiesced with zero seeds; skipping");
        return;
    }

    let settings = Settings::default();
    let args = dust_balance::DustBalanceManyArgs {
        source: Source {
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: fetch_concurrency(),
            fetch_compute_concurrency: None,
            src_files: None,
            // Live-chain warmup: appending a synthetic dust-warp block
            // here would fight the per-test `dust_balance::execute`
            // calls (which set dust_warp=true on their own and would
            // see a mismatched state root from the warmed snapshot).
            // The post-save warp re-apply step in
            // `build_fork_aware_context_cached` keeps the persisted
            // snapshot clean either way, but `false` is the right
            // semantics for a shared warmup against the live chain.
            dust_warp: false,
            ignore_block_context: false,
            fetch_only_cached: false,
            fetch_cache: fetch_cache_config(),
            ledger_state_db: warmup_ledger_state_db(),
        },
        seeds: seeds.clone(),
        dry_run: false,
    };

    tracing::info!(
        "warmup: quiesced after {:?}; running execute_many for {} seed(s) into {}",
        WARMUP_QUIESCENCE,
        seeds.len(),
        warmup_ledger_state_db(),
    );
    let started = Instant::now();
    match dust_balance::execute_many(args).await {
        Ok(results) => tracing::info!(
            "warmup: execute_many completed for {} seed(s) in {:?}",
            results.len(),
            started.elapsed()
        ),
        Err(e) => tracing::warn!(
            "warmup: execute_many failed after {:?}: {e} — per-test \
             `dust_balance::execute` calls will fall back to a full \
             genesis replay each",
            started.elapsed()
        ),
    }
}

// -------- DEV WALLET FUNDING GUARD --------

/// Seed of the local-env dev wallet funded on bring-up: the `init-mnight-faucet`
/// job claims ~950M NIGHT for it over the c2m bridge and registers it for DUST.
pub(crate) const DEV_WALLET_SEED: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";

/// Block until the dev wallet (seed `0x..01`) holds spendable NIGHT and has
/// generated DUST — i.e. it can fund and pay fees for a contract deploy/call.
///
/// In CI the `init-mnight-faucet` job (gated before the e2e suite) guarantees
/// this; on a warm local env it is already true. Tests attached to a
/// just-started env poll here (rather than flake) while the faucet job runs and
/// DUST accrues. Panics with a clear message if funding never arrives.
pub(crate) async fn ensure_dev_wallet_funded() {
    use midnight_node_ledger_helpers::WalletSeed;
    use midnight_node_toolkit::commands::show_wallet::{self, ShowWalletArgs, ShowWalletResult};

    let settings = Settings::default();
    let seed = WalletSeed::try_from_hex_str(DEV_WALLET_SEED).expect("valid dev wallet seed");
    let deadline = Instant::now() + Duration::from_secs(180);

    loop {
        let args = ShowWalletArgs {
            source: Source {
                src_url: Some(settings.node_client.base_url.clone()),
                fetch_concurrency: fetch_concurrency(),
                fetch_compute_concurrency: None,
                src_files: None,
                dust_warp: false,
                ignore_block_context: false,
                fetch_only_cached: false,
                fetch_cache: fetch_cache_config(),
                ledger_state_db: String::new(),
            },
            seed: Some(seed.clone()),
            address: None,
            debug: false,
            dry_run: false,
        };

        match show_wallet::execute(args).await {
            Ok(ShowWalletResult::Json(w)) => {
                let night: u128 = w.utxos.iter().map(|u| u.value).sum();
                if night > 0 && !w.dust_utxos.is_empty() {
                    tracing::info!(
                        "dev wallet {DEV_WALLET_SEED:.8}… funded: {night} NIGHT across {} utxo(s), {} dust utxo(s)",
                        w.utxos.len(),
                        w.dust_utxos.len(),
                    );
                    return;
                }
                tracing::info!(
                    "dev wallet not deploy-ready yet (night={night}, dust_utxos={}); waiting…",
                    w.dust_utxos.len()
                );
            }
            Ok(other) => tracing::warn!("unexpected show-wallet result: {other:?}"),
            Err(e) => tracing::warn!("show-wallet for dev wallet failed (retrying): {e}"),
        }

        assert!(
            Instant::now() < deadline,
            "dev wallet {DEV_WALLET_SEED} was not funded + DUST-registered within 180s — is the \
             init-mnight-faucet bring-up job healthy?"
        );
        sleep(Duration::from_secs(3)).await;
    }
}

/// A `Source` reading the live chain at `url`.
fn live_source(url: &str) -> Source {
    Source {
        src_url: Some(url.to_string()),
        fetch_concurrency: fetch_concurrency(),
        fetch_compute_concurrency: None,
        src_files: None,
        dust_warp: false,
        ignore_block_context: false,
        fetch_only_cached: false,
        fetch_cache: fetch_cache_config(),
        ledger_state_db: String::new(),
    }
}

fn to_file_dest(
    dest: &std::path::Path,
) -> midnight_node_toolkit::tx_generator::destination::Destination {
    midnight_node_toolkit::tx_generator::destination::Destination {
        dest_urls: vec![],
        rate: 1.0,
        dest_file: Some(dest.to_string_lossy().to_string()),
        no_watch_progress: true,
    }
}

/// Build a fresh contract-deploy transaction against the live chain, funded by
/// the dev wallet (`0x..01`), written to `dest` and returned as raw `.mn` bytes.
///
/// Generated dynamically (not from a static fixture) so its intent TTL is valid
/// against current chain time. The toolkit's in-process local prover handles ZK
/// proving (MIDNIGHT_LEDGER_TEST_STATIC_DIR, set in .envrc); no proof server.
/// `rng_seed` makes the deploy — and thus the contract address — deterministic.
pub(crate) async fn build_contract_deploy(
    url: &str,
    dest: &std::path::Path,
    rng_seed: Option<[u8; 32]>,
) -> Vec<u8> {
    use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs};
    use midnight_node_toolkit::tx_generator::builder::{Builder, ContractCall, ContractDeployArgs};

    let args = GenerateTxsArgs {
        builder: Builder::ContractSimple(ContractCall::Deploy(ContractDeployArgs {
            funding_seed: DEV_WALLET_SEED.to_string(),
            authority_seeds: vec![],
            authority_threshold: None,
            rng_seed,
        })),
        source: live_source(url),
        destination: to_file_dest(dest),
        proof_server: None,
        dry_run: false,
    };
    generate_txs::execute(args)
        .await
        .expect("generate-txs contract-simple deploy failed");
    std::fs::read(dest).expect("read generated deploy tx file")
}

/// True if `msg` names a transient shared-wallet ledger error that a rebuild
/// against fresh state typically clears: DUST double-spend (196), input not in
/// UTxOs (195), or invalid DUST spend proof (170). Everything built from the
/// single dev wallet `0x..01` can hit these when the state it was built against
/// has advanced (DUST regenerates every block) by submit/apply time — more
/// likely under CI's slower, `--test-threads`-concurrent proving.
pub(crate) fn is_transient_shared_wallet_error(msg: &str) -> bool {
    msg.contains("custom error: 196")
        || msg.contains("custom error: 195")
        || msg.contains("custom error: 170")
}

/// Build a fresh deploy funded by the dev wallet, submit it, wait for inclusion,
/// and return `(landed tx bytes, contract address)` — the bytes for callers that
/// replay them, the address for callers that query the deployed contract.
///
/// Retries by rebuilding on a transient DUST double-spend (ledger code 196) or
/// input-not-in-utxos (195): the deploy tests share dev wallet `0x..01`, so a
/// serialized prior deploy's DUST/UTxO spend may not yet be visible in the state
/// this build fetched. Rebuilding after a short settle picks up the fresh state.
/// Panics if it never lands.
pub(crate) async fn deploy_and_confirm(
    client: &midnight_node_e2e::api::midnight::MidnightClient,
    url: &str,
) -> (Vec<u8>, String) {
    use midnight_node_ledger_helpers::extract_tx_with_context;
    use midnight_node_toolkit::commands::contract_address::{self, ContractAddressArgs};

    let tempdir = tempfile::tempdir().expect("create tempdir");
    const ATTEMPTS: u8 = 4;
    for attempt in 1..=ATTEMPTS {
        let deploy_file = tempdir.path().join(format!("deploy_{attempt}.mn"));
        let deploy_bytes = build_contract_deploy(url, &deploy_file, None).await;
        let (deploy_tx, _ctx) = extract_tx_with_context(&deploy_bytes);
        match client.submit_expecting_success(deploy_tx.to_vec()).await {
            Ok(()) => {
                let addr = contract_address::execute(ContractAddressArgs {
                    src_file: deploy_file.to_string_lossy().to_string(),
                    tagged: false,
                    untagged: false,
                })
                .expect("derive contract address from deploy tx");
                return (deploy_tx.to_vec(), addr);
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    is_transient_shared_wallet_error(&msg) && attempt < ATTEMPTS,
                    "DEPLOY_TX failed to land (attempt {attempt}/{ATTEMPTS}): {msg}"
                );
                tracing::warn!(
                    "deploy attempt {attempt}/{ATTEMPTS} hit transient shared-wallet contention \
                     ({msg}); rebuilding against fresh state after settle"
                );
                sleep(Duration::from_secs(8)).await;
            }
        }
    }
    unreachable!("deploy_and_confirm loop returns or panics")
}

/// The dev wallet's own unshielded address on the `local` network (seed 0x..01).
/// Regenerate with:
///   midnight-node-toolkit show-address --network local --seed 0x..01 | jq -r .unshielded
pub(crate) const DEV_WALLET_UNSHIELDED_ADDR_LOCAL: &str =
    "mn_addr_local1h3ssm5ru2t6eqy4g3she78zlxn96e36ms6pq996aduvmateh9p9snnpjtr";

/// Build one tiny unshielded self-transfer from the dev wallet (0x..01) to `dest`.
///
/// A single tx on purpose: the dev wallet has one DUST UTxO, and the toolkit's
/// sender fires a batch's txs concurrently, so multiple dev-wallet txs sent
/// together collide on their DUST spend (InvalidDustSpendProof / DustDoubleSpend).
/// The multi-dest-url regression is about the *sender* managing N destination
/// clients, not N txs — so one valid tx across N URLs is the right, conflict-free
/// exercise.
pub(crate) async fn build_unshielded_self_transfer(url: &str, dest: &std::path::Path) {
    use midnight_node_toolkit::cli_parsers as cli;
    use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs};
    use midnight_node_toolkit::tx_generator::builder::{Builder, SingleTxArgs};

    let recipient = cli::wallet_address(DEV_WALLET_UNSHIELDED_ADDR_LOCAL)
        .expect("valid dev wallet unshielded address");
    let seed = WalletSeed::try_from_hex_str(DEV_WALLET_SEED).expect("valid dev wallet seed");

    let args = GenerateTxsArgs {
        builder: Builder::SingleTx(SingleTxArgs {
            outputs: vec![],
            shielded_amount: vec![],
            shielded_token_type: vec![],
            unshielded_amount: vec![100],
            unshielded_token_type: vec![],
            source_seed: seed,
            funding_seed: None,
            destination_address: vec![recipient],
            input_utxos: vec![],
            rng_seed: None,
            coin_selection: cli::coin_selection_strategy("largest-first").unwrap(),
        }),
        source: live_source(url),
        destination: to_file_dest(dest),
        proof_server: None,
        dry_run: false,
    };
    generate_txs::execute(args)
        .await
        .expect("generate-txs single-tx self-transfer failed");
}

// -------- GLOBAL ASYNC FAUCET MANAGER --------

static FAUCET_MANAGER: OnceCell<Arc<FaucetManager>> = OnceCell::const_new();

pub(crate) async fn global_faucet_manager() -> Arc<FaucetManager> {
    FAUCET_MANAGER
        .get_or_init(|| async {
            let settings = Settings::default();
            let faucet_wallet =
                CardanoClient::new_from_funded(settings.ogmios_client.clone(), settings.constants)
                    .await;

            Arc::new(FaucetManager::new(faucet_wallet).await)
        })
        .await
        .clone()
}

// -------- TOOLKIT FETCH CACHE --------

/// Cache backend for the toolkit's tx fetcher, selected by feature.
///
/// - local-env: `InMemory` — local chains are small and ephemeral, so
///   syncing into RAM per run costs nothing and adds no dependencies.
/// - qanet: `Postgres` when `TOOLKIT_CACHE_DB_URL` is set (CI wires
///   the shared cache via this secret — see PR #1578; developers can
///   set it locally to e.g. an SSH-tunneled RDS), otherwise
///   `InMemory` so local invocations without a tunnel still work.
pub(crate) fn fetch_cache_config() -> FetchCacheConfig {
    #[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]
    {
        FetchCacheConfig::InMemory
    }
    #[cfg(feature = "qanet")]
    {
        // CI sets `TOOLKIT_CACHE_DB_URL` to the shared toolkit-cache
        // RDS (see PR #1578); locally the SSH-tunneled URL is the
        // default so developers don't have to remember to set it.
        let url = std::env::var("TOOLKIT_CACHE_DB_URL").unwrap_or_else(|_| {
            "postgres://toolkit_cache_admin@127.0.0.1:10135/toolkit_cache_qanet".to_string()
        });
        FetchCacheConfig::Postgres { database_url: url }
    }
}

/// Per-env `DustBalanceArgs::source.fetch_concurrency`. Each cNIGHT
/// observation test opens this many websocket fetch workers against the
/// Midnight node during `dust_balance::execute`. With N tests running in
/// parallel, the node sees `N * fetch_concurrency` concurrent connections
/// — go too high on local-env and the node 429s mid-fetch (and
/// `MidnightClient::new()` calls from later test waves get rejected too).
///
/// - local-env: 4 — small chain, low total work; 4 workers × ~10 parallel
///   tests stays well under the node's connection cap.
/// - qanet: 20 — Cardano Preview's chain is large, fetch is the bottleneck,
///   the remote node has more headroom.
pub(crate) fn fetch_concurrency() -> usize {
    #[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]
    {
        4
    }
    #[cfg(feature = "qanet")]
    {
        20
    }
}

// -------- TEST MODULES --------
mod c2m_bridge;
mod cnight;
mod contract_state;
mod governance;
mod operational;

use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::faucet::FaucetManager;
use midnight_node_ledger_helpers::WalletSeed;
use midnight_node_toolkit::commands::dust_balance;
use midnight_node_toolkit::tx_generator::source::{FetchCacheConfig, Source};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard, OnceCell, Semaphore};
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
// subset runs (`cargo test ... contract_state::`, `... rpc_abuse::`) where
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
/// Set when the warmup snapshots its seed batch (quiescence reached). Seeds
/// registered after this point are NOT covered by the warmup — their
/// `dust_balance::execute` replays from genesis, which is multi-hour on
/// Preview and can starve the whole suite past the job timeout (#1644).
static WARMUP_FIRED: AtomicBool = AtomicBool::new(false);
/// Set once the warmup's `execute_many` finished (successfully or not).
/// Most tests reach their `dust_balance::execute` only after their
/// observation await — hours past warmup completion — but a test that
/// probes a balance *before* its await must gate on [`wait_for_warmup`]
/// or its execute races the warmup with a genesis replay of its own.
static WARMUP_DONE: AtomicBool = AtomicBool::new(false);

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
    if WARMUP_FIRED.load(Ordering::SeqCst) {
        tracing::warn!(
            "warmup: seed registered AFTER the warmup batch fired — its \
             dust_balance calls will replay from genesis (multi-hour on \
             Preview, and concurrent replays can starve the whole suite). \
             Create + register every seed a test needs at the top of the \
             test body, before any faucet/submission work (#1644)."
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
                // Contain panics from the warmup internals (e.g. the
                // toolkit's postgres backend panicking on PoolTimedOut, run
                // 28825722707): the warmup is best-effort — per-test
                // executes fall back to their own replay — but WARMUP_DONE
                // must be set regardless or `wait_for_warmup` callers idle
                // for their full cap.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rt.block_on(run_warmup());
                }));
                if result.is_err() {
                    tracing::warn!(
                        "warmup: thread panicked — wallet snapshots may be missing; \
                         per-test dust_balance calls will fall back to their own replay"
                    );
                }
                WARMUP_DONE.store(true, Ordering::SeqCst);
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
    // From here on, newly registered seeds miss the batch; make
    // `register_test_seed` warn loudly about it.
    WARMUP_FIRED.store(true, Ordering::SeqCst);
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
    WARMUP_DONE.store(true, Ordering::SeqCst);
}

/// Block until the background warmup finished writing wallet snapshots,
/// or `cap` elapsed (warmup wedged — e.g. node unreachable; the caller
/// then proceeds and its `execute` falls back to a genesis replay).
/// Cheap no-op once the warmup is done.
pub(crate) async fn wait_for_warmup(cap: Duration) {
    let started = Instant::now();
    while !WARMUP_DONE.load(Ordering::SeqCst) {
        if started.elapsed() > cap {
            tracing::warn!(
                "wait_for_warmup: warmup not done after {cap:?}; proceeding \
                 without the warm cache"
            );
            return;
        }
        sleep(WARMUP_POLL).await;
    }
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

// -------- DUST-BALANCE CONCURRENCY GATE --------

/// Gate for per-test `dust_balance::execute` calls.
///
/// All observation tests cross the stability barrier within seconds of each
/// other and immediately launch their dust-balance replays; each replay
/// builds a fresh node client and spawns `fetch_concurrency()` websocket
/// fetch workers, so N concurrent tests hit the node RPC with an N×-worker
/// connection storm. devnet's RPC times out under that load (run
/// 28702436341 failed tests with `CouldNotObtainRpcMethodList` /
/// `CannotFetchValue` request timeouts). With the warm wallet cache each
/// execute is seconds of work, so limiting concurrency costs little
/// wall-clock — and the first execute through the gate writes the
/// since-warmup delta blocks into the shared fetch cache, making the queued
/// ones cheaper still.
static DUST_BALANCE_GATE: Semaphore = Semaphore::const_new(2);

/// Failures of `dust_balance::execute` worth a fresh attempt: the toolkit's
/// node client has no mid-execute reconnect, so a websocket dropped by the
/// RPC's idle-killer or a restart surfaces as `RestartNeeded` /
/// `Connection(Closed)`; load-balanced replicas answering at slightly
/// different heights can return `null` for a block hash another replica
/// already announced ("invalid type: null" in run 28838014366); and the
/// shared Postgres fetch cache can transiently exhaust its pool. A retried
/// execute builds fresh clients, and the blocks the failed attempt fetched
/// are already in the fetch cache, so retries are cheap.
fn is_transient_dust_balance_error(e: &(dyn std::error::Error + Send + Sync)) -> bool {
    let s = format!("{e:?}");
    [
        "RestartNeeded",
        "Connection(Closed)",
        "RequestTimeout",
        "Request timeout",
        "invalid type: null",
        "PoolTimedOut",
    ]
    .iter()
    .any(|m| s.contains(m))
}

async fn dust_balance_execute_with_retry(
    args: dust_balance::DustBalanceArgs,
) -> Result<dust_balance::DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
    const ATTEMPTS: usize = 3;
    const RETRY_DELAY: Duration = Duration::from_secs(15);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let attempt_args = dust_balance::DustBalanceArgs {
            source: args.source.clone(),
            seed: args.seed.clone(),
            dry_run: args.dry_run,
        };
        match dust_balance::execute(attempt_args).await {
            Ok(r) => return Ok(r),
            Err(e) if attempt < ATTEMPTS && is_transient_dust_balance_error(e.as_ref()) => {
                tracing::warn!(
                    "dust_balance attempt {attempt}/{ATTEMPTS} failed transiently: {e:?}; \
                     retrying in {RETRY_DELAY:?}"
                );
                sleep(RETRY_DELAY).await;
            }
            Err(e) => return Err(e),
        }
    }
}

pub(crate) async fn gated_dust_balance(
    args: dust_balance::DustBalanceArgs,
) -> Result<dust_balance::DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
    let _permit = DUST_BALANCE_GATE
        .acquire()
        .await
        .expect("dust-balance gate semaphore closed");
    dust_balance_execute_with_retry(args).await
}

/// Ungated dust-balance for *window-sensitive* reads only: the spacing tests
/// (`spend_cnight_producing_dust`,
/// `stop_dust_producing_after_deregistration_and_rotation`) submit both tx
/// batches upfront and must read `balance_before` after the first batch's
/// watermark crossing but *before* the second's — a window of only the
/// enforced block spacing (~10-15 min at degraded Preview rates). Queueing
/// such a read behind [`DUST_BALANCE_GATE`] can eat the whole window: in run
/// 28825722707 stop_dust's read waited 12 min in the queue and completed the
/// second the dereg/rotate batch crossed, reading `balance_before = 0`. At
/// most two tests bypass concurrently, which the node RPC tolerates.
pub(crate) async fn window_dust_balance(
    args: dust_balance::DustBalanceArgs,
) -> Result<dust_balance::DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
    dust_balance_execute_with_retry(args).await
}

// -------- TOOLKIT FETCH CACHE --------

/// Cache backend for the toolkit's tx fetcher, selected by feature.
///
/// - local-env: `InMemory` — local chains are small and ephemeral, so
///   syncing into RAM per run costs nothing and adds no dependencies.
/// - qanet/devnet: `Postgres` when `TOOLKIT_CACHE_DB_URL` is set (CI
///   wires the shared cache via this secret — see PR #1578; developers
///   can set it locally to e.g. an SSH-tunneled RDS), otherwise
///   `InMemory` so local invocations without a tunnel still work. The
///   cache tables are keyed by chain id, so both networks share one
///   database without collisions.
pub(crate) fn fetch_cache_config() -> FetchCacheConfig {
    #[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]
    {
        FetchCacheConfig::InMemory
    }
    #[cfg(any(feature = "qanet", feature = "devnet"))]
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
/// - qanet/devnet: 20 — remote chains are larger, fetch is the
///   bottleneck, and the remote nodes have more headroom.
pub(crate) fn fetch_concurrency() -> usize {
    #[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]
    {
        4
    }
    #[cfg(any(feature = "qanet", feature = "devnet"))]
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
mod rpc_abuse;

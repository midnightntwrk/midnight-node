use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::faucet::FaucetManager;
use std::sync::atomic::{AtomicUsize, Ordering};
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

// -------- GLOBAL ASYNC FAUCET MANAGER --------

static FAUCET_MANAGER: OnceCell<Arc<FaucetManager>> = OnceCell::const_new();

pub(crate) async fn global_faucet_manager() -> Arc<FaucetManager> {
    FAUCET_MANAGER
        .get_or_init(|| async {
            let settings = Settings::default();
            let faucet_wallet =
                CardanoClient::new_from_funded(settings.ogmios_client.clone(), settings.constants)
                    .await;

            Arc::new(FaucetManager::new(settings.ogmios_client, faucet_wallet).await)
        })
        .await
        .clone()
}

// -------- TEST MODULES --------
mod cnight;
mod contract_state;
mod governance;
mod operational;
mod rpc_abuse;

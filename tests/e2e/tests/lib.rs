use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::faucet::FaucetManager;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, MutexGuard, OnceCell, Semaphore};

// Tests that must complete before any DEPLOY_TX submission.
// IMPORTANT: --test-threads must be >= NUM_PRE_DEPLOY_TESTS + NUM_DEPLOY_TESTS (currently 6),
// otherwise these tests cannot run concurrently and will deadlock.
const NUM_PRE_DEPLOY_TESTS: usize = 3;
const NUM_DEPLOY_TESTS: usize = 3;

static PRE_DEPLOY_COUNT: AtomicUsize = AtomicUsize::new(0);
static DEPLOY_GATE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(0));
// Deploy tests submit the same DEPLOY_TX, so concurrent submissions race in the
// txpool: one wins, the other gets "already imported", and pre_dispatch failures
// on the loser can ban the tx, leaving no live deployment. Serialize deploy tests
// behind this mutex so each runs to completion before the next starts.
static DEPLOY_SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn finished_pre_deploy_test() {
    let prev = PRE_DEPLOY_COUNT.fetch_add(1, Ordering::SeqCst);
    if prev == NUM_PRE_DEPLOY_TESTS - 1 {
        DEPLOY_GATE.add_permits(NUM_DEPLOY_TESTS);
    }
}

pub(crate) async fn wait_before_deploying() -> MutexGuard<'static, ()> {
    // Set E2E_SKIP_DEPLOY_GATE=1 to bypass the pre-deploy gate, e.g. when
    // running a single deploy test with `cargo test <name>`. Without this,
    // the gate would block forever waiting for pre-deploy tests that
    // aren't being run.
    if std::env::var_os("E2E_SKIP_DEPLOY_GATE").is_none() {
        let permit = DEPLOY_GATE.acquire().await.unwrap();
        permit.forget();
    }
    DEPLOY_SERIAL.lock().await
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

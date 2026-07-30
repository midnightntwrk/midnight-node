use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;
use tokio::time::{Duration, sleep, timeout};

use crate::{PreDeployGuard, wait_before_deploying};

// ============================================================================
// Audit Issue AD (#1166): Return ContractNotPresent Instead of Default State
//
// The RPC `midnight_contractState` must surface a `ContractNotPresent` error
// when queried for a contract that has never been deployed, so that callers
// can distinguish "deployed contract with empty state" from "no such contract".
// ============================================================================

fn assert_contract_not_present_error(err: &(dyn std::error::Error + 'static)) {
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("not present") || msg.contains("notpresent"),
        "expected ContractNotPresent error, got: {err}"
    );
}

/// #1166: a well-formed but undeployed contract address must return
/// ContractNotPresent — not an empty string and not a generic decode error.
/// Uses a well-formed but never-deployed address so we know the address itself
/// parses; the only reason for failure is "no contract here".
/// Pre-deploy gated so it runs before any deploy submission.
#[e2e_test]
async fn contract_state_for_undeployed_address_returns_not_present() {
    let _pre_deploy_guard = PreDeployGuard::new();

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // A well-formed 32-byte contract address that is never deployed. Contract
    // addresses are raw hex (network-independent), so no fixture is needed.
    let addr = "0000000000000000000000000000000000000000000000000000000000000001";

    let result = client.get_contract_state(addr).await;

    let err = result.expect_err("expected ContractNotPresent for undeployed contract, got Ok");
    assert_contract_not_present_error(err.as_ref());
}

/// #1166: an unparseable (non-hex) address must be rejected at the RPC layer
/// with BadContractAddress, distinct from ContractNotPresent. This protects
/// the new error variant from being conflated with input-validation failures.
#[e2e_test]
async fn contract_state_rejects_unparseable_address() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    let result = client.get_contract_state("zz_not_hex").await;

    let err = result.expect_err("expected BadContractAddress, got Ok");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("decode") && msg.contains("contract address"),
        "expected BadContractAddress (\"Unable to decode contract address\"), got: {err}"
    );
    assert!(
        !msg.contains("not present"),
        "BadContractAddress and ContractNotPresent must be distinct errors, got: {err}"
    );
}

/// #1166: the same address must return ContractNotPresent at a pre-deploy
/// block hash and the deployed state at a post-deploy block hash. This is
/// the strongest demonstration that the RPC now lets callers distinguish
/// "missing contract" from "contract with empty state".
///
/// Block 1 (the first block after genesis) is the pre-deploy reference —
/// no user transaction can have been included yet.
#[e2e_test]
async fn contract_state_distinguishes_historical_and_current_blocks() {
    // The deploy is funded by dev wallet 0x..01, which the local-env funds at runtime
    // over the cNIGHT bridge (init-mnight-faucet). Wait until it can fund + pay fees.
    crate::ensure_dev_wallet_funded().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client.clone()).await;

    // AURA produces block 1 ~6s after genesis. On a freshly-started CI runner
    // this test can race the first block, so poll briefly rather than failing
    // outright.
    let pre_deploy_hash = timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(hash) = client.get_block_hash_at_height(1).await {
                return hash;
            }
            sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .expect("block 1 not produced within 30s");

    let _deploy_guard = wait_before_deploying().await;

    // Generate a fresh DEPLOY_TX dynamically against the live chain. The
    // static fixture in res/test-contract has its intent_ttl baked in at
    // generation time and expires once chain time advances past it (~14 days
    // after fixture regeneration), so we can't rely on it for CI/live envs.
    // The toolkit's local prover (via MIDNIGHT_LEDGER_TEST_STATIC_DIR, set in
    // .envrc) handles ZK proof generation in-process; no external proof
    // server is required.
    let url = settings.node_client.base_url.clone();

    // Build + submit a fresh deploy (retrying past transient shared-wallet DUST
    // contention) and return its contract address. Dynamic generation keeps the
    // intent TTL valid against current chain time; the toolkit's in-process local
    // prover handles ZK proving (MIDNIGHT_LEDGER_TEST_STATIC_DIR, set in .envrc).
    let (_deploy_tx, addr) = crate::deploy_and_confirm(&client, &url).await;
    tracing::info!("Contract deployed at address: {addr}");

    // The deploy is now included; the contract must be present at the current head.
    let post_deploy_state = client
        .get_contract_state(&addr)
        .await
        .expect("expected deployed state at current head, got Err");
    assert!(
        !post_deploy_state.is_empty(),
        "deployed contract state should be non-empty ({} hex chars)",
        post_deploy_state.len()
    );

    // At block 1 the contract cannot exist — must error with ContractNotPresent.
    let pre_result = client
        .get_contract_state_at(&addr, Some(pre_deploy_hash))
        .await;
    let err = pre_result.expect_err("expected ContractNotPresent at pre-deploy block 1, got Ok");
    assert_contract_not_present_error(err.as_ref());

    tracing::info!("✓ block 1 → ContractNotPresent; current head → deployed state");
}

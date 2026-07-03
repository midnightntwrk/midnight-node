use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;
use midnight_node_ledger_helpers::extract_tx_with_context;

use crate::{
    PreDeployGuard, build_contract_deploy, build_contract_store, ensure_dev_wallet_funded,
};

// ============================================================================
// DDoS Mitigation E2E Tests (PR367)
// Tests for ADR-0003: Pre-Dispatch Validation of Guaranteed Transaction Part
//
// The attack shape is a contract `store` for a contract that was never
// deployed on-chain: it must be rejected at pre_dispatch (ContractNotPresent)
// so attackers can't fill blocks with failing, fee-less transactions.
//
// The store is built dynamically against the live chain with the deploy
// *overlaid* (so the toolkit's generator treats the contract as present) but
// the deploy is never submitted — see `build_contract_store`. Wallet 0x..01 is
// funded at runtime over the cNIGHT bridge (init-mnight-faucet).
// ============================================================================

/// Assert a rejection error looks like an InvalidTransaction from pre_dispatch.
fn assert_invalid_transaction(error_msg: &str) {
    let m = error_msg.to_lowercase();
    assert!(
        m.contains("invalid") || m.contains("transaction") || error_msg.contains("1010"),
        "expected an InvalidTransaction (pre_dispatch) error, got: {error_msg}"
    );
}

/// PR367-TC-0003-06: DDoS Attack Prevention - Single Transaction
///
/// A store for a non-deployed contract must be rejected at pre_dispatch
/// (ContractNotPresent), so attackers can't fill blocks with failing,
/// fee-less transactions.
#[e2e_test]
async fn ddos_attack_transaction_rejected_at_rpc() {
    // Pre-deploy-safe: the deploy is only overlaid for generation, never submitted,
    // so no contract lands on-chain.
    let _pre_deploy_guard = PreDeployGuard::new();
    ensure_dev_wallet_funded().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client.clone()).await;
    let url = settings.node_client.base_url.clone();

    let tempdir = tempfile::tempdir().expect("create tempdir");
    let deploy_file = tempdir.path().join("deploy.mn");
    let store_file = tempdir.path().join("store.mn");

    // Build (but never submit) a deploy, then a store against it via overlay.
    build_contract_deploy(&url, &deploy_file, None).await;
    let store_bytes = build_contract_store(&url, &deploy_file, &store_file, None).await;
    let (store_tx, _ctx) = extract_tx_with_context(&store_bytes);

    tracing::info!("Submitting STORE for a never-deployed contract (expect rejection)...");
    let result = client.submit_expecting_rejection(store_tx.to_vec()).await;
    assert!(
        result.is_ok(),
        "store-without-deploy should be rejected at pre_dispatch, but was accepted: {:?}",
        result.err()
    );
    let error_msg = result.unwrap();
    tracing::info!("✓ rejected with: {error_msg}");
    assert_invalid_transaction(&error_msg);
}

/// PR367-TC-0003-06: DDoS Attack Prevention - Batch Attack
///
/// Multiple distinct attack transactions must all be rejected. Each targets a
/// different never-deployed contract (distinct deploy rng_seed), so none can be
/// deduplicated by the pool — every one must be independently rejected.
#[e2e_test]
async fn ddos_batch_attack_all_rejected() {
    let _pre_deploy_guard = PreDeployGuard::new();
    ensure_dev_wallet_funded().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client.clone()).await;
    let url = settings.node_client.base_url.clone();
    let tempdir = tempfile::tempdir().expect("create tempdir");

    const TOTAL_ATTACKS: u8 = 3;
    for i in 1..=TOTAL_ATTACKS {
        let deploy_file = tempdir.path().join(format!("deploy_{i}.mn"));
        let store_file = tempdir.path().join(format!("store_{i}.mn"));
        // Distinct rng_seed → distinct contract address → distinct store tx.
        let mut rng_seed = [0u8; 32];
        rng_seed[31] = i;

        build_contract_deploy(&url, &deploy_file, Some(rng_seed)).await;
        let store_bytes =
            build_contract_store(&url, &deploy_file, &store_file, Some(rng_seed)).await;
        let (store_tx, _ctx) = extract_tx_with_context(&store_bytes);

        let result = client.submit_expecting_rejection(store_tx.to_vec()).await;
        assert!(
            result.is_ok(),
            "attack tx {i}/{TOTAL_ATTACKS} should be rejected, but was accepted: {:?}",
            result.err()
        );
        assert_invalid_transaction(&result.unwrap());
        tracing::info!("  ✓ attack tx {i}/{TOTAL_ATTACKS} rejected");
    }
    tracing::info!("✓ all {TOTAL_ATTACKS} attack transactions rejected");
}

/// PR367-TC-0003-02 E2E: Replay Attack Prevention
///
/// Submitting the same deploy transaction twice must reject the duplicate at
/// pre_dispatch (replay protection / ContractAlreadyDeployed) once the first is
/// on-chain.
#[e2e_test]
async fn replay_attack_rejected_via_rpc() {
    ensure_dev_wallet_funded().await;
    // Submits a real deploy — coordinate with the pre-deploy quiescence gate.
    let _deploy_guard = crate::wait_before_deploying().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client.clone()).await;
    let url = settings.node_client.base_url.clone();

    // First submission must land on-chain (retries past transient shared-wallet
    // DUST contention); returns the exact bytes that landed, to replay verbatim.
    tracing::info!("Submitting DEPLOY_TX (first attempt, expect success)...");
    let (deploy_tx, _addr) = crate::deploy_and_confirm(&client, &url).await;

    // Replaying the identical deploy must be rejected — the contract now exists.
    tracing::info!("Replaying the same DEPLOY_TX (expect rejection)...");
    let result = client.submit_expecting_rejection(deploy_tx.clone()).await;
    assert!(
        result.is_ok(),
        "replayed DEPLOY_TX should be rejected at pre_dispatch, but was accepted: {:?}",
        result.err()
    );
    let error_msg = result.unwrap();
    tracing::info!("✓ replay rejected with: {error_msg}");
    assert_invalid_transaction(&error_msg);
}

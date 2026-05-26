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

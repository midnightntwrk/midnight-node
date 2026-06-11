use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;

use crate::{PreDeployGuard, wait_before_deploying};

// ============================================================================
// DDoS Mitigation E2E Tests (PR367)
// Tests for ADR-0003: Pre-Dispatch Validation of Guaranteed Transaction Part
// ============================================================================

/// PR367-TC-0003-06: DDoS Attack Prevention - Single Transaction
///
/// Verifies that a transaction which would fail the guaranteed part
/// (due to ContractNotPresent) is rejected at the RPC level via pre_dispatch.
/// This prevents the DDoS attack vector where attackers fill blocks with
/// failing transactions that don't pay fees.
#[e2e_test]
async fn ddos_attack_transaction_rejected_at_rpc() {
    let _pre_deploy_guard = PreDeployGuard::new();
    use midnight_node_res::undeployed::transactions::STORE_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // STORE_TX requires the contract to be deployed first.
    // Without DEPLOY_TX, it will fail at pre_dispatch with ContractNotPresent.
    // This simulates an attacker trying to consume blockspace without paying fees.
    tracing::info!("=== PR367-TC-0003-06: DDoS Attack Prevention Test ===");
    tracing::info!("Submitting STORE_TX without prior DEPLOY_TX...");
    tracing::info!("Expected: Transaction rejected at pre_dispatch (ContractNotPresent)");

    let result = client.submit_expecting_rejection(STORE_TX.to_vec()).await;

    assert!(
        result.is_ok(),
        "Transaction should be rejected at pre_dispatch, but was accepted: {:?}",
        result.err()
    );

    let error_msg = result.unwrap();
    tracing::info!("✓ Transaction rejected with error: {}", error_msg);

    // The error should indicate an invalid transaction
    // (exact message depends on subxt error formatting)
    assert!(
        error_msg.to_lowercase().contains("invalid")
            || error_msg.to_lowercase().contains("transaction")
            || error_msg.contains("1010"), // Substrate InvalidTransaction code
        "Expected InvalidTransaction error, got: {}",
        error_msg
    );

    tracing::info!(
        "✓ PR367-TC-0003-06 PASSED: Attack transaction rejected, no blockspace consumed"
    );
}

/// PR367-TC-0003-06: DDoS Attack Prevention - Batch Attack
///
/// Verifies that multiple attack transactions are all rejected.
/// Simulates an attacker attempting to flood the network with failing transactions.
#[e2e_test]
async fn ddos_batch_attack_all_rejected() {
    let _pre_deploy_guard = PreDeployGuard::new();
    use midnight_node_res::undeployed::transactions::STORE_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    tracing::info!("=== PR367-TC-0003-06: Batch Attack Prevention Test ===");
    tracing::info!("Submitting 5 attack transactions (STORE_TX without DEPLOY_TX)...");

    let mut rejected_count = 0;
    let total_attacks = 5;

    for i in 0..total_attacks {
        let result = client.submit_expecting_rejection(STORE_TX.to_vec()).await;
        if result.is_ok() {
            rejected_count += 1;
            tracing::info!("  Attack tx {}/{} rejected ✓", i + 1, total_attacks);
        } else {
            tracing::info!(
                "  Attack tx {}/{} unexpectedly accepted! Error: {:?}",
                i + 1,
                total_attacks,
                result.err()
            );
        }
    }

    assert_eq!(
        rejected_count, total_attacks,
        "All {} attack transactions should be rejected, but only {} were",
        total_attacks, rejected_count
    );

    tracing::info!(
        "✓ PR367-TC-0003-06 PASSED: All {} attack transactions rejected",
        total_attacks
    );
}

/// PR367-TC-0003-02 E2E: Replay Attack Prevention
///
/// Verifies that submitting the same transaction twice results in rejection.
/// The replay protection mechanism should reject the duplicate transaction
/// at pre_dispatch, preventing replay attacks from consuming blockspace.
#[e2e_test]
async fn replay_attack_rejected_via_rpc() {
    use midnight_node_res::undeployed::transactions::DEPLOY_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    tracing::info!("=== PR367-TC-0003-02 E2E: Replay Attack Prevention Test ===");

    let _deploy_guard = wait_before_deploying().await;

    // First submission - may succeed or fail depending on node state
    // (contract may already be deployed from previous test runs)
    tracing::info!("Submitting DEPLOY_TX (first attempt)...");
    let first_result = client.submit_midnight_tx(DEPLOY_TX.to_vec()).await;

    match &first_result {
        Ok(_) => tracing::info!("  First submission accepted (contract not yet deployed)"),
        Err(e) => tracing::info!(
            "  First submission rejected (expected if contract exists): {}",
            e
        ),
    }

    // If first succeeded, wait for it to be processed before replay attempt
    if let Ok(mut progress) = first_result {
        tracing::info!("Waiting for first transaction to be included in block...");
        while let Some(status) = progress.next().await {
            match status {
                Ok(subxt::tx::TransactionStatus::InBestBlock(info)) => {
                    tracing::info!("  First transaction in best block: {:?}", info.block_hash());
                    break;
                }
                Ok(subxt::tx::TransactionStatus::InFinalizedBlock(info)) => {
                    tracing::info!("  First transaction finalized: {:?}", info.block_hash());
                    break;
                }
                Ok(subxt::tx::TransactionStatus::Error { message }) => {
                    tracing::info!("  First transaction error: {}", message);
                    break;
                }
                Ok(subxt::tx::TransactionStatus::Invalid { message }) => {
                    tracing::info!("  First transaction invalid: {}", message);
                    break;
                }
                Ok(subxt::tx::TransactionStatus::Dropped { message }) => {
                    tracing::info!("  First transaction dropped: {}", message);
                    break;
                }
                Ok(_) => continue,
                Err(e) => {
                    tracing::info!("  First transaction status error: {}", e);
                    break;
                }
            }
        }
    }

    // Second submission - MUST fail (either replay protection or ContractAlreadyDeployed)
    // Both are valid rejections that prevent the attack vector
    tracing::info!("Submitting DEPLOY_TX (second attempt - should be rejected)...");
    let second_result = client.submit_expecting_rejection(DEPLOY_TX.to_vec()).await;

    assert!(
        second_result.is_ok(),
        "Replay transaction should be rejected, but was accepted: {:?}",
        second_result.err()
    );

    let error_msg = second_result.unwrap();
    tracing::info!("✓ Replay transaction rejected with: {}", error_msg);

    // Verify the error indicates an invalid transaction
    // Accept various error types: replay protection, already deployed, or generic invalid.
    // "banned" covers the Substrate txpool ban: once the first submission is found
    // invalid its hash is temporarily banned, so the replay is rejected with
    // "Transaction is temporarily banned" — still a valid replay rejection.
    assert!(
        error_msg.to_lowercase().contains("invalid")
            || error_msg.to_lowercase().contains("replay")
            || error_msg.to_lowercase().contains("already")
            || error_msg.to_lowercase().contains("banned")
            || error_msg.contains("1010"), // Substrate InvalidTransaction code
        "Expected InvalidTransaction or replay-related error, got: {}",
        error_msg
    );

    tracing::info!("✓ PR367-TC-0003-02 E2E PASSED: Replay attack rejected, no blockspace consumed");
}

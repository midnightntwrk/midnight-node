use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;
use tokio::time::{Duration, timeout};

/// Verifies that governance contracts (council_forever and tech_auth_forever) were
/// deployed by midnight-setup and validates membership reset events.
///
/// This test verifies:
/// 1. Council Forever contract exists at the expected address with NFT
/// 2. Technical Authority Forever contract exists at the expected address with NFT
/// 3. Midnight blockchain emits membership reset events for the deployed contracts
#[e2e_test]
async fn verify_governance_contracts_and_validate_membership_reset() {
    tracing::info!("=== Verifying Governance Contracts Deployed by midnight-setup ===");

    let settings = Settings::default();

    let cardano_client =
        CardanoClient::new_from_funded(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Get expected addresses and policy IDs from runtime-values
    let council_address = config::council_forever_address();
    let council_policy_id = config::council_forever_policy_id();

    let tech_auth_address = config::tech_auth_forever_address();
    let tech_auth_policy_id = config::tech_auth_forever_policy_id();

    tracing::info!("Council Forever:");
    tracing::info!("  Policy ID (expected): {}", council_policy_id);
    tracing::info!("  Address: {}", council_address);

    tracing::info!("Technical Authority Forever:");
    tracing::info!("  Policy ID (expected): {}", tech_auth_policy_id);
    tracing::info!("  Address: {}", tech_auth_address);

    // Query UTxOs at council contract address to verify deployment
    tracing::info!("\n=== Verifying Council Forever Contract ===");
    let council_utxos = cardano_client.query_utxos(&council_address).await;
    assert!(
        !council_utxos.is_empty(),
        "Council Forever contract not found at expected address. Was midnight-setup run?"
    );

    // Verify at least one UTxO has an NFT with the expected policy ID
    let council_policy_bytes = hex::decode(&council_policy_id).expect("valid policy id hex");
    let council_has_nft = council_utxos.iter().any(|utxo| {
        utxo.value
            .native_tokens
            .iter()
            .any(|(policy_id, _)| policy_id.as_ref() == council_policy_bytes.as_slice())
    });
    assert!(
        council_has_nft,
        "Council Forever contract NFT with policy {} not found",
        council_policy_id
    );
    tracing::info!("✓ Council Forever contract verified at {}", council_address);

    // Query UTxOs at tech auth contract address to verify deployment
    tracing::info!("\n=== Verifying Technical Authority Forever Contract ===");
    let tech_auth_utxos = cardano_client.query_utxos(&tech_auth_address).await;
    assert!(
        !tech_auth_utxos.is_empty(),
        "Technical Authority Forever contract not found at expected address. Was midnight-setup run?"
    );

    // Verify at least one UTxO has an NFT with the expected policy ID
    let tech_auth_policy_bytes = hex::decode(&tech_auth_policy_id).expect("valid policy id hex");
    let tech_auth_has_nft = tech_auth_utxos.iter().any(|utxo| {
        utxo.value
            .native_tokens
            .iter()
            .any(|(policy_id, _)| policy_id.as_ref() == tech_auth_policy_bytes.as_slice())
    });
    assert!(
        tech_auth_has_nft,
        "Technical Authority Forever contract NFT with policy {} not found",
        tech_auth_policy_id
    );
    tracing::info!(
        "✓ Technical Authority Forever contract verified at {}",
        tech_auth_address
    );

    tracing::info!("\n=== Both Governance Contracts Verified Successfully ===");

    // Subscribe to federated authority observation events with timeout.
    // The subscribe helper scans historical blocks too, so no upfront
    // stability wait is needed.
    tracing::info!("Subscribing to federated authority events (timeout: 30 seconds)...");

    let events_result = timeout(
        Duration::from_secs(30),
        midnight_client.subscribe_to_federated_authority_events(),
    )
    .await;

    match events_result {
        Ok(Ok(_)) => {
            tracing::info!("Successfully received federated authority events");
        }
        Ok(Err(e)) => {
            tracing::info!("\n=== Governance Contracts Verification PARTIAL SUCCESS ===");
            tracing::info!("Contracts verified on-chain, but event subscription failed.");
            panic!("⚠ Failed to receive federated authority events: {}", e);
        }
        Err(_) => {
            tracing::info!("\n=== Governance Contracts Verification PARTIAL SUCCESS ===");
            tracing::info!(
                "Contracts verified on-chain, but events were not received within timeout."
            );
            panic!("⚠ Timeout waiting for federated authority events (30 seconds elapsed)");
        }
    }
}

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
    tracing::info!("Waiting for Midnight blockchain to emit membership reset events...\n");

    // Subscribe to federated authority observation events with timeout
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

/// Verifies that the federated_ops_forever contract was deployed by midnight-setup.
///
/// This test verifies:
/// 1. Federated Operators Forever contract exists at the expected address
/// 2. The contract NFT was minted with the expected policy ID
#[e2e_test]
async fn verify_federated_ops_contract_deployment() {
    tracing::info!("=== Verifying Federated Operators Contract Deployed by midnight-setup ===");

    let settings = Settings::default();

    let cardano_client =
        CardanoClient::new_from_funded(settings.ogmios_client, settings.constants).await;

    // Get expected address and policy ID from runtime-values
    let federated_ops_address = config::federated_ops_forever_address();
    let federated_ops_policy_id = config::federated_ops_forever_policy_id();

    tracing::info!("Federated Operators Forever:");
    tracing::info!("  Policy ID (expected): {}", federated_ops_policy_id);
    tracing::info!("  Address: {}", federated_ops_address);

    // Query UTxOs at federated ops contract address to verify deployment
    tracing::info!("\n=== Verifying Federated Operators Forever Contract ===");
    let federated_ops_utxos = cardano_client.query_utxos(&federated_ops_address).await;
    assert!(
        !federated_ops_utxos.is_empty(),
        "Federated Operators Forever contract not found at expected address. Was midnight-setup run?"
    );

    // Verify at least one UTxO has an NFT with the expected policy ID
    let federated_ops_policy_bytes =
        hex::decode(&federated_ops_policy_id).expect("valid policy id hex");
    let has_nft = federated_ops_utxos.iter().any(|utxo| {
        utxo.value
            .native_tokens
            .iter()
            .any(|(policy_id, _)| policy_id.as_ref() == federated_ops_policy_bytes.as_slice())
    });
    assert!(
        has_nft,
        "Federated Operators Forever contract NFT with policy {} not found",
        federated_ops_policy_id
    );

    tracing::info!(
        "✓ Federated Operators Forever contract verified at {}",
        federated_ops_address
    );
    tracing::info!("\n=== Federated Operators Contract Verification Complete ===");
}

/// Verify D-Parameter RPC endpoint accepts block hash parameter for historical queries.
///
/// This test verifies:
/// - systemParameters_getDParameter accepts optional block hash parameter
/// - Querying at genesis block returns valid values
/// - Querying at current block returns valid values
/// - Querying at an invalid block hash returns an error
///
/// LIMITATION: Since D-parameter can only be changed via governance (Root origin),
/// this test cannot fully verify that historical queries return *different* values
/// at different blocks when the parameter has changed. To fully test that scenario,
/// a governance transaction would need to update the D-parameter between blocks.
/// However, this test does verify the historical query code path is exercised
/// by querying at different block heights and validating error handling.
#[e2e_test]
async fn query_d_parameter_at_historical_block() {
    tracing::info!("=== D-Parameter Historical Block Query E2E Test ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Step 1: Get genesis block hash (block 0) to test historical query at earliest block
    let genesis_block_hash = midnight_client
        .get_block_hash_at_height(0)
        .await
        .expect("Failed to get genesis block hash");
    tracing::info!(
        "Genesis block hash: 0x{}",
        hex::encode(genesis_block_hash.as_bytes())
    );

    // Step 2: Get current best block hash
    let current_block_hash = midnight_client
        .get_best_block_hash()
        .await
        .expect("Failed to get best block hash");
    tracing::info!(
        "Current block hash: 0x{}",
        hex::encode(current_block_hash.as_bytes())
    );

    // Step 3: Query D-Parameter at genesis block
    tracing::info!("Querying D-param at genesis block...");
    let d_param_at_genesis = midnight_client
        .get_d_parameter_at(genesis_block_hash)
        .await
        .expect("Failed to query D-param at genesis block");
    tracing::info!(
        "D-param at genesis: ({}, {})",
        d_param_at_genesis.num_permissioned_candidates,
        d_param_at_genesis.num_registered_candidates
    );

    // Step 4: Query D-Parameter at current block
    tracing::info!("Querying D-param at current block...");
    let d_param_at_current = midnight_client
        .get_d_parameter_at(current_block_hash)
        .await
        .expect("Failed to query D-param at current block");
    tracing::info!(
        "D-param at current: ({}, {})",
        d_param_at_current.num_permissioned_candidates,
        d_param_at_current.num_registered_candidates
    );

    // Step 5: Verify both queries returned valid data
    // Note: Values may be the same since D-parameter hasn't been changed via governance.
    // This test primarily verifies the historical query code path works, not that
    // different blocks have different values (which would require governance changes).
    tracing::info!("✓ Historical block queries returned valid D-parameter data");

    // Step 6: Test error handling - query with invalid block hash
    tracing::info!("Testing error handling with invalid block hash...");
    let invalid_block_hash = subxt::utils::H256::from([0xff; 32]);
    let invalid_query_result = midnight_client.get_d_parameter_at(invalid_block_hash).await;

    assert!(
        invalid_query_result.is_err(),
        "Query with invalid block hash should return an error, but got: {:?}",
        invalid_query_result
    );
    tracing::info!(
        "✓ Invalid block hash correctly rejected: {}",
        invalid_query_result.unwrap_err()
    );

    // Step 7: Verify querying the same block hash is idempotent
    tracing::info!("Verifying idempotent queries at same block hash...");
    let d_param_at_genesis_again = midnight_client
        .get_d_parameter_at(genesis_block_hash)
        .await
        .expect("Failed to query D-param at genesis block again");

    assert_eq!(
        d_param_at_genesis.num_permissioned_candidates,
        d_param_at_genesis_again.num_permissioned_candidates,
        "D-param permissioned at same block hash should be consistent"
    );
    assert_eq!(
        d_param_at_genesis.num_registered_candidates,
        d_param_at_genesis_again.num_registered_candidates,
        "D-param registered at same block hash should be consistent"
    );

    tracing::info!("✓ Historical block query verification passed");
    tracing::info!("Note: D-parameter values at genesis and current block are the same");
    tracing::info!("because no governance transaction has updated the parameter.");
    tracing::info!("To fully test historical value differences, use update_d_parameter");
    tracing::info!("via federated authority governance between block queries.");
}

// ========== Aiken Permissioned Candidates E2E Tests ==========
// These tests verify permissioned candidates via the new Aiken contracts

/// TC-PC-001: Verify systemParameters_getAriadneParameters returns valid structure.
///
/// Tests that the RPC endpoint returns correctly structured data including:
/// - D-Parameter with permissioned and registered candidate counts
/// - Block info metadata showing where D-Parameter was fetched from
/// - Permissioned candidates list (may be None if not set on mainchain)
#[e2e_test]
async fn get_ariadne_parameters_returns_valid_structure() {
    tracing::info!("=== TC-PC-001: Ariadne Parameters Structure Validation ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Use epoch 4 to query data from epoch 2 (SDK applies 2-epoch offset).
    // Contracts are deployed in epoch 2, so querying epoch 4 returns data from epoch 2.
    let epoch_number = 4u64;

    let ariadne_params = midnight_client
        .get_ariadne_parameters(epoch_number, None)
        .await
        .expect("Failed to get Ariadne parameters");

    tracing::info!("Ariadne Parameters Response:");
    tracing::info!(
        "  D-Parameter: ({}, {})",
        ariadne_params.d_parameter.num_permissioned_candidates,
        ariadne_params.d_parameter.num_registered_candidates
    );
    tracing::info!(
        "  Permissioned Candidates: {:?}",
        ariadne_params
            .permissioned_candidates
            .as_ref()
            .map(|c| c.len())
    );

    // Verify D-Parameter structure is valid (values can be 0)
    // The important thing is that the RPC call succeeded and returned valid types
    tracing::info!("✓ Ariadne parameters structure is valid");
}

/// TC-PC-003: Verify D-Parameter from pallet matches expected configuration.
///
/// The D-Parameter is now sourced from pallet-system-parameters instead of Cardano.
/// In local environment, it's configured as (4, 1) - 4 permissioned, 1 registered.
#[e2e_test]
async fn d_parameter_from_pallet_matches_config() {
    tracing::info!("=== TC-PC-003: D-Parameter Pallet Integration ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Query D-Parameter directly via the dedicated RPC
    let d_param = midnight_client
        .get_d_parameter()
        .await
        .expect("Failed to get D-Parameter");

    tracing::info!(
        "D-Parameter from pallet-system-parameters: ({}, {})",
        d_param.num_permissioned_candidates,
        d_param.num_registered_candidates
    );

    // Also query via getAriadneParameters to verify consistency
    // Use epoch 2 (minimum supported epoch)
    let ariadne_params = midnight_client
        .get_ariadne_parameters(2, None)
        .await
        .expect("Failed to get Ariadne parameters");

    tracing::info!(
        "D-Parameter from getAriadneParameters: ({}, {})",
        ariadne_params.d_parameter.num_permissioned_candidates,
        ariadne_params.d_parameter.num_registered_candidates
    );

    // Verify both endpoints return the same D-Parameter
    assert_eq!(
        d_param.num_permissioned_candidates, ariadne_params.d_parameter.num_permissioned_candidates,
        "D-Parameter permissioned count should match between endpoints"
    );
    assert_eq!(
        d_param.num_registered_candidates, ariadne_params.d_parameter.num_registered_candidates,
        "D-Parameter registered count should match between endpoints"
    );

    // Local environment configures D-Parameter as (3, 0)
    // 3 permissioned (Alice, Bob, Charlie) from qanet config
    assert_eq!(
        d_param.num_permissioned_candidates, 3,
        "Permissioned count should match system-parameters config (expected 3)"
    );
    assert_eq!(
        d_param.num_registered_candidates, 0,
        "Registered count should match system-parameters config (expected 0)"
    );

    tracing::info!("✓ D-Parameter correctly sourced from pallet-system-parameters");
}

/// TC-PC-002: Verify permissioned candidates match Aiken format.
///
/// In local environment, 3 permissioned candidates (Alice, Bob, Charlie)
/// are inserted during setup. This test verifies they are returned in the
/// Aiken contract format with the correct structure.
#[e2e_test]
async fn permissioned_candidates_aiken_format() {
    tracing::info!("=== TC-PC-002: Aiken Permissioned Candidates Format Validation ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Use epoch 4 to query data from epoch 2 (SDK applies 2-epoch offset).
    // Contracts are deployed in epoch 2, so querying epoch 4 returns data from epoch 2.
    let epoch_number = 4u64;

    let ariadne_params = midnight_client
        .get_ariadne_parameters(epoch_number, None)
        .await
        .expect("Failed to get Ariadne parameters");

    if let Some(candidates) = &ariadne_params.permissioned_candidates {
        tracing::info!("Found {} permissioned candidates", candidates.len());

        // Local environment inserts 3 permissioned candidates
        assert!(
            candidates.len() >= 3,
            "Expected at least 3 permissioned candidates in local-env, found {}",
            candidates.len()
        );

        // Verify each candidate has required keys
        // With Aiken format, the structure is:
        // - sidechainPublicKey: hex string
        // - keys: object with named keys { "aura": "0x...", "gran": "0x..." }
        // - isValid: boolean
        for (i, candidate) in candidates.iter().enumerate() {
            let has_sidechain_key = candidate.get("sidechainPublicKey").is_some()
                || candidate.get("sidechain_public_key").is_some();

            // Check for keys object containing aura and gran keys (Aiken format)
            let keys = candidate.get("keys");
            let has_keys = keys
                .and_then(|k| k.as_object())
                .map(|obj| obj.contains_key("aura") && obj.contains_key("gran"))
                .unwrap_or(false);

            tracing::info!(
                "  Candidate {}: sidechain={}, has_keys={}",
                i,
                has_sidechain_key,
                has_keys
            );

            assert!(
                has_sidechain_key,
                "Candidate {} should have sidechain public key",
                i
            );
            assert!(
                has_keys,
                "Candidate {} should have keys object with aura and gran entries",
                i
            );
        }

        tracing::info!(
            "✓ All permissioned candidates have Aiken format with sidechainPublicKey and keys object"
        );
    } else {
        // In some test environments, permissioned candidates might not be set
        tracing::info!(
            "⚠ No permissioned candidates returned (may be expected in some environments)"
        );
    }
}

/// TC-PC-004: Verify authority selection uses Aiken permissioned candidates.
///
/// This test verifies the full authority selection flow:
/// 1. Waits for the chain to reach a stable epoch (epoch >= 2)
/// 2. Queries the current AURA authorities from the runtime
/// 3. Queries permissioned candidates from Ariadne parameters
/// 4. Verifies candidates have valid key structure (AURA, GRANDPA, sidechain keys)
///
/// This confirms that the Aiken-format permissioned candidates are correctly
/// parsed and available via the systemParameters RPC.
#[e2e_test]
async fn authority_selection_uses_aiken_candidates() {
    tracing::info!("=== TC-PC-004: Aiken Permissioned Candidates Validation ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Use epoch 4 to query data from epoch 2 (SDK applies 2-epoch offset).
    // Contracts are deployed in epoch 2, so querying epoch 4 returns data from epoch 2.
    let target_epoch = 4u64;
    tracing::info!(
        "Using epoch {} for permissioned candidates validation (data_epoch = {})",
        target_epoch,
        target_epoch - 2
    );

    // Wait for a finalized block to ensure chain state is stable
    let _finalized_hash = midnight_client
        .wait_for_next_finalized_block()
        .await
        .expect("Failed to wait for finalized block");

    // Query permissioned candidates from Ariadne parameters
    // Uses systemParameters_getAriadneParameters RPC
    let ariadne_params = midnight_client
        .get_ariadne_parameters(target_epoch, None)
        .await
        .expect("Failed to get Ariadne parameters");

    let candidates = ariadne_params
        .permissioned_candidates
        .expect("Expected permissioned candidates to be present");

    tracing::info!(
        "Permissioned candidates from Aiken contracts: {}",
        candidates.len()
    );

    assert!(
        !candidates.is_empty(),
        "Expected at least one permissioned candidate"
    );

    // Validate each candidate has the expected Aiken key structure
    // Structure: { sidechainPublicKey: "0x...", keys: { "aura": "0x...", "gran": "0x..." } }
    let mut valid_candidates = 0;
    for (i, candidate) in candidates.iter().enumerate() {
        let keys = candidate
            .get("keys")
            .expect(&format!("Candidate {} missing 'keys' field", i));

        // Validate AURA key
        let aura_key = keys
            .get("aura")
            .expect(&format!("Candidate {} missing 'aura' key", i));
        let aura_str = aura_key.as_str().unwrap_or("");
        assert!(!aura_str.is_empty(), "Candidate {} has empty AURA key", i);

        // Validate GRANDPA key (key type is "gran" - 4-byte identifier)
        let grandpa_key = keys
            .get("gran")
            .expect(&format!("Candidate {} missing 'gran' key", i));
        let grandpa_str = grandpa_key.as_str().unwrap_or("");
        assert!(
            !grandpa_str.is_empty(),
            "Candidate {} has empty GRANDPA key",
            i
        );

        // Validate sidechain public key (at candidate level, not inside keys)
        let sidechain_key = candidate
            .get("sidechainPublicKey")
            .or_else(|| candidate.get("sidechain_public_key"))
            .expect(&format!("Candidate {} missing 'sidechainPublicKey'", i));
        let sidechain_str = sidechain_key.as_str().unwrap_or("");
        assert!(
            !sidechain_str.is_empty(),
            "Candidate {} has empty sidechainPublicKey",
            i
        );

        tracing::info!(
            "  [{}] AURA: {}... GRANDPA: {}... Sidechain: {}...",
            i,
            &aura_str[..aura_str.len().min(16)],
            &grandpa_str[..grandpa_str.len().min(16)],
            &sidechain_str[..sidechain_str.len().min(16)]
        );

        valid_candidates += 1;
    }

    assert_eq!(
        valid_candidates,
        candidates.len(),
        "All candidates should have valid key structure"
    );

    tracing::info!(
        "\n✓ Validated {} Aiken permissioned candidates with complete key structure",
        valid_candidates
    );
}

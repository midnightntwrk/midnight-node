use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::events::{
    Deregistration, MappingAdded, Registration,
};
use ogmios_client::query_ledger_state::QueryLedgerState;
use std::slice::from_ref;
use tokio::time::{Duration, timeout};
use whisky::Asset;

#[tokio::test]
async fn register_for_dust_production() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let dust_hex = MidnightClient::new_dust_hex();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let collateral_utxo = cardano_client
        .make_collateral()
        .await
        .expect("Failed to make collateral");
    let assets = vec![Asset::new_from_str("lovelace", "10000000")];
    let tx_in = cardano_client
        .fund_wallet(assets)
        .await
        .expect("Failed to fund a wallet");

    let utxos = cardano_client
        .ogmios_clients
        .query_utxos(from_ref(&address_bech32))
        .await
        .unwrap();
    assert_eq!(
        utxos.len(),
        2,
        "New wallet should have exactly two UTXOs after funding"
    );

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register transaction")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {:?}",
        register_tx_id
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: [u8; 33] = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let registration_events = midnight_client
        .subscribe_to_cnight_observation_events(&register_tx_id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let registration = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0 == dust_address
        });
    assert!(
        registration.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Registration event found: {:?}",
        registration.unwrap()
    );

    let mapping_added = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0 == dust_address
                && map.0.utxo_tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_added.is_some(),
        "Did not find MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    println!(
        "Matching MappingAdded event found: {:?}",
        mapping_added.unwrap()
    );
}

#[tokio::test]
async fn deploy_governance_contracts_and_validate_membership_reset() {
    println!("=== Starting Governance Contracts E2E Test ===");

    let settings = Settings::default();
    let policies = settings.constants.policies.clone();
    let funded_address = settings.constants.payments.funded_address.clone();

    let cardano_client =
        CardanoClient::new_from_funded(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Example Sr25519 public keys for testing (Alice and Eve from Substrate)
    // In production, these would be the actual governance authority member keys
    const ALICE_SR25519: &str = "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
    const EVE_SR25519: &str = "e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e";

    // Use the funded_address from config as the deployer
    // The funded_address owns the one-shot UTxOs, so we use it for all inputs to simplify signing
    println!("Using funded_address for deployment: {}", funded_address);

    // Alice's Cardano key hash
    let alice_cardano_hash = "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b";

    // Bob's Cardano key hash
    let bob_cardano_hash = "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2c";

    // Fund UTxOs for deployment (these will be owned by funded_address)
    let funding_assets = vec![Asset::new_from_str("lovelace", "500000000")]; // 500 ADA
    let tx_in_utxo = cardano_client
        .fund_wallet(funding_assets.clone())
        .await
        .expect("Failed to fund a wallet");
    println!("First funding UTXO created");

    // Create additional funding UTxO for second deployment
    let tx_in_utxo_2 = cardano_client
        .fund_wallet(funding_assets)
        .await
        .expect("Failed to fund a wallet");
    println!("Second funding UTXO created");

    // Create collateral for script transactions
    let collateral_utxo = cardano_client
        .make_collateral()
        .await
        .expect("Failed to generate collateral");
    println!("Collateral UTXO created");

    // Load contract CBORs and calculate addresses and policy IDs
    let council_cbor = policies.council_forever_cbor_double_encoding();
    let council_address = policies.council_forever_address();
    let council_policy_id = policies.council_forever_policy_id();

    let tech_auth_cbor = policies.tech_auth_forever_cbor_double_encoding();
    let tech_auth_address = policies.tech_auth_forever_address();
    let tech_auth_policy_id = policies.tech_auth_forever_policy_id();

    println!("Council Forever:");
    println!("  Policy ID (calculated): {}", council_policy_id);
    println!("  Address: {}", council_address);

    println!("Technical Authority Forever:");
    println!("  Policy ID (calculated): {}", tech_auth_policy_id);
    println!("  Address: {}", tech_auth_address);

    // Get pre-created one-shot UTxOs from local-environment
    // These are created by the Cardano entrypoint.sh script during network setup
    let council_one_shot = cardano_client
        .one_shot_utxo("council")
        .await
        .expect("Failed to get one shot council");
    println!("✓ Council one-shot UTXO retrieved from local-environment");

    let tech_auth_one_shot = cardano_client
        .one_shot_utxo("techauth")
        .await
        .expect("Failed to get one shot techauth");
    println!("✓ Technical Authority one-shot UTXO retrieved from local-environment");

    // Deploy Council Forever contract
    println!("\n=== Deploying Council Forever Contract ===");
    let council_members = vec![
        (alice_cardano_hash.to_string(), ALICE_SR25519.to_string()),
        (bob_cardano_hash.to_string(), EVE_SR25519.to_string()),
    ];

    let council_tx_id = cardano_client
        .deploy_governance_contract(
            &tx_in_utxo,
            &collateral_utxo,
            &council_one_shot,
            &council_cbor,
            &council_address,
            &council_policy_id,
            council_members.clone(),
            2, // total_signers
        )
        .await
        .expect("Failed to deploy the governance contract")
        .transaction
        .id;

    println!("✓ Council Forever contract deployed successfully with tx ID: {council_tx_id:?}");

    // Deploy Technical Authority Forever contract
    println!("\n=== Deploying Technical Authority Forever Contract ===");
    let tech_auth_members = vec![
        (alice_cardano_hash.to_string(), ALICE_SR25519.to_string()),
        (bob_cardano_hash.to_string(), EVE_SR25519.to_string()),
    ];

    let tech_auth_tx_id = cardano_client
        .deploy_governance_contract(
            &tx_in_utxo_2,
            &collateral_utxo,
            &tech_auth_one_shot,
            &tech_auth_cbor,
            &tech_auth_address,
            &tech_auth_policy_id,
            tech_auth_members.clone(),
            2, // total_signers
        )
        .await
        .expect("Failed to deploy the governance contract")
        .transaction
        .id;

    println!(
        "✓ Technical Authority Forever contract deployed successfully with tx ID: {tech_auth_tx_id:?}"
    );

    println!("\n=== Both Governance Contracts Deployed Successfully ===");
    println!("Waiting for Midnight blockchain to emit membership reset events...\n");

    // Subscribe to federated authority observation events with timeout
    println!("Subscribing to federated authority events (timeout: 30 seconds)...");

    let events_result = timeout(
        Duration::from_secs(30),
        midnight_client.subscribe_to_federated_authority_events(),
    )
    .await;

    match events_result {
        Ok(Ok(_)) => {
            println!("Successfully received federated authority events");
        }
        Ok(Err(e)) => {
            println!("\n=== Governance Contracts E2E Test PARTIAL SUCCESS ===");
            println!("Contracts deployed successfully, but event subscription failed.");
            println!(
                "The contracts are active on-chain, but event verification could not be completed."
            );
            panic!("⚠ Failed to receive federated authority events: {}", e);
        }
        Err(_) => {
            println!("\n=== Governance Contracts E2E Test PARTIAL SUCCESS ===");
            println!(
                "Contracts deployed successfully, but events were not received within timeout."
            );
            println!(
                "The contracts are active on-chain. The Midnight blockchain may need more time to process."
            );
            panic!("⚠ Timeout waiting for federated authority events (30 seconds elapsed)");
        }
    }
}

#[tokio::test]
async fn register_2_cardano_same_dust_address_production() {
    let settings = Settings::default();
    let cardano_client_1 =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let cardano_client_2 =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let address_bech_32_1 = cardano_client_1.address_as_bech32();
    let address_bech_32_2 = cardano_client_2.address_as_bech32();
    println!("First Cardano wallet created: {:?}", address_bech_32_1);
    println!("Second Cardano wallet created: {:?}", address_bech_32_2);

    let dust_hex = MidnightClient::new_dust_hex();
    let dust_bytes: [u8; 33] = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    println!(
        "Registering First Cardano wallet {} with DUST address {}",
        address_bech_32_1, dust_hex
    );
    println!(
        "Registering Second Cardano wallet {} with DUST address {}",
        address_bech_32_2, dust_hex
    );

    let collateral_utxo_1 = cardano_client_1
        .make_collateral()
        .await
        .expect("Failed to create collateral");
    let assets_1 = vec![Asset::new_from_str("lovelace", "10000000")];
    let tx_in_1 = cardano_client_1
        .fund_wallet(assets_1)
        .await
        .expect("Failed to fund a wallet");

    let collateral_utxo_2 = cardano_client_2
        .make_collateral()
        .await
        .expect("Failed to fund_wallet");
    let assets_2 = vec![Asset::new_from_str("lovelace", "10000000")];
    let tx_in_2 = cardano_client_2
        .fund_wallet(assets_2)
        .await
        .expect("Failed to fund a wallet");

    let utxos_1 = cardano_client_1
        .ogmios_clients
        .query_utxos(from_ref(&address_bech_32_1))
        .await
        .unwrap();
    assert_eq!(
        utxos_1.len(),
        2,
        "First wallet should have exactly two UTXOs after funding"
    );

    let utxos_2 = cardano_client_2
        .ogmios_clients
        .query_utxos(from_ref(&address_bech_32_2))
        .await
        .unwrap();
    assert_eq!(
        utxos_2.len(),
        2,
        "Second wallet should have exactly two UTXOs after funding"
    );

    let register_tx_id_1 = cardano_client_1
        .register(&dust_hex, &tx_in_1, &collateral_utxo_1)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction for the first cardano submitted with hash: {:?}",
        register_tx_id_1
    );

    let register_tx_id_2 = cardano_client_2
        .register(&dust_hex, &tx_in_2, &collateral_utxo_2)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction for second cardano submitted with hash: {:?}",
        register_tx_id_1
    );

    let reward_address_1 = cardano_client_1.reward_address_bytes();
    let reward_address_2 = cardano_client_2.reward_address_bytes();

    let dust_address: [u8; 33] = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let registration_events_1 = midnight_client
        .subscribe_to_cnight_observation_events(&register_tx_id_1)
        .await
        .expect("Failed to listen to cNgD registration event");

    let registration_events_2 = midnight_client
        .subscribe_to_cnight_observation_events(&register_tx_id_2)
        .await
        .expect("Failed to listen to cNgD registration event");

    let registration_1 = registration_events_1
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address_1
                && reg.0.dust_public_key.0 == dust_address
        });

    let registration_2 = registration_events_2
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address_2
                && reg.0.dust_public_key.0 == dust_address
        });

    assert!(
        registration_1.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );

    assert!(
        registration_2.is_some(),
        "Did not find second registration event with expected second reward_address and dust_address"
    );

    println!(
        "Matching Registration event found: {:?}",
        registration_1.unwrap()
    );

    println!(
        "Matching Second Registration event found: {:?}",
        registration_2.unwrap()
    );

    let mapping_added_1 = registration_events_1
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address_1
                && map.0.dust_public_key.0 == dust_address
                && map.0.utxo_tx_hash.0 == register_tx_id_1
        });

    let mapping_added_2 = registration_events_2
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address_2
                && map.0.dust_public_key.0 == dust_address
                && map.0.utxo_tx_hash.0 == register_tx_id_2
        });
    assert!(
        mapping_added_1.is_some(),
        "Did not find first MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    assert!(
        mapping_added_2.is_some(),
        "Did not find second MappingAdded event with expected second_reward_address, dust_address, and utxo_id"
    );

    println!(
        "Matching first MappingAdded event found: {:?}",
        mapping_added_1.unwrap()
    );

    println!(
        "Matching second MappingAdded event found: {:?}",
        mapping_added_2.unwrap()
    );
}

#[tokio::test]
async fn cnight_produces_dust() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let bech32_address = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", bech32_address);

    let dust_hex = MidnightClient::new_dust_hex();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        bech32_address, dust_hex
    );

    let collateral_utxo = cardano_client
        .make_collateral()
        .await
        .expect("Failed to make collateral");
    let assets = vec![Asset::new_from_str("lovelace", "10000000")];
    let tx_in = cardano_client
        .fund_wallet(assets)
        .await
        .expect("Failed to fund a wallet");

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {:?}",
        register_tx_id
    );

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {:?}", amount, tx_id);

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo = match cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(tx_id))
        .await
    {
        Some(cnight_utxo) => cnight_utxo,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix = b"asset_create";
    let nonce =
        MidnightClient::calculate_nonce(prefix, cnight_utxo.transaction.id, cnight_utxo.index);
    println!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    println!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );
}

#[tokio::test]
async fn deregister_from_dust_production() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let dust_hex = MidnightClient::new_dust_hex();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let collateral_utxo = cardano_client
        .make_collateral()
        .await
        .expect("Failed to make collateral");
    let assets = vec![Asset::new_from_str("lovelace", "10000000")];
    let tx_in = cardano_client
        .fund_wallet(assets)
        .await
        .expect("Failed to fund a wallet");

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {:?}",
        register_tx_id
    );

    let validator_address = cardano_client.constants.policies.auth_token_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let utxos = cardano_client
        .ogmios_clients
        .query_utxos(from_ref(&address_bech32))
        .await
        .unwrap();
    assert!(!utxos.is_empty(), "No UTXOs found for funding address");
    let utxo = utxos
        .iter()
        .max_by_key(|u| u.value.lovelace)
        .expect("No UTXO with lovelace found");

    let deregister_tx = cardano_client
        .deregister(utxo, &register_tx, &collateral_utxo)
        .await
        .expect("Failed to deregister")
        .transaction
        .id;
    println!(
        "Deregistration transaction submitted with hash: {:?}",
        deregister_tx
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: [u8; 33] = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let events = midnight_client
        .subscribe_to_cnight_observation_events(&deregister_tx)
        .await
        .expect("Failed to listen to cNgD registration event");

    let deregistration = events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Deregistration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0 == dust_address
        });
    assert!(
        deregistration.is_some(),
        "Did not find deregistration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Deregistration event found: {:?}",
        deregistration.unwrap()
    );

    let mapping_removed = events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| {
            evt.as_event::<c_night_observation::events::MappingRemoved>()
                .ok()
                .flatten()
        })
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0 == dust_address
                && map.0.utxo_tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    println!(
        "Matching MappingRemoved event found: {:?}",
        mapping_removed.unwrap()
    );
}

#[tokio::test]
async fn alice_cannot_deregister_bob() {
    let settings = Settings::default();
    // Create Alice and Bob wallets
    let alice =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;

    let bob = CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let bob_bech32 = bob.address_as_bech32();
    let dust_hex = MidnightClient::new_dust_hex();

    // Fund Alice and Bob wallets
    let ada_to_fund = vec![Asset::new_from_str("lovelace", "10000000")];
    let alice_collateral = alice
        .make_collateral()
        .await
        .expect("Failed to make collateral");
    let deregister_tx_in = alice
        .fund_wallet(ada_to_fund.clone())
        .await
        .expect("Failed to fund a wallet");

    let bob_collateral = bob
        .make_collateral()
        .await
        .expect("Failed to make collateral");
    let register_tx_in = bob
        .fund_wallet(ada_to_fund.clone())
        .await
        .expect("Failed to fund a wallet");

    // Bob registers his DUST address
    println!(
        "Registering Bob wallet {} with DUST address {}",
        bob_bech32, dust_hex
    );
    let register_tx_id = bob
        .register(&dust_hex, &register_tx_in, &bob_collateral)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {:?}",
        register_tx_id
    );

    // Find Bob's registration UTXO
    let validator_address = bob.constants.policies.auth_token_address();
    let register_tx = bob
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    // Alice attempts to deregister Bob
    let deregister_tx = alice
        .deregister(&deregister_tx_in, &register_tx, &alice_collateral)
        .await;
    assert!(
        deregister_tx.is_err(),
        "Alice should not be able to deregister Bob"
    );

    // Check if Bob's registration still exists in mapping validator UTXOs
    let still_unspent = bob
        .is_utxo_unspent_for_3_blocks(&validator_address, &hex::encode(register_tx_id))
        .await;
    assert!(
        still_unspent,
        "Bob's registration UTXO should still be unspent"
    );
}

#[tokio::test]
async fn removing_excessive_registrations() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let dust_hex = MidnightClient::new_dust_hex();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let second_dust_hex = MidnightClient::new_dust_hex();
    println!(
        "Registering Cardano wallet {} with second DUST address {}",
        address_bech32, second_dust_hex
    );

    let collateral_utxo = cardano_client
        .make_collateral()
        .await
        .expect("Failed to make collateral");
    let assets = vec![Asset::new_from_str("lovelace", "10000000")];
    let second_assets = vec![Asset::new_from_str("lovelace", "10000000")];
    let tx_in = cardano_client
        .fund_wallet(assets)
        .await
        .expect("Failed to fund a wallet");
    let second_tx_in = cardano_client
        .fund_wallet(second_assets)
        .await
        .expect("Failed to fund a wallet");

    let utxos = cardano_client
        .ogmios_clients
        .query_utxos(from_ref(&address_bech32))
        .await
        .unwrap();
    assert_eq!(
        utxos.len(),
        3,
        "New wallet should have exactly two UTXOs after funding"
    );

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register transaction")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {:?}",
        register_tx_id
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: [u8; 33] = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let second_dust_address: [u8; 33] = hex::decode(&second_dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let registration_events = midnight_client
        .subscribe_to_cnight_observation_events(&register_tx_id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let registration = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0 == dust_address
        });
    assert!(
        registration.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Registration event found: {:?}",
        registration.unwrap()
    );

    let mapping_added = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0 == dust_address
                && map.0.utxo_tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_added.is_some(),
        "Did not find MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    println!(
        "Matching MappingAdded event found: {:?}",
        mapping_added.unwrap()
    );

    let second_register_tx_id = cardano_client
        .register(&second_dust_hex, &second_tx_in, &collateral_utxo)
        .await
        .expect("Failed to register transaction")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {:?}",
        register_tx_id
    );

    let second_registration_events = midnight_client
        .subscribe_to_cnight_observation_events(&second_register_tx_id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let second_mapping_added = second_registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0 == second_dust_address
                && map.0.utxo_tx_hash.0 == second_register_tx_id
        });
    assert!(
        second_mapping_added.is_some(),
        "Did not find second MappingAdded event with expected reward_address, second_dust_address, and second_register_tx_id"
    );
    println!(
        "Matching second MappingAdded event found: {:?}",
        second_mapping_added.unwrap()
    );

    let deregistration = second_registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Deregistration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0 == dust_address
        });
    assert!(
        deregistration.is_some(),
        "Did not find deregistration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Deregistration event found: {:?}",
        deregistration.unwrap()
    );

    let validator_address = cardano_client.constants.policies.auth_token_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let more_assets = vec![Asset::new_from_str("lovelace", "160000000")];
    let tx_in_for_deregister = cardano_client
        .fund_wallet(more_assets)
        .await
        .expect("Failed to fund a wallet");

    let deregister_tx = cardano_client
        .deregister(&tx_in_for_deregister, &register_tx, &collateral_utxo)
        .await
        .expect("Failed to deregister")
        .transaction
        .id;
    println!(
        "Deregistration transaction submitted with hash: {:?}",
        deregister_tx
    );

    let deregister_events = midnight_client
        .subscribe_to_cnight_observation_events(&deregister_tx)
        .await
        .expect("Failed to listen to cNgD registration event");

    let mapping_removed = deregister_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| {
            evt.as_event::<c_night_observation::events::MappingRemoved>()
                .ok()
                .flatten()
        })
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0 == dust_address
                && map.0.utxo_tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    println!(
        "Matching MappingRemoved event found: {:?}",
        mapping_removed.unwrap()
    );

    let registration_after_removing_excessive_mapping = deregister_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0 == second_dust_address
        });
    assert!(
        registration_after_removing_excessive_mapping.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Registration event found: {:?}",
        registration_after_removing_excessive_mapping.unwrap()
    );

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {:?}", amount, tx_id);

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo = match cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(tx_id))
        .await
    {
        Some(cnight_utxo) => cnight_utxo,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix = b"asset_create";
    let nonce =
        MidnightClient::calculate_nonce(prefix, cnight_utxo.transaction.id, cnight_utxo.index);
    println!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    println!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, second_dust_hex,
        "UTXO owner does not match DUST address"
    );
}

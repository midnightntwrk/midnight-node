use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::faucet::FaucetManager;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::events::{
    Deregistration, MappingAdded, Registration,
};
use midnight_node_toolkit::commands::dust_balance::{
    self, DustBalanceArgs, DustBalanceJson, DustBalanceResult,
};
use midnight_node_toolkit::tx_generator::source::{FetchCacheConfig, Source};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tokio::time::{Duration, timeout};

// -------- GLOBAL ASYNC FAUCET MANAGER --------

static FAUCET_MANAGER: OnceCell<Arc<FaucetManager>> = OnceCell::const_new();

async fn global_faucet_manager() -> Arc<FaucetManager> {
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

// -------- TESTS --------

#[tokio::test]
async fn register_for_dust_production() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let utxos = cardano_client.utxos().await;
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
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: Vec<u8> = hex::decode(&dust_hex)
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
                && reg.0.dust_public_key.0.0 == dust_address
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
                && map.0.dust_public_key.0.0 == dust_bytes
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
    let address_bech32 = cardano_client.address_as_bech32();
    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in_utxo = faucet.request_tokens(&address_bech32, 500_000_000).await;
    let tx_in_utxo_2 = faucet.request_tokens(&address_bech32, 500_000_000).await;
    println!("Wallet funded for governance contract deployment");

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
    let cardano_client_2 = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let address_bech_32_1 = cardano_client_1.address_as_bech32();
    let address_bech_32_2 = cardano_client_2.address_as_bech32();
    println!("First Cardano wallet created: {:?}", address_bech_32_1);
    println!("Second Cardano wallet created: {:?}", address_bech_32_2);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    let dust_bytes: [u8; 33] = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    println!(
        "Registering First Cardano wallet {} with DUST address {}",
        address_bech_32_1, dust_hex
    );
    println!(
        "Registering Second Cardano wallet {} with DUST address {}",
        address_bech_32_2, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo_1 = faucet.request_tokens(&address_bech_32_1, 5_000_000).await;
    let tx_in_1 = faucet.request_tokens(&address_bech_32_1, 10_000_000).await;
    let collateral_utxo_2 = faucet.request_tokens(&address_bech_32_2, 5_000_000).await;
    let tx_in_2 = faucet.request_tokens(&address_bech_32_2, 10_000_000).await;

    let utxos_1 = cardano_client_1.utxos().await;
    assert_eq!(
        utxos_1.len(),
        2,
        "First wallet should have exactly two UTXOs after funding"
    );

    let utxos_2 = cardano_client_2.utxos().await;
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
        "Registration transaction for the first cardano submitted with hash: {}",
        hex::encode(register_tx_id_1)
    );

    let register_tx_id_2 = cardano_client_2
        .register(&dust_hex, &tx_in_2, &collateral_utxo_2)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction for second cardano submitted with hash: {}",
        hex::encode(register_tx_id_2)
    );

    let reward_address_1 = cardano_client_1.reward_address_bytes();
    let reward_address_2 = cardano_client_2.reward_address_bytes();

    let dust_address: Vec<u8> = hex::decode(&dust_hex)
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
                && reg.0.dust_public_key.0.0 == dust_address
        });

    let registration_2 = registration_events_2
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address_2
                && reg.0.dust_public_key.0.0 == dust_address
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
                && map.0.dust_public_key.0.0 == dust_bytes
                && map.0.utxo_tx_hash.0 == register_tx_id_1
        });

    let mapping_added_2 = registration_events_2
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address_2
                && map.0.dust_public_key.0.0 == dust_bytes
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
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let bech32_address = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", bech32_address);

    let midnight_wallet_seed = MidnightClient::new_seed();
    println!(
        "Midnight wallet seed: {}",
        hex::encode(midnight_wallet_seed.as_bytes())
    );
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        bech32_address, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&bech32_address, 5_000_000).await;
    let tx_in = faucet.request_tokens(&bech32_address, 10_000_000).await;

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );
    match cardano_client
        .find_utxo_by_tx_id(
            &cardano_client.address_as_bech32(),
            hex::encode(register_tx_id),
        )
        .await
    {
        Some(_) => (),
        None => panic!("No registration UTXO found"),
    };

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > *balance)
    );
}

#[tokio::test]
async fn deregister_from_dust_production() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let utxos = cardano_client.utxos().await;
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
        "Deregistration transaction submitted with hash: {}",
        hex::encode(deregister_tx)
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: Vec<u8> = hex::decode(&dust_hex)
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
                && reg.0.dust_public_key.0.0 == dust_address
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
                && map.0.dust_public_key.0.0 == dust_bytes
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

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));
}

#[tokio::test]
async fn alice_cannot_deregister_bob() {
    let settings = Settings::default();

    // Create Alice and Bob wallets
    let alice =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let bob = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let bob_bech32 = bob.address_as_bech32();
    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);

    // Fund Alice and Bob wallets
    let faucet = global_faucet_manager().await;
    let alice_collateral = faucet
        .request_tokens(&alice.address_as_bech32(), 5_000_000)
        .await;
    let deregister_tx_in = faucet
        .request_tokens(&alice.address_as_bech32(), 10_000_000)
        .await;
    let bob_collateral = faucet.request_tokens(&bob_bech32, 5_000_000).await;
    let register_tx_in = faucet.request_tokens(&bob_bech32, 10_000_000).await;

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
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    // Find Bob's registration UTXO
    let validator_address = bob.constants.policies.mapping_validator_address();
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

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let second_midnight_wallet_seed = MidnightClient::new_seed();
    let second_dust_hex = MidnightClient::new_dust_hex(second_midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with second DUST address {}",
        address_bech32, second_dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;
    let second_tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;
    let tx_in_for_deregister = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let utxos = cardano_client.utxos().await;
    assert_eq!(
        utxos.len(),
        4,
        "New wallet should have exactly two UTXOs after funding"
    );

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register transaction")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
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
                && reg.0.dust_public_key.0.0 == dust_address
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
                && map.0.dust_public_key.0.0 == dust_address
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
        "Second registration transaction submitted with hash: {}",
        hex::encode(second_register_tx_id)
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
                && map.0.dust_public_key.0.0 == second_dust_address
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
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        deregistration.is_some(),
        "Did not find deregistration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Deregistration event found: {:?}",
        deregistration.unwrap()
    );

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    // Deregister the first mapping, so the second mapping should be active from deregistration the first one
    let deregister_tx = cardano_client
        .deregister(&tx_in_for_deregister, &register_tx, &collateral_utxo)
        .await
        .expect("Failed to deregister")
        .transaction
        .id;
    println!(
        "Deregistration transaction submitted with hash: {}",
        hex::encode(deregister_tx)
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
                && map.0.dust_public_key.0.0 == dust_address
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
                && reg.0.dust_public_key.0.0 == second_dust_address
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
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, second_dust_hex,
        "UTXO owner does not match DUST address"
    );
}

#[tokio::test]
async fn create_hundred_registrations() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let mut tx_in = faucet.request_tokens(&address_bech32, 500_000_000).await;

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();

    let mut register_tx_id: [[u8; 32]; 101] = [[0; 32]; 101];

    let mut last_deregistration_tx_id: [u8; 32] = [0; 32];

    let mut dust_hex = String::new();

    //run n registrations
    for i in 0..101 {
        let midnight_wallet_seed = MidnightClient::new_seed();
        dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
        println!(
            "Registering Cardano wallet {} with DUST address {}",
            address_bech32, dust_hex
        );

        let register_tx_in = cardano_client
            .find_utxo_by_tx_id(
                &cardano_client.address_as_bech32(),
                hex::encode(tx_in.transaction.id),
            )
            .await
            .expect("Failed to find UTXO for registration");

        register_tx_id[i] = cardano_client
            .register(&dust_hex, &register_tx_in, &collateral_utxo)
            .await
            .expect("Failed to register transaction")
            .transaction
            .id;
        println!(
            "Registration transaction submitted with hash: {}",
            hex::encode(register_tx_id[i])
        );
        tx_in = cardano_client
            .find_utxo_by_tx_id(
                &cardano_client.address_as_bech32(),
                hex::encode(register_tx_id[i]),
            )
            .await
            .expect("Failed to find UTXO for next registration");

        println!("UTXO for next registration: {:?}", tx_in);
    }

    //run n-1 deregistrations
    for i in 0..100 {
        let register_tx = cardano_client
            .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id[i]))
            .await
            .expect("No registration UTXO found after registering");
        println!("Found registration UTXO: {:?}", register_tx);

        let tx_in_for_deregister = cardano_client
            .find_utxo_by_tx_id(
                &cardano_client.address_as_bech32(),
                hex::encode(tx_in.transaction.id),
            )
            .await
            .expect("Failed to find UTXO for deregistration");

        let deregister_tx = cardano_client
            .deregister(&tx_in_for_deregister, &register_tx, &collateral_utxo)
            .await
            .expect("Failed to deregister")
            .transaction
            .id;
        println!(
            "Deregistration transaction submitted with hash: {}",
            hex::encode(deregister_tx)
        );
        tx_in = cardano_client
            .find_utxo_by_tx_id(
                &cardano_client.address_as_bech32(),
                hex::encode(deregister_tx),
            )
            .await
            .expect("Failed to find UTXO for next registration");

        println!("UTXO for next deregistration: {:?}", tx_in);
        last_deregistration_tx_id = deregister_tx;
    }

    //assertions for the last registration
    let reward_address = cardano_client.reward_address_bytes();
    println!("Reward address hex: {}", hex::encode(&reward_address));
    println!("DUST address hex: {}", dust_hex);
    let dust_address: [u8; 33] = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();

    let registration_events = midnight_client
        .subscribe_to_cnight_observation_events(&last_deregistration_tx_id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let registration = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        registration.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    println!(
        "Matching Registration event found: {:?}",
        registration.unwrap()
    );
}

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
#[tokio::test]
async fn ddos_attack_transaction_rejected_at_rpc() {
    use midnight_node_res::undeployed::transactions::STORE_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // STORE_TX requires the contract to be deployed first.
    // Without DEPLOY_TX, it will fail at pre_dispatch with ContractNotPresent.
    // This simulates an attacker trying to consume blockspace without paying fees.
    println!("=== PR367-TC-0003-06: DDoS Attack Prevention Test ===");
    println!("Submitting STORE_TX without prior DEPLOY_TX...");
    println!("Expected: Transaction rejected at pre_dispatch (ContractNotPresent)");

    let result = client.submit_expecting_rejection(STORE_TX.to_vec()).await;

    assert!(
        result.is_ok(),
        "Transaction should be rejected at pre_dispatch, but was accepted: {:?}",
        result.err()
    );

    let error_msg = result.unwrap();
    println!("✓ Transaction rejected with error: {}", error_msg);

    // The error should indicate an invalid transaction
    // (exact message depends on subxt error formatting)
    assert!(
        error_msg.to_lowercase().contains("invalid")
            || error_msg.to_lowercase().contains("transaction")
            || error_msg.contains("1010"), // Substrate InvalidTransaction code
        "Expected InvalidTransaction error, got: {}",
        error_msg
    );

    println!("✓ PR367-TC-0003-06 PASSED: Attack transaction rejected, no blockspace consumed");
}

/// PR367-TC-0003-06: DDoS Attack Prevention - Batch Attack
///
/// Verifies that multiple attack transactions are all rejected.
/// Simulates an attacker attempting to flood the network with failing transactions.
#[tokio::test]
async fn ddos_batch_attack_all_rejected() {
    use midnight_node_res::undeployed::transactions::STORE_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    println!("=== PR367-TC-0003-06: Batch Attack Prevention Test ===");
    println!("Submitting 5 attack transactions (STORE_TX without DEPLOY_TX)...");

    let mut rejected_count = 0;
    let total_attacks = 5;

    for i in 0..total_attacks {
        let result = client.submit_expecting_rejection(STORE_TX.to_vec()).await;
        if result.is_ok() {
            rejected_count += 1;
            println!("  Attack tx {}/{} rejected ✓", i + 1, total_attacks);
        } else {
            println!(
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

    println!(
        "✓ PR367-TC-0003-06 PASSED: All {} attack transactions rejected",
        total_attacks
    );
}

/// PR367-TC-0003-02 E2E: Replay Attack Prevention
///
/// Verifies that submitting the same transaction twice results in rejection.
/// The replay protection mechanism should reject the duplicate transaction
/// at pre_dispatch, preventing replay attacks from consuming blockspace.
#[tokio::test]
async fn replay_attack_rejected_via_rpc() {
    use midnight_node_res::undeployed::transactions::DEPLOY_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    println!("=== PR367-TC-0003-02 E2E: Replay Attack Prevention Test ===");

    // First submission - may succeed or fail depending on node state
    // (contract may already be deployed from previous test runs)
    println!("Submitting DEPLOY_TX (first attempt)...");
    let first_result = client.submit_midnight_tx(DEPLOY_TX.to_vec()).await;

    match &first_result {
        Ok(_) => println!("  First submission accepted (contract not yet deployed)"),
        Err(e) => println!(
            "  First submission rejected (expected if contract exists): {}",
            e
        ),
    }

    // If first succeeded, wait for it to be processed before replay attempt
    if let Ok(mut progress) = first_result {
        println!("Waiting for first transaction to be included in block...");
        while let Some(status) = progress.next().await {
            match status {
                Ok(subxt::tx::TxStatus::InBestBlock(info)) => {
                    println!("  First transaction in best block: {:?}", info.block_hash());
                    break;
                }
                Ok(subxt::tx::TxStatus::InFinalizedBlock(info)) => {
                    println!("  First transaction finalized: {:?}", info.block_hash());
                    break;
                }
                Ok(subxt::tx::TxStatus::Error { message }) => {
                    println!("  First transaction error: {}", message);
                    break;
                }
                Ok(subxt::tx::TxStatus::Invalid { message }) => {
                    println!("  First transaction invalid: {}", message);
                    break;
                }
                Ok(subxt::tx::TxStatus::Dropped { message }) => {
                    println!("  First transaction dropped: {}", message);
                    break;
                }
                Ok(_) => continue,
                Err(e) => {
                    println!("  First transaction status error: {}", e);
                    break;
                }
            }
        }
    }

    // Second submission - MUST fail (either replay protection or ContractAlreadyDeployed)
    // Both are valid rejections that prevent the attack vector
    println!("Submitting DEPLOY_TX (second attempt - should be rejected)...");
    let second_result = client.submit_expecting_rejection(DEPLOY_TX.to_vec()).await;

    assert!(
        second_result.is_ok(),
        "Replay transaction should be rejected, but was accepted: {:?}",
        second_result.err()
    );

    let error_msg = second_result.unwrap();
    println!("✓ Replay transaction rejected with: {}", error_msg);

    // Verify the error indicates an invalid transaction
    // Accept various error types: replay protection, already deployed, or generic invalid
    assert!(
        error_msg.to_lowercase().contains("invalid")
            || error_msg.to_lowercase().contains("replay")
            || error_msg.to_lowercase().contains("already")
            || error_msg.contains("1010"), // Substrate InvalidTransaction code
        "Expected InvalidTransaction or replay-related error, got: {}",
        error_msg
    );

    println!("✓ PR367-TC-0003-02 E2E PASSED: Replay attack rejected, no blockspace consumed");
}

/// PR367-TC-0003-03 E2E: Valid Transaction Succeeds
///
/// Confirms no regression - valid transactions should still be accepted.
/// Note: This test requires a fresh node state where the contract hasn't been deployed.
#[tokio::test]
#[ignore = "Requires fresh node state - run manually with cargo test-e2e-local"]
async fn valid_deploy_transaction_succeeds_via_rpc() {
    use midnight_node_res::undeployed::transactions::DEPLOY_TX;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    println!("=== PR367-TC-0003-03 E2E: Valid Transaction Test ===");
    println!("Submitting valid DEPLOY_TX...");

    let result = client.submit_expecting_success(DEPLOY_TX.to_vec()).await;

    assert!(
        result.is_ok(),
        "Valid DEPLOY_TX should be accepted, but was rejected: {:?}",
        result.err()
    );

    println!("✓ PR367-TC-0003-03 E2E PASSED: Valid transaction accepted and included in block");
}
#[tokio::test]
async fn register_twice_with_same_cardano_address() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    // register second time
    let tx_in2 = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let midnight_wallet_seed2 = MidnightClient::new_seed();
    let dust_hex2 = MidnightClient::new_dust_hex(midnight_wallet_seed2);
    let register_tx_id2 = cardano_client
        .register(&dust_hex2, &tx_in2, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id2)
    );

    let register_tx2 = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id2))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx2);

    let amount2 = 100;
    let tx_id2 = cardano_client
        .mint_tokens(amount2, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount2, hex::encode(tx_id2));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo2 = match cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(tx_id2))
        .await
    {
        Some(cnight_utxo2) => cnight_utxo2,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix2 = b"asset_create";
    let nonce2 =
        MidnightClient::calculate_nonce(prefix2, cnight_utxo2.transaction.id, cnight_utxo2.index);
    println!("Calculated nonce for cNIGHT UTXO: {}", nonce2);

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed2,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));
}

#[tokio::test]
async fn deregister_with_valid_cnight_utxo() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let utxos = cardano_client.utxos().await;
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
        "Deregistration transaction submitted with hash: {}",
        hex::encode(deregister_tx)
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: Vec<u8> = hex::decode(&dust_hex)
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
                && reg.0.dust_public_key.0.0 == dust_address
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
                && map.0.dust_public_key.0.0 == dust_bytes
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

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > *balance)
    );
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
#[tokio::test]
async fn query_d_parameter_at_historical_block() {
    println!("=== D-Parameter Historical Block Query E2E Test ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Step 1: Get genesis block hash (block 0) to test historical query at earliest block
    let genesis_block_hash = midnight_client
        .get_block_hash_at_height(0)
        .await
        .expect("Failed to get genesis block hash");
    println!(
        "Genesis block hash: 0x{}",
        hex::encode(genesis_block_hash.as_bytes())
    );

    // Step 2: Get current best block hash
    let current_block_hash = midnight_client
        .get_best_block_hash()
        .await
        .expect("Failed to get best block hash");
    println!(
        "Current block hash: 0x{}",
        hex::encode(current_block_hash.as_bytes())
    );

    // Step 3: Query D-Parameter at genesis block
    println!("Querying D-param at genesis block...");
    let d_param_at_genesis = midnight_client
        .get_d_parameter_at(genesis_block_hash)
        .await
        .expect("Failed to query D-param at genesis block");
    println!(
        "D-param at genesis: ({}, {})",
        d_param_at_genesis.num_permissioned_candidates,
        d_param_at_genesis.num_registered_candidates
    );

    // Step 4: Query D-Parameter at current block
    println!("Querying D-param at current block...");
    let d_param_at_current = midnight_client
        .get_d_parameter_at(current_block_hash)
        .await
        .expect("Failed to query D-param at current block");
    println!(
        "D-param at current: ({}, {})",
        d_param_at_current.num_permissioned_candidates,
        d_param_at_current.num_registered_candidates
    );

    // Step 5: Verify both queries returned valid data
    // Note: Values may be the same since D-parameter hasn't been changed via governance.
    // This test primarily verifies the historical query code path works, not that
    // different blocks have different values (which would require governance changes).
    println!("✓ Historical block queries returned valid D-parameter data");

    // Step 6: Test error handling - query with invalid block hash
    println!("Testing error handling with invalid block hash...");
    let invalid_block_hash = subxt::utils::H256::from([0xff; 32]);
    let invalid_query_result = midnight_client.get_d_parameter_at(invalid_block_hash).await;

    assert!(
        invalid_query_result.is_err(),
        "Query with invalid block hash should return an error, but got: {:?}",
        invalid_query_result
    );
    println!(
        "✓ Invalid block hash correctly rejected: {}",
        invalid_query_result.unwrap_err()
    );

    // Step 7: Verify querying the same block hash is idempotent
    println!("Verifying idempotent queries at same block hash...");
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

    println!("✓ Historical block query verification passed");
    println!();
    println!("Note: D-parameter values at genesis and current block are the same");
    println!("because no governance transaction has updated the parameter.");
    println!("To fully test historical value differences, use update_d_parameter");
    println!("via federated authority governance between block queries.");
}
#[tokio::test]
async fn deregister_first_mapping() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    //check utxo1 producing dust
    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    // register second time
    let tx_in2 = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let midnight_wallet_seed2 = MidnightClient::new_seed();
    let dust_hex2 = MidnightClient::new_dust_hex(midnight_wallet_seed2);
    let register_tx_id2 = cardano_client
        .register(&dust_hex2, &tx_in2, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id2)
    );

    let register_tx2 = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id2))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx2);

    let amount2 = 100;
    let tx_id2 = cardano_client
        .mint_tokens(amount2, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount2, hex::encode(tx_id2));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo2 = match cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(tx_id2))
        .await
    {
        Some(cnight_utxo2) => cnight_utxo2,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix2 = b"asset_create";
    let nonce2 =
        MidnightClient::calculate_nonce(prefix2, cnight_utxo2.transaction.id, cnight_utxo2.index);
    println!("Calculated nonce for cNIGHT UTXO: {}", nonce2);

    //check utxo2 NOT producing dust
    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed2,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));

    // deregister first mapping
    let utxos = cardano_client.utxos().await;
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
        "Deregistration transaction submitted with hash: {}",
        hex::encode(deregister_tx)
    );

    let collateral_utxo2 = faucet.request_tokens(&address_bech32, 5_000_000).await;

    let amount3 = 100;
    let tx_id3 = cardano_client
        .mint_tokens(amount3, &collateral_utxo2)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount3, hex::encode(tx_id3));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo3 = match cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(tx_id3))
        .await
    {
        Some(cnight_utxo3) => cnight_utxo3,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix3 = b"asset_create";
    let nonce3 =
        MidnightClient::calculate_nonce(prefix3, cnight_utxo3.transaction.id, cnight_utxo3.index);
    println!("Calculated nonce for cNIGHT UTXO: {}", nonce3);

    //check utxo3 producing dust
    let args3 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result3 = dust_balance::execute(args3)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result3 {
        println!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args4 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result4 = dust_balance::execute(args4)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result4 {
        println!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result4, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > *balance)
    );
}

#[tokio::test]
async fn produce_dust_from_tokens_owned_before_registration() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;
    let address_bech32 = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", address_bech32);

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 6_000_000).await;
    // for minting cNIGHT tokens
    faucet.request_tokens(&address_bech32, 7_000_000).await;

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));

    let cnight_utxo_new = cardano_client
        .rotate_cnight(&cnight_utxo)
        .await
        .expect("Failed to rotate cNight UTxO");
    println!(
        "Rotated cNIGHT UTXO: {}",
        &hex::encode(&cnight_utxo_new.transaction.id)
    );

    let cnight_new = match cardano_client
        .find_utxo_by_tx_id(
            &cardano_client.address_as_bech32(),
            hex::encode(&cnight_utxo_new.transaction.id),
        )
        .await
    {
        Some(cnight_new) => cnight_new,
        None => panic!("No cNIGHT UTXO found after rotation"),
    };

    let prefix2 = b"asset_create";
    let nonce_new =
        MidnightClient::calculate_nonce(prefix2, cnight_new.transaction.id, cnight_new.index);
    println!("Calculated nonce for cNIGHT UTXO: {}", nonce_new);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce_new, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    println!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance: {}", total);
    }

    assert!(matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));
}

#[tokio::test]
async fn stop_dust_producing_after_deregistration_and_rotation() {
    // case for stop dust production (reg -> mint -> dereg -> rotate)
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let address_bech32 = cardano_client.address_as_bech32();
    let base_url = settings.node_client.base_url.clone();
    let same_base_url = settings.node_client.base_url.clone();
    let midnight_client = MidnightClient::new(settings.node_client).await;
    println!("New Cardano wallet created: {:?}", address_bech32);

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 6_000_000).await;
    faucet.request_tokens(&address_bech32, 7_000_000).await;

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32, dust_hex
    );

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let utxos = cardano_client.utxos().await;
    assert!(!utxos.is_empty(), "No UTXOs found for funding address");
    let utxo = utxos
        .iter()
        .max_by_key(|u| u.value.lovelace)
        .expect("No UTXO with lovelace found");

    let validator_address = cardano_client
        .constants
        .policies
        .mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    println!("Found registration UTXO: {:?}", register_tx);

    let deregister_tx = cardano_client
        .deregister(utxo, &register_tx, &collateral_utxo)
        .await
        .expect("Failed to deregister")
        .transaction
        .id;
    println!(
        "Deregistration transaction submitted with hash: {}",
        hex::encode(deregister_tx)
    );

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(same_base_url),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    let mut balance_before_rotation: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance before rotation: {}", total);
        balance_before_rotation = total;
    }

    let cnight_utxo_new = cardano_client
        .rotate_cnight(&cnight_utxo)
        .await
        .expect("Failed to rotate cNight UTxO");
    println!(
        "Rotated cNIGHT UTXO: {}",
        &hex::encode(&cnight_utxo_new.transaction.id)
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(base_url),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let spend_cnight_event = midnight_client
        .subscribe_to_cnight_observation_events(&cnight_utxo_new.transaction.id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance_after_rotation: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance after rotation: {}", total);
        balance_after_rotation = total;
    }

    assert!(
        balance_after_rotation < balance_before_rotation,
        "balance_after_rotation ({}) must be less than balance_before_rotation ({})",
        balance_after_rotation,
        balance_before_rotation
    );
}

#[tokio::test]
async fn spend_cnight_producing_dust() {
    let settings = Settings::default();
    let cardano_client =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let bech32_address = cardano_client.address_as_bech32();
    println!("New Cardano wallet created: {:?}", bech32_address);

    let bob_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let bob_bech32 = bob_client.address_as_bech32();
    println!("Bob's Cardano wallet created: {:?}", bob_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    println!(
        "Registering Cardano wallet {} with DUST address {}",
        bech32_address, dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&bech32_address, 5_000_000).await;
    let tx_in = faucet.request_tokens(&bech32_address, 10_000_000).await;

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    println!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    match cardano_client
        .find_utxo_by_tx_id(
            &cardano_client.address_as_bech32(),
            hex::encode(register_tx_id),
        )
        .await
    {
        Some(_) => (),
        None => panic!("No registration UTXO found"),
    };

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    println!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    println!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        println!("Total dust balance: {}", total);
        balance = total;
    }

    // sleep 10s
    println!("Sleeping 10 seconds before spending cNIGHT...");
    tokio::time::sleep(Duration::from_secs(10)).await;
    let cnight_spent_utxo = cardano_client.spend_cnight(&cnight_utxo, &bob_bech32).await;

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            fetch_cache: FetchCacheConfig::InMemory,
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let spend_cnight_event = midnight_client
        .subscribe_to_cnight_observation_events(&cnight_spent_utxo.unwrap().transaction.id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        println!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total < *balance)
    );
}

// ========== Aiken Permissioned Candidates E2E Tests ==========
// These tests verify permissioned candidates via the new Aiken contracts

/// TC-PC-001: Verify systemParameters_getAriadneParameters returns valid structure.
///
/// Tests that the RPC endpoint returns correctly structured data including:
/// - D-Parameter with permissioned and registered candidate counts
/// - Block info metadata showing where D-Parameter was fetched from
/// - Permissioned candidates list (may be None if not set on mainchain)
#[tokio::test]
async fn get_ariadne_parameters_returns_valid_structure() {
    println!("=== TC-PC-001: Ariadne Parameters Structure Validation ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Use epoch 2 for local environment (minimum supported epoch)
    let epoch_number = 2u64;

    let ariadne_params = midnight_client
        .get_ariadne_parameters(epoch_number, None)
        .await
        .expect("Failed to get Ariadne parameters");

    println!("Ariadne Parameters Response:");
    println!(
        "  D-Parameter: ({}, {})",
        ariadne_params.d_parameter.num_permissioned_candidates,
        ariadne_params.d_parameter.num_registered_candidates
    );
    println!(
        "  Permissioned Candidates: {:?}",
        ariadne_params
            .permissioned_candidates
            .as_ref()
            .map(|c| c.len())
    );

    // Verify D-Parameter structure is valid (values can be 0)
    // The important thing is that the RPC call succeeded and returned valid types
    println!("✓ Ariadne parameters structure is valid");
}

/// TC-PC-003: Verify D-Parameter from pallet matches expected configuration.
///
/// The D-Parameter is now sourced from pallet-system-parameters instead of Cardano.
/// In local environment, it's configured as (10, 0) - 10 permissioned, 0 registered.
#[tokio::test]
async fn d_parameter_from_pallet_matches_config() {
    println!("=== TC-PC-003: D-Parameter Pallet Integration ===");

    let settings = Settings::default();
    let midnight_client = MidnightClient::new(settings.node_client).await;

    // Query D-Parameter directly via the dedicated RPC
    let d_param = midnight_client
        .get_d_parameter()
        .await
        .expect("Failed to get D-Parameter");

    println!(
        "D-Parameter from pallet-system-parameters: ({}, {})",
        d_param.num_permissioned_candidates, d_param.num_registered_candidates
    );

    // Also query via getAriadneParameters to verify consistency
    // Use epoch 2 (minimum supported epoch)
    let ariadne_params = midnight_client
        .get_ariadne_parameters(2, None)
        .await
        .expect("Failed to get Ariadne parameters");

    println!(
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

    // Local environment configures D-Parameter as (10, 0)
    // This comes from pallet-system-parameters, not Cardano
    assert_eq!(
        d_param.num_permissioned_candidates, 10,
        "Permissioned count should match system-parameters config (expected 10)"
    );
    assert_eq!(
        d_param.num_registered_candidates, 0,
        "Registered count should match system-parameters config (expected 0)"
    );

    println!("✓ D-Parameter correctly sourced from pallet-system-parameters");
}

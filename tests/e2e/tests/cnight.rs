use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::events::{
    Deregistration, MappingAdded, Registration,
};
use midnight_node_toolkit::commands::dust_balance::{
    self, DustBalanceArgs, DustBalanceJson, DustBalanceResult,
};
use midnight_node_toolkit::tx_generator::source::{FetchCacheConfig, Source};
use std::collections::HashMap;
use std::collections::HashSet;
use tokio::time::Duration;

use crate::global_faucet_manager;

// -------- TESTS --------

#[e2e_test]
async fn register_for_dust_production() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
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
    tracing::info!(
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
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        registration.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Registration event found: {:?}",
        registration.unwrap()
    );

    let mapping_added = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.decode_fields_as::<MappingAdded>().and_then(|r| r.ok()))
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0.0 == dust_bytes
                && map.0.utxo_id.tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_added.is_some(),
        "Did not find MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingAdded event found: {:?}",
        mapping_added.unwrap()
    );
}

#[e2e_test]
async fn register_2_cardano_same_dust_address_production() {
    let settings = Settings::default();
    let base_url = settings.node_client.base_url.clone();
    let cardano_client_1 =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let cardano_client_2 = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let address_bech_32_1 = cardano_client_1.address_as_bech32();
    let address_bech_32_2 = cardano_client_2.address_as_bech32();
    tracing::info!("First Cardano wallet created: {:?}", address_bech_32_1);
    tracing::info!("Second Cardano wallet created: {:?}", address_bech_32_2);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    let dust_bytes: [u8; 33] = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    tracing::info!(
        "Registering First Cardano wallet {} with DUST address {}",
        address_bech_32_1,
        dust_hex
    );
    tracing::info!(
        "Registering Second Cardano wallet {} with DUST address {}",
        address_bech_32_2,
        dust_hex
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
    tracing::info!(
        "Registration transaction for the first cardano submitted with hash: {}",
        hex::encode(register_tx_id_1)
    );

    let register_tx_id_2 = cardano_client_2
        .register(&dust_hex, &tx_in_2, &collateral_utxo_2)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    tracing::info!(
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
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address_1
                && reg.0.dust_public_key.0.0 == dust_address
        });

    let registration_2 = registration_events_2
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
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

    tracing::info!(
        "Matching Registration event found: {:?}",
        registration_1.unwrap()
    );

    tracing::info!(
        "Matching Second Registration event found: {:?}",
        registration_2.unwrap()
    );

    let mapping_added_1 = registration_events_1
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.decode_fields_as::<MappingAdded>().and_then(|r| r.ok()))
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address_1
                && map.0.dust_public_key.0.0 == dust_bytes
                && map.0.utxo_id.tx_hash.0 == register_tx_id_1
        });

    let mapping_added_2 = registration_events_2
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.decode_fields_as::<MappingAdded>().and_then(|r| r.ok()))
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address_2
                && map.0.dust_public_key.0.0 == dust_bytes
                && map.0.utxo_id.tx_hash.0 == register_tx_id_2
        });
    assert!(
        mapping_added_1.is_some(),
        "Did not find first MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    assert!(
        mapping_added_2.is_some(),
        "Did not find second MappingAdded event with expected second_reward_address, dust_address, and utxo_id"
    );

    tracing::info!(
        "Matching first MappingAdded event found: {:?}",
        mapping_added_1.unwrap()
    );

    tracing::info!(
        "Matching second MappingAdded event found: {:?}",
        mapping_added_2.unwrap()
    );

    let amount = 100;
    let tx_id = cardano_client_1
        .mint_tokens(amount, &collateral_utxo_1)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo = match cardano_client_1
        .find_utxo_by_tx_id(&cardano_client_1.address_as_bech32(), hex::encode(tx_id))
        .await
    {
        Some(cnight_utxo) => cnight_utxo,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix = b"asset_create";
    let nonce =
        MidnightClient::calculate_nonce(prefix, cnight_utxo.transaction.id, cnight_utxo.index);
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let nonce_for_check = nonce.clone();

    let _amount2 = 100;
    let tx_id2 = cardano_client_2
        .mint_tokens(amount, &collateral_utxo_2)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id2));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo2 = match cardano_client_2
        .find_utxo_by_tx_id(&cardano_client_2.address_as_bech32(), hex::encode(tx_id2))
        .await
    {
        Some(cnight_utxo2) => cnight_utxo2,
        None => panic!("No cNIGHT UTXO found after minting"),
    };

    let prefix2 = b"asset_create";
    let nonce2 =
        MidnightClient::calculate_nonce(prefix2, cnight_utxo2.transaction.id, cnight_utxo2.index);
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce2);

    let nonce2_for_check = nonce2.clone();

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(base_url),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let mut sources: HashMap<String, u128> = HashMap::new();

    if let DustBalanceResult::Json(DustBalanceJson { source, .. }) = &result {
        tracing::info!("Sources ({}):", source.len());
        for (k, v) in source.iter() {
            tracing::info!("  {} => {}", k, v);
        }
        sources = source.clone();
    }

    assert_eq!(sources.len(), 2);

    if let DustBalanceResult::Json(DustBalanceJson {
        generation_infos, ..
    }) = &result
    {
        let actual: HashSet<String> = generation_infos
            .iter()
            .map(|p| p.dust_output.backing_night.clone())
            .collect();

        let expected: HashSet<String> = [nonce_for_check, nonce2_for_check].into_iter().collect();

        assert_eq!(actual, expected);
    } else {
        panic!("Waiting DustBalanceResult::Json(..)");
    }
}

#[e2e_test]
async fn cnight_produces_dust() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let bech32_address = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", bech32_address);

    let midnight_wallet_seed = MidnightClient::new_seed();
    tracing::info!(
        "Midnight wallet seed: {}",
        hex::encode(midnight_wallet_seed.as_bytes())
    );
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        bech32_address,
        dust_hex
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
    tracing::info!(
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
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
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
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > *balance)
    );
}

#[e2e_test]
async fn deregister_from_dust_production() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
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
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = config::mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

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
    tracing::info!(
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
        .filter_map(|evt| {
            evt.decode_fields_as::<Deregistration>()
                .and_then(|r| r.ok())
        })
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        deregistration.is_some(),
        "Did not find deregistration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Deregistration event found: {:?}",
        deregistration.unwrap()
    );

    let mapping_removed = events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| {
            evt.decode_fields_as::<c_night_observation::events::MappingRemoved>()
                .and_then(|r| r.ok())
        })
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0.0 == dust_bytes
                && map.0.utxo_id.tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingRemoved event found: {:?}",
        mapping_removed.unwrap()
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));
}

#[e2e_test]
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
    tracing::info!(
        "Registering Bob wallet {} with DUST address {}",
        bob_bech32,
        dust_hex
    );
    let register_tx_id = bob
        .register(&dust_hex, &register_tx_in, &bob_collateral)
        .await
        .expect("Failed to register")
        .transaction
        .id;
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    // Find Bob's registration UTXO
    let validator_address = config::mapping_validator_address();
    let register_tx = bob
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

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

#[e2e_test]
async fn removing_excessive_registrations() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    let second_midnight_wallet_seed = MidnightClient::new_seed();
    let second_dust_hex = MidnightClient::new_dust_hex(second_midnight_wallet_seed);
    tracing::info!(
        "Registering Cardano wallet {} with second DUST address {}",
        address_bech32,
        second_dust_hex
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
    tracing::info!(
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
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        registration.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Registration event found: {:?}",
        registration.unwrap()
    );

    let mapping_added = registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.decode_fields_as::<MappingAdded>().and_then(|r| r.ok()))
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0.0 == dust_address
                && map.0.utxo_id.tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_added.is_some(),
        "Did not find MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingAdded event found: {:?}",
        mapping_added.unwrap()
    );

    let second_register_tx_id = cardano_client
        .register(&second_dust_hex, &second_tx_in, &collateral_utxo)
        .await
        .expect("Failed to register transaction")
        .transaction
        .id;
    tracing::info!(
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
        .filter_map(|evt| evt.decode_fields_as::<MappingAdded>().and_then(|r| r.ok()))
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0.0 == second_dust_address
                && map.0.utxo_id.tx_hash.0 == second_register_tx_id
        });
    assert!(
        second_mapping_added.is_some(),
        "Did not find second MappingAdded event with expected reward_address, second_dust_address, and second_register_tx_id"
    );
    tracing::info!(
        "Matching second MappingAdded event found: {:?}",
        second_mapping_added.unwrap()
    );

    let deregistration = second_registration_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| {
            evt.decode_fields_as::<Deregistration>()
                .and_then(|r| r.ok())
        })
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        deregistration.is_some(),
        "Did not find deregistration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Deregistration event found: {:?}",
        deregistration.unwrap()
    );

    let validator_address = config::mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

    // Deregister the first mapping, so the second mapping should be active from deregistration the first one
    let deregister_tx = cardano_client
        .deregister(&tx_in_for_deregister, &register_tx, &collateral_utxo)
        .await
        .expect("Failed to deregister")
        .transaction
        .id;
    tracing::info!(
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
            evt.decode_fields_as::<c_night_observation::events::MappingRemoved>()
                .and_then(|r| r.ok())
        })
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0.0 == dust_address
                && map.0.utxo_id.tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingRemoved event found: {:?}",
        mapping_removed.unwrap()
    );

    let registration_after_removing_excessive_mapping = deregister_events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == second_dust_address
        });
    assert!(
        registration_after_removing_excessive_mapping.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    tracing::info!(
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
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, second_dust_hex,
        "UTXO owner does not match DUST address"
    );
}

#[e2e_test]
async fn create_hundred_registrations() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let mut tx_in = faucet.request_tokens(&address_bech32, 500_000_000).await;

    let validator_address = config::mapping_validator_address();

    let mut register_tx_id: [[u8; 32]; 101] = [[0; 32]; 101];

    let mut last_deregistration_tx_id: [u8; 32] = [0; 32];

    let mut dust_hex = String::new();

    //run n registrations
    for i in 0..101 {
        let midnight_wallet_seed = MidnightClient::new_seed();
        dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
        tracing::info!(
            "Registering Cardano wallet {} with DUST address {}",
            address_bech32,
            dust_hex
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
        tracing::info!(
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

        tracing::info!("UTXO for next registration: {:?}", tx_in);
    }

    //run n-1 deregistrations
    for i in 0..100 {
        let register_tx = cardano_client
            .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id[i]))
            .await
            .expect("No registration UTXO found after registering");
        tracing::info!("Found registration UTXO: {:?}", register_tx);

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
        tracing::info!(
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

        tracing::info!("UTXO for next deregistration: {:?}", tx_in);
        last_deregistration_tx_id = deregister_tx;
    }

    //assertions for the last registration
    let reward_address = cardano_client.reward_address_bytes();
    tracing::info!("Reward address hex: {}", hex::encode(&reward_address));
    tracing::info!("DUST address hex: {}", dust_hex);
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
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        registration.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Registration event found: {:?}",
        registration.unwrap()
    );
}

#[e2e_test]
async fn register_twice_with_same_cardano_address() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
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
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = config::mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    // register second time
    let tx_in2 = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let midnight_wallet_seed2 = MidnightClient::new_seed();
    let dust_hex2 = MidnightClient::new_dust_hex(midnight_wallet_seed2.clone());
    let register_tx_id2 = cardano_client
        .register(&dust_hex2, &tx_in2, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id2)
    );

    let register_tx2 = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id2))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx2);

    let amount2 = 100;
    let tx_id2 = cardano_client
        .mint_tokens(amount2, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount2, hex::encode(tx_id2));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce2);

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed2,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));
}

#[e2e_test]
async fn deregister_with_valid_cnight_utxo() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
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
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = config::mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
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
    tracing::info!(
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
        .filter_map(|evt| {
            evt.decode_fields_as::<Deregistration>()
                .and_then(|r| r.ok())
        })
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        deregistration.is_some(),
        "Did not find deregistration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Deregistration event found: {:?}",
        deregistration.unwrap()
    );

    let mapping_removed = events
        .iter()
        .filter_map(|e| e.ok())
        .filter_map(|evt| {
            evt.decode_fields_as::<c_night_observation::events::MappingRemoved>()
                .and_then(|r| r.ok())
        })
        .find(|map| {
            map.0.cardano_reward_address.0 == reward_address
                && map.0.dust_public_key.0.0 == dust_bytes
                && map.0.utxo_id.tx_hash.0 == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingRemoved event found: {:?}",
        mapping_removed.unwrap()
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > *balance)
    );
}

#[e2e_test]
async fn deregister_first_mapping() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
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
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let validator_address = config::mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
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
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    // register second time
    let tx_in2 = faucet.request_tokens(&address_bech32, 10_000_000).await;

    let midnight_wallet_seed2 = MidnightClient::new_seed();
    let dust_hex2 = MidnightClient::new_dust_hex(midnight_wallet_seed2.clone());
    let register_tx_id2 = cardano_client
        .register(&dust_hex2, &tx_in2, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id2)
    );

    let register_tx2 = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id2))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx2);

    let amount2 = 100;
    let tx_id2 = cardano_client
        .mint_tokens(amount2, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount2, hex::encode(tx_id2));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce2);

    //check utxo2 NOT producing dust
    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed2,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
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
    tracing::info!(
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
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount3, hex::encode(tx_id3));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce3);

    //check utxo3 producing dust
    let args3 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result3 = dust_balance::execute(args3)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result3 {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    let args4 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result4 = dust_balance::execute(args4)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result4 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result4, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > *balance)
    );
}

#[e2e_test]
async fn produce_dust_from_tokens_owned_before_registration() {
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 6_000_000).await;
    // for minting cNIGHT tokens
    faucet.request_tokens(&address_bech32, 7_000_000).await;

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    let _dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    let amount = 100;
    let tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    tracing::info!(
        "Registration transaction submitted with hash: {}",
        hex::encode(register_tx_id)
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));

    let cnight_utxo_new = cardano_client
        .rotate_cnight(&cnight_utxo)
        .await
        .expect("Failed to rotate cNight UTxO");
    tracing::info!(
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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce_new);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce_new, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
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
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));
}

#[e2e_test]
async fn stop_dust_producing_after_deregistration_and_rotation() {
    // case for stop dust production (reg -> mint -> dereg -> rotate)
    let settings = Settings::default();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let address_bech32 = cardano_client.address_as_bech32();
    let base_url = settings.node_client.base_url.clone();
    let same_base_url = settings.node_client.base_url.clone();
    let midnight_client = MidnightClient::new(settings.node_client).await;
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 6_000_000).await;
    faucet.request_tokens(&address_bech32, 7_000_000).await;

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    let register_tx_id = cardano_client
        .register(&dust_hex, &tx_in, &collateral_utxo)
        .await
        .expect("Failed to register tx")
        .transaction
        .id;
    tracing::info!(
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
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
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

    let validator_address = config::mapping_validator_address();
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

    let deregister_tx = cardano_client
        .deregister(utxo, &register_tx, &collateral_utxo)
        .await
        .expect("Failed to deregister")
        .transaction
        .id;
    tracing::info!(
        "Deregistration transaction submitted with hash: {}",
        hex::encode(deregister_tx)
    );

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(same_base_url),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    let mut balance_before_rotation: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance before rotation: {}", total);
        balance_before_rotation = total;
    }

    let cnight_utxo_new = cardano_client
        .rotate_cnight(&cnight_utxo)
        .await
        .expect("Failed to rotate cNight UTxO");
    tracing::info!(
        "Rotated cNIGHT UTXO: {}",
        &hex::encode(&cnight_utxo_new.transaction.id)
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(base_url),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let _spend_cnight_event = midnight_client
        .subscribe_to_cnight_observation_events(&cnight_utxo_new.transaction.id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance_after_rotation: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance after rotation: {}", total);
        balance_after_rotation = total;
    }

    assert!(
        balance_after_rotation < balance_before_rotation,
        "balance_after_rotation ({}) must be less than balance_before_rotation ({})",
        balance_after_rotation,
        balance_before_rotation
    );
}

#[e2e_test]
async fn spend_cnight_producing_dust() {
    let settings = Settings::default();
    let cardano_client =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let bech32_address = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", bech32_address);

    let bob_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let bob_bech32 = bob_client.address_as_bech32();
    tracing::info!("Bob's Cardano wallet created: {:?}", bob_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        bech32_address,
        dust_hex
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
    tracing::info!(
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
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(tx_id));

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
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    let utxo_owner = midnight_client
        .poll_utxo_owners_until_change(nonce, None, 60, 1000)
        .await
        .expect("Failed to poll UTXO owners");
    tracing::info!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

    let utxo_owner_hex = hex::encode(utxo_owner.unwrap().0.0);
    tracing::info!("UTXO owner in hex: {:?}", utxo_owner_hex);
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
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed.clone(),
        dry_run: false,
    };

    let result = dust_balance::execute(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    // sleep 10s
    tracing::info!("Sleeping 10 seconds before spending cNIGHT...");
    tokio::time::sleep(Duration::from_secs(10)).await;
    let cnight_spent_utxo = cardano_client.spend_cnight(&cnight_utxo, &bob_bech32).await;

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: 1,
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: FetchCacheConfig::InMemory,
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: "".to_string(),
        },
        seed: midnight_wallet_seed,
        dry_run: false,
    };

    let _spend_cnight_event = midnight_client
        .subscribe_to_cnight_observation_events(&cnight_spent_utxo.unwrap().transaction.id)
        .await
        .expect("Failed to listen to cNgD registration event");

    let result2 = dust_balance::execute(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total < *balance)
    );
}

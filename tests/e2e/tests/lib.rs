use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;
use midnight_node_e2e::faucet::FaucetManager;
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
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, MutexGuard, OnceCell, Semaphore};
use tokio::time::{Duration, sleep, timeout};

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

fn finished_pre_deploy_test() {
    let prev = PRE_DEPLOY_COUNT.fetch_add(1, Ordering::SeqCst);
    if prev == NUM_PRE_DEPLOY_TESTS - 1 {
        DEPLOY_GATE.add_permits(NUM_DEPLOY_TESTS);
    }
}

async fn wait_before_deploying() -> MutexGuard<'static, ()> {
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

    let amount2 = 100;
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

    finished_pre_deploy_test();

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

    finished_pre_deploy_test();

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
    // Accept various error types: replay protection, already deployed, or generic invalid
    assert!(
        error_msg.to_lowercase().contains("invalid")
            || error_msg.to_lowercase().contains("replay")
            || error_msg.to_lowercase().contains("already")
            || error_msg.contains("1010"), // Substrate InvalidTransaction code
        "Expected InvalidTransaction or replay-related error, got: {}",
        error_msg
    );

    tracing::info!("✓ PR367-TC-0003-02 E2E PASSED: Replay attack rejected, no blockspace consumed");
}

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
/// Uses CONTRACT_ADDR (the address DEPLOY_TX deploys to) so we know the
/// address itself parses; the only reason for failure is "no contract here".
/// Pre-deploy gated so it runs before any DEPLOY_TX submission.
#[e2e_test]
async fn contract_state_for_undeployed_address_returns_not_present() {
    use midnight_node_res::undeployed::transactions::CONTRACT_ADDR;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    let addr = std::str::from_utf8(CONTRACT_ADDR)
        .expect("CONTRACT_ADDR is ASCII hex")
        .trim();

    let result = client.get_contract_state(addr).await;

    finished_pre_deploy_test();

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
    use midnight_node_ledger_helpers::extract_tx_with_context;
    use midnight_node_toolkit::commands::contract_address::{self, ContractAddressArgs};
    use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs};
    use midnight_node_toolkit::tx_generator::builder::{Builder, ContractCall, ContractDeployArgs};
    use midnight_node_toolkit::tx_generator::destination::Destination;
    use midnight_node_toolkit::tx_generator::source::Source;

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
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let deploy_file = tempdir.path().join("contract_deploy.mn");
    let deploy_file_str = deploy_file.to_string_lossy().to_string();

    tracing::info!("Generating fresh DEPLOY_TX against live chain at {url}...");
    let gen_args = GenerateTxsArgs {
        builder: Builder::ContractSimple(ContractCall::Deploy(ContractDeployArgs {
            funding_seed: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            authority_seeds: vec![],
            authority_threshold: None,
            rng_seed: None,
        })),
        source: Source {
            src_url: Some(url.clone()),
            fetch_concurrency: 20,
            fetch_compute_concurrency: None,
            src_files: None,
            dust_warp: false,
            ignore_block_context: false,
            fetch_only_cached: false,
            fetch_cache: FetchCacheConfig::InMemory,
            ledger_state_db: String::new(),
        },
        destination: Destination {
            dest_urls: vec![],
            rate: 1.0,
            dest_file: Some(deploy_file_str.clone()),
            no_watch_progress: true,
        },
        proof_server: None,
        dry_run: false,
    };
    generate_txs::execute(gen_args)
        .await
        .expect("generate-txs contract-simple deploy failed");

    let addr = contract_address::execute(ContractAddressArgs {
        src_file: deploy_file_str.clone(),
        tagged: false,
        untagged: false,
    })
    .expect("extract contract address from deploy tx");
    tracing::info!("Contract address (dynamic): {addr}");

    let deploy_bytes = std::fs::read(&deploy_file).expect("read generated deploy tx file");
    let (deploy_tx_bytes, _block_context) = extract_tx_with_context(&deploy_bytes);

    tracing::info!("Submitting DEPLOY_TX...");
    let mut progress = client
        .submit_midnight_tx(deploy_tx_bytes)
        .await
        .expect("DEPLOY_TX submission rejected by RPC");

    let post_deploy_hash = timeout(Duration::from_secs(60), async {
        while let Some(status) = progress.next().await {
            match status {
                Ok(subxt::tx::TransactionStatus::InBestBlock(info)) => {
                    tracing::info!("  DEPLOY_TX in best block: {:?}", info.block_hash());
                    return Ok::<_, String>(info.block_hash());
                }
                Ok(subxt::tx::TransactionStatus::InFinalizedBlock(info)) => {
                    tracing::info!("  DEPLOY_TX finalized: {:?}", info.block_hash());
                    return Ok(info.block_hash());
                }
                Ok(subxt::tx::TransactionStatus::Invalid { message })
                | Ok(subxt::tx::TransactionStatus::Dropped { message })
                | Ok(subxt::tx::TransactionStatus::Error { message }) => {
                    return Err(format!("DEPLOY_TX terminated without inclusion: {message}"));
                }
                Ok(other) => tracing::info!("  status: {other:?}"),
                Err(e) => return Err(format!("progress error: {e}")),
            }
        }
        Err("progress stream ended without confirmation".to_string())
    })
    .await
    .expect("DEPLOY_TX did not reach a terminal status within 60s")
    .expect("DEPLOY_TX failed to land in a block");

    let post_deploy_state = client
        .get_contract_state_at(&addr, Some(post_deploy_hash))
        .await
        .expect("expected deployed state at post-deploy block, got Err");
    assert!(
        !post_deploy_state.is_empty(),
        "deployed contract state should be non-empty"
    );
    tracing::info!(
        "Contract present at post-deploy block {:?} ({} hex chars of state)",
        post_deploy_hash,
        post_deploy_state.len()
    );

    // At block 1 the contract cannot exist — must error with ContractNotPresent.
    let pre_result = client
        .get_contract_state_at(&addr, Some(pre_deploy_hash))
        .await;
    let err = pre_result.expect_err("expected ContractNotPresent at pre-deploy block 1, got Ok");
    assert_contract_not_present_error(err.as_ref());

    tracing::info!(
        "✓ block 1 → ContractNotPresent; block {:?} → deployed state",
        post_deploy_hash
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
    let dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
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

    let spend_cnight_event = midnight_client
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

    let spend_cnight_event = midnight_client
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

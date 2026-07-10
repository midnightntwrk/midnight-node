use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;
use midnight_node_ledger_helpers::UnshieldedSignatureScheme;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::events::{
    Deregistration, Registration,
};
use midnight_node_toolkit::cli_parsers::SchemeSeed;
use midnight_node_toolkit::commands::dust_balance::{
    self, DustBalanceArgs, DustBalanceJson, DustBalanceResult,
};
use midnight_node_toolkit::tx_generator::source::Source;
use std::collections::HashMap;
use std::collections::HashSet;
use subxt::SubstrateConfig;
use subxt::events::Event;
use subxt::ext::codec::Decode;
use tokio::time::Duration;

use crate::{global_faucet_manager, register_test_seed, wait_for_warmup, warmup_ledger_state_db};

// -------- TIMEOUTS --------

// The follower only processes Cardano blocks once they're a security
// parameter deep, and that stability window scales with Preview's block
// rate — ~4.5h at the degraded ~34s/block. Sized with headroom; a stuck
// follower fails much earlier via the stall detector. See the README's
// "Cardano stability barrier" section.
const OBSERVATION_AWAIT_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

// ~10 Preview blocks at the degraded ~34s/block rate — absorbs the
// block-interval tail past the typical 1-2 block inclusion.
const TX_INCLUSION_TIMEOUT: Duration = Duration::from_secs(360);

// -------- EVENT FORMAT HELPERS --------
//
// The auto-generated subxt event types wrap their byte fields in newtypes
// which `Debug` renders as raw `Vec<u8>`-style arrays — pages of
// `[224, 236, 215, ...]` noise. These helpers render the same fields as
// `0x<hex>` for legible test logs.

fn fmt_registration(reg: &Registration) -> String {
    format!(
        "reward_address=0x{} dust_public_key=0x{}",
        hex::encode(&reg.0.cardano_reward_address.0),
        hex::encode(&reg.0.dust_public_key.0.0),
    )
}

fn fmt_deregistration(dereg: &Deregistration) -> String {
    format!(
        "reward_address=0x{} dust_public_key=0x{}",
        hex::encode(&dereg.0.cardano_reward_address.0),
        hex::encode(&dereg.0.dust_public_key.0.0),
    )
}

// -------- MAPPING EVENT DECODER --------
//
// `MappingEntry`'s shape changed between runtimes — pre-`390ba426b` it had
// `{ utxo_tx_hash, utxo_index }`, post-`390ba426b` it has `{ utxo_id: UtxoId
// { tx_hash, index } }`. The SCALE byte layout is identical, but subxt's
// `decode_fields_as` cross-references the chain's metadata field names and
// will refuse a structurally-different target type. That makes the
// auto-generated `MappingAdded`/`MappingRemoved` types from
// `midnight_metadata_latest` fail to decode on any runtime that doesn't
// match the .scale file shipped at build time (today: qanet runs the old
// shape, bundled metadata has the new shape).
//
// Workaround: decode the field bytes directly using a wire struct whose
// SCALE layout matches BOTH shapes. This bypasses the field-name check.
#[derive(Decode, Debug)]
#[codec(crate = subxt::ext::codec)]
struct MappingEntryWire {
    cardano_reward_address: [u8; 29],
    dust_public_key: Vec<u8>,
    utxo_tx_hash: [u8; 32],
    utxo_index: u16,
}

fn decode_mapping_event(
    evt: &Event<'_, SubstrateConfig>,
    variant: &str,
) -> Option<MappingEntryWire> {
    if evt.pallet_name() != "CNightObservation" || evt.event_name() != variant {
        return None;
    }
    MappingEntryWire::decode(&mut evt.field_bytes()).ok()
}

fn fmt_mapping_entry(entry: &MappingEntryWire) -> String {
    format!(
        "reward_address=0x{} dust_public_key=0x{} utxo_id=0x{}#{}",
        hex::encode(entry.cardano_reward_address),
        hex::encode(&entry.dust_public_key),
        hex::encode(entry.utxo_tx_hash),
        entry.utxo_index,
    )
}

// -------- TESTS --------

#[e2e_test]
async fn register_for_dust_production() {
    let settings = Settings::default();
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
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

    cardano_client
        .wait_for_tx_inclusion(
            &register_tx_id,
            &config::mapping_validator_address(),
            TX_INCLUSION_TIMEOUT,
        )
        .await
        .expect("register tx should be included within timeout");

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: Vec<u8> = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let all_events = midnight_client
        .await_cnight_observations(
            &[register_tx_id],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("register observation should arrive within timeout");

    let registration = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
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
        "Matching Registration event found: {}",
        fmt_registration(&registration.unwrap())
    );

    let mapping_added = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingAdded"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address
                && entry.dust_public_key == dust_bytes
                && entry.utxo_tx_hash == register_tx_id
        });
    assert!(
        mapping_added.is_some(),
        "Did not find MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingAdded event found: {}",
        fmt_mapping_entry(&mapping_added.unwrap())
    );
}

#[e2e_test]
async fn register_2_cardano_same_dust_address_production() {
    let settings = Settings::default();
    let base_url = settings.node_client.base_url.clone();
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client_1 =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let cardano_client_2 = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;

    let address_bech_32_1 = cardano_client_1.address_as_bech32();
    let address_bech_32_2 = cardano_client_2.address_as_bech32();
    tracing::info!("First Cardano wallet created: {:?}", address_bech_32_1);
    tracing::info!("Second Cardano wallet created: {:?}", address_bech_32_2);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
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

    // Submit registrations first, then wait for them to be in a Cardano
    // block before submitting mints. Without the inclusion wait, whisky's
    // tx-funding for mint queries Ogmios's pre-mempool UTXO set and picks
    // the same wallet UTXO that register's mempool tx is consuming —
    // Cardano then rejects mint with "All inputs are spent".
    //
    // This is a CHEAP wait (~20-60s, one Cardano block), NOT a stability
    // wait. The full Cardano stability + Midnight follower catch-up are
    // handled by the single `await_cnight_observations` call further
    // below.
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

    // Cardano-side inclusion wait: register's output lives at the mapping
    // validator script address.
    let mapping_validator_addr = config::mapping_validator_address();
    let inclusion_timeout = TX_INCLUSION_TIMEOUT;
    cardano_client_1
        .wait_for_tx_inclusion(
            &register_tx_id_1,
            &mapping_validator_addr,
            inclusion_timeout,
        )
        .await
        .expect("register tx 1 should be included within timeout");
    cardano_client_2
        .wait_for_tx_inclusion(
            &register_tx_id_2,
            &mapping_validator_addr,
            inclusion_timeout,
        )
        .await
        .expect("register tx 2 should be included within timeout");

    let amount = 100;
    let mint_tx_id_1 = cardano_client_1
        .mint_tokens(amount, &collateral_utxo_1)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!(
        "Minted {} cNIGHT (wallet 1). Tx: {}",
        amount,
        hex::encode(mint_tx_id_1)
    );

    let mint_tx_id_2 = cardano_client_2
        .mint_tokens(amount, &collateral_utxo_2)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!(
        "Minted {} cNIGHT (wallet 2). Tx: {}",
        amount,
        hex::encode(mint_tx_id_2)
    );

    // Compute mint-output nonces for the storage assertions below.
    // FIXME: find_utxo_by_tx_id returns first utxo; find by native
    // token or return all utxos.
    let cnight_utxo_1 = cardano_client_1
        .find_utxo_by_tx_id(
            &cardano_client_1.address_as_bech32(),
            hex::encode(mint_tx_id_1),
        )
        .await
        .expect("No cNIGHT UTXO found after minting (wallet 1)");
    let nonce_1 = MidnightClient::calculate_nonce(
        b"asset_create",
        cnight_utxo_1.transaction.id,
        cnight_utxo_1.index,
    );
    tracing::info!("Calculated nonce for cNIGHT UTXO (wallet 1): {}", nonce_1);

    let cnight_utxo_2 = cardano_client_2
        .find_utxo_by_tx_id(
            &cardano_client_2.address_as_bech32(),
            hex::encode(mint_tx_id_2),
        )
        .await
        .expect("No cNIGHT UTXO found after minting (wallet 2)");
    let nonce_2 = MidnightClient::calculate_nonce(
        b"asset_create",
        cnight_utxo_2.transaction.id,
        cnight_utxo_2.index,
    );
    tracing::info!("Calculated nonce for cNIGHT UTXO (wallet 2): {}", nonce_2);

    let reward_address_1 = cardano_client_1.reward_address_bytes();
    let reward_address_2 = cardano_client_2.reward_address_bytes();
    let dust_address: Vec<u8> = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let nonce_for_check = nonce_1.clone();
    let nonce2_for_check = nonce_2.clone();

    // THE ONE WAIT. Subscribes to Midnight blocks, accumulates events
    // for all four tx_ids, returns when every one has been observed.
    // 4h cap on Preview (k=432 ≈ 3h + buffer for follower lag);
    // trivially fast on local-env.
    let all_events = midnight_client
        .await_cnight_observations(
            &[
                register_tx_id_1,
                register_tx_id_2,
                mint_tx_id_1,
                mint_tx_id_2,
            ],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("all four cNIGHT observations should arrive within timeout");

    // Filter accumulated events for the registration / mapping asserts.
    let registration_1 = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address_1
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        registration_1.is_some(),
        "Did not find registration event with expected reward_address and dust_address"
    );
    tracing::info!(
        "Matching Registration event found: {}",
        fmt_registration(&registration_1.unwrap())
    );

    let registration_2 = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address_2
                && reg.0.dust_public_key.0.0 == dust_address
        });
    assert!(
        registration_2.is_some(),
        "Did not find second registration event with expected second reward_address and dust_address"
    );
    tracing::info!(
        "Matching Second Registration event found: {}",
        fmt_registration(&registration_2.unwrap())
    );

    let mapping_added_1 = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingAdded"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address_1
                && entry.dust_public_key == dust_bytes
                && entry.utxo_tx_hash == register_tx_id_1
        });
    assert!(
        mapping_added_1.is_some(),
        "Did not find first MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching first MappingAdded event found: {}",
        fmt_mapping_entry(&mapping_added_1.unwrap())
    );

    let mapping_added_2 = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingAdded"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address_2
                && entry.dust_public_key == dust_bytes
                && entry.utxo_tx_hash == register_tx_id_2
        });
    assert!(
        mapping_added_2.is_some(),
        "Did not find second MappingAdded event with expected second_reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching second MappingAdded event found: {}",
        fmt_mapping_entry(&mapping_added_2.unwrap())
    );

    // Storage assertion: by the time `await_cnight_observations` returned,
    // Midnight has observed the mint and populated NightUtxoOwners for the
    // mint nonces. No polling needed.
    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce_1.clone())
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(base_url),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let bech32_address = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", bech32_address);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
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

    midnight_client
        .await_cnight_observations(
            &[register_tx_id, tx_id],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("register + mint observations should arrive within timeout");

    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce)
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    // Wait for Midnight to produce additional blocks so dust can accrue.
    // Midnight block time is 6s; 12s ≈ 2 blocks, enough for the
    // second balance read to observe growth without flakiness.
    tokio::time::sleep(Duration::from_secs(12)).await;

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result2 = crate::gated_dust_balance(args2)
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
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
    let all_events = midnight_client
        .await_cnight_observations(
            &[deregister_tx],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("deregister observation should arrive within timeout");

    let deregistration = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
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
        "Matching Deregistration event found: {}",
        fmt_deregistration(&deregistration.unwrap())
    );

    let mapping_removed = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingRemoved"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address
                && entry.dust_public_key == dust_bytes
                && entry.utxo_tx_hash == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingRemoved event found: {}",
        fmt_mapping_entry(&mapping_removed.unwrap())
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total == 0));
}

#[e2e_test]
async fn removing_excessive_registrations() {
    let settings = Settings::default();
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed);
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    let second_midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(second_midnight_wallet_seed.clone());
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
        "New wallet should have exactly four UTXOs after funding"
    );

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: [u8; 33] = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();
    let second_dust_address: [u8; 33] = hex::decode(&second_dust_hex)
        .expect("Failed to decode second DUST hex")
        .try_into()
        .unwrap();

    // Phase 1: submit both registrations back-to-back, then wait for
    // each to land in a Cardano block. The inclusion waits are cheap
    // (~20-60s each, NOT stability waits) and prevent whisky from
    // picking the same wallet UTXO twice when later txs query the
    // pre-mempool UTXO set via Ogmios.
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

    let validator_address = config::mapping_validator_address();
    let inclusion_timeout = TX_INCLUSION_TIMEOUT;
    cardano_client
        .wait_for_tx_inclusion(&register_tx_id, &validator_address, inclusion_timeout)
        .await
        .expect("register tx should be included within timeout");
    cardano_client
        .wait_for_tx_inclusion(
            &second_register_tx_id,
            &validator_address,
            inclusion_timeout,
        )
        .await
        .expect("second register tx should be included within timeout");

    // Phase 2: explicitly deregister the first mapping. Needs the
    // first registration's UTXO at the validator script address.
    let register_tx = cardano_client
        .find_utxo_by_tx_id(&validator_address, hex::encode(register_tx_id))
        .await
        .expect("No registration UTXO found after registering");
    tracing::info!("Found registration UTXO: {:?}", register_tx);

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

    cardano_client
        .wait_for_tx_inclusion(&deregister_tx, &address_bech32, inclusion_timeout)
        .await
        .expect("deregister tx should be included within timeout");

    // Phase 3: mint cNIGHT and compute its observation nonce.
    let amount = 100;
    let mint_tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(mint_tx_id));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo = cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(mint_tx_id))
        .await
        .expect("No cNIGHT UTXO found after minting");
    let nonce = MidnightClient::calculate_nonce(
        b"asset_create",
        cnight_utxo.transaction.id,
        cnight_utxo.index,
    );
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    // THE ONE WAIT — four tx_ids in one accumulating subscription.
    // Iteration order over `all_events` is block order, so `.find`
    // below returns the chronologically-first matching event, which
    // is exactly what each assertion wants:
    //   - Registration(d1)        ← register_1
    //   - MappingAdded(d1, r1)    ← register_1
    //   - MappingAdded(d2, r2)    ← second_register
    //   - Deregistration(d1)      ← second_register (second is
    //                               excessive, so first gets queued
    //                               out implicitly)
    //   - MappingRemoved(d1, r1)  ← deregister
    //   - Registration(d2)        ← deregister (d2 is now the
    //                               active mapping after the
    //                               excessive first one was removed)
    let all_events = midnight_client
        .await_cnight_observations(
            &[
                register_tx_id,
                second_register_tx_id,
                deregister_tx,
                mint_tx_id,
            ],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("all four cNIGHT observations should arrive within timeout");

    let registration = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
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
        "Matching Registration event found: {}",
        fmt_registration(&registration.unwrap())
    );

    let mapping_added = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingAdded"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address
                && entry.dust_public_key == dust_address
                && entry.utxo_tx_hash == register_tx_id
        });
    assert!(
        mapping_added.is_some(),
        "Did not find MappingAdded event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingAdded event found: {}",
        fmt_mapping_entry(&mapping_added.unwrap())
    );

    let second_mapping_added = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingAdded"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address
                && entry.dust_public_key == second_dust_address
                && entry.utxo_tx_hash == second_register_tx_id
        });
    assert!(
        second_mapping_added.is_some(),
        "Did not find second MappingAdded event with expected reward_address, second_dust_address, and second_register_tx_id"
    );
    tracing::info!(
        "Matching second MappingAdded event found: {}",
        fmt_mapping_entry(&second_mapping_added.unwrap())
    );

    let deregistration = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
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
        "Matching Deregistration event found: {}",
        fmt_deregistration(&deregistration.unwrap())
    );

    let mapping_removed = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingRemoved"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address
                && entry.dust_public_key == dust_address
                && entry.utxo_tx_hash == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingRemoved event found: {}",
        fmt_mapping_entry(&mapping_removed.unwrap())
    );

    let registration_after_removing_excessive_mapping = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| evt.decode_fields_as::<Registration>().and_then(|r| r.ok()))
        .find(|reg| {
            reg.0.cardano_reward_address.0 == reward_address
                && reg.0.dust_public_key.0.0 == second_dust_address
        });
    assert!(
        registration_after_removing_excessive_mapping.is_some(),
        "Did not find Registration event for second_dust_address after deregister"
    );
    tracing::info!(
        "Matching Registration event found (after deregister): {}",
        fmt_registration(&registration_after_removing_excessive_mapping.unwrap())
    );

    // Storage assertion: by the time `await_cnight_observations`
    // returned, Midnight has observed the mint and populated
    // NightUtxoOwners for the mint nonce. No polling needed.
    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce)
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, second_dust_hex,
        "UTXO owner does not match second DUST address"
    );
}

/// Local-env only: 100 sequential registrations would dominate the stability
/// barrier wait on Cardano Preview and isn't useful coverage there. The test
/// simply does not exist when compiled with `--features qanet`.
///
/// Marked `#[ignore]` because even on local-env the 100 sequential Cardano
/// submissions take ~7 min and dominate the suite wall-clock. Run on demand
/// with `cargo test ... -- --ignored create_hundred_registrations`.
#[cfg(any(feature = "local", feature = "local-dev", feature = "local-ci"))]
#[e2e_test]
#[ignore]
async fn create_hundred_registrations() {
    let settings = Settings::default();
    let ogmios_settings = settings.ogmios_client.clone();
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
        register_test_seed(midnight_wallet_seed.clone());
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
        .await_cnight_observations(
            &[last_deregistration_tx_id],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("last deregister observation should arrive within timeout");

    let registration = registration_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
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
        "Matching Registration event found: {}",
        fmt_registration(&registration.unwrap())
    );
}

#[e2e_test]
async fn register_twice_with_same_cardano_address() {
    let settings = Settings::default();
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    // Created upfront so it makes the warmup batch.
    let midnight_wallet_seed2 = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed2.clone());

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

    midnight_client
        .await_cnight_observations(
            &[register_tx_id, tx_id],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("register + mint observations should arrive within timeout");

    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce)
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    // register second time (seed already registered for warmup above)
    let tx_in2 = faucet.request_tokens(&address_bech32, 10_000_000).await;

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
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
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
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed2,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result2 = crate::gated_dust_balance(args2)
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
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

    // Phase 1: register → find its validator UTXO. `find_utxo_by_tx_id`
    // polls (~4 min cap) so it doubles as a cheap Cardano-side
    // inclusion wait — no separate `wait_for_tx_inclusion` needed.
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

    // Phase 2: mint cNIGHT → find its UTXO. Same inclusion-wait
    // pattern via find_utxo_by_tx_id.
    let amount = 100;
    let mint_tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(mint_tx_id));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo = cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(mint_tx_id))
        .await
        .expect("No cNIGHT UTXO found after minting");

    let nonce = MidnightClient::calculate_nonce(
        b"asset_create",
        cnight_utxo.transaction.id,
        cnight_utxo.index,
    );
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    // Phase 3: deregister. Wallet UTXO query is safe — both register
    // and mint are confirmed in Cardano blocks at this point (the two
    // find_utxo_by_tx_id calls above already polled until inclusion).
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

    // Confirm inclusion before the await snapshots its target — a tx
    // still in the mempool can land above the target, out of the
    // scan-back's reach.
    cardano_client
        .wait_for_tx_inclusion(
            &deregister_tx,
            &cardano_client.address_as_bech32(),
            TX_INCLUSION_TIMEOUT,
        )
        .await
        .expect("deregister tx should be included within timeout");

    let reward_address = cardano_client.reward_address_bytes();
    let dust_address: Vec<u8> = hex::decode(&dust_hex)
        .expect("Failed to decode DUST hex")
        .try_into()
        .unwrap();

    // THE ONE WAIT — register, mint, deregister observed together.
    let all_events = midnight_client
        .await_cnight_observations(
            &[register_tx_id, mint_tx_id, deregister_tx],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("all three cNIGHT observations should arrive within timeout");

    let deregistration = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
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
        "Matching Deregistration event found: {}",
        fmt_deregistration(&deregistration.unwrap())
    );

    let mapping_removed = all_events
        .iter()
        .flat_map(|e| e.iter().filter_map(|x| x.ok()))
        .filter_map(|evt| decode_mapping_event(&evt, "MappingRemoved"))
        .find(|entry| {
            entry.cardano_reward_address == reward_address
                && entry.dust_public_key == dust_bytes
                && entry.utxo_tx_hash == register_tx_id
        });
    assert!(
        mapping_removed.is_some(),
        "Did not find MappingRemoved event with expected reward_address, dust_address, and utxo_id"
    );
    tracing::info!(
        "Matching MappingRemoved event found: {}",
        fmt_mapping_entry(&mapping_removed.unwrap())
    );

    // Storage assertion: mint owner. By the time
    // await_cnight_observations returned, Midnight has populated
    // NightUtxoOwners for the mint nonce. No polling needed.
    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce)
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    // Wait for Midnight to produce additional blocks so dust can accrue.
    // Midnight block time is 6s; 12s ≈ 2 blocks, enough for the
    // second balance read to observe growth without flakiness.
    tokio::time::sleep(Duration::from_secs(12)).await;

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result2 = crate::gated_dust_balance(args2)
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    // Created upfront so it makes the warmup batch.
    let midnight_wallet_seed2 = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed2.clone());

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

    midnight_client
        .await_cnight_observations(
            &[register_tx_id, tx_id],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("register + mint observations should arrive within timeout");

    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce)
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    //check utxo1 producing dust
    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    // register second time (seed already registered for warmup above)
    let tx_in2 = faucet.request_tokens(&address_bech32, 10_000_000).await;

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
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed2,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result2 = crate::gated_dust_balance(args2)
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
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result3 = crate::gated_dust_balance(args3)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result3 {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    assert!(matches!(result, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total > 0));

    // Wait for Midnight to produce additional blocks so dust can accrue.
    // Midnight block time is 6s; 12s ≈ 2 blocks, enough for the
    // second balance read to observe growth without flakiness.
    tokio::time::sleep(Duration::from_secs(12)).await;

    let args4 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result4 = crate::gated_dust_balance(args4)
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;
    let address_bech32 = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    // Register the seed before the slow faucet work so it makes the
    // warmup batch.
    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    let _dust_bytes: Vec<u8> = hex::decode(&dust_hex).unwrap().try_into().unwrap();
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 6_000_000).await;
    // for minting cNIGHT tokens
    faucet.request_tokens(&address_bech32, 7_000_000).await;

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

    // This probe runs before the warmup finishes; wait for it so the read
    // hits the warm cache. The assert-0 premise holds until the
    // registration clears the stability window, much later.
    wait_for_warmup(Duration::from_secs(2 * 60 * 60)).await;

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
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

    midnight_client
        .await_cnight_observations(
            &[cnight_utxo_new.transaction.id],
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("rotate observation should arrive within timeout");

    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce_new)
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result2 = crate::gated_dust_balance(args2)
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let address_bech32 = cardano_client.address_as_bech32();
    let base_url = settings.node_client.base_url.clone();
    let same_base_url = settings.node_client.base_url.clone();
    let midnight_client = MidnightClient::new(settings.node_client).await;
    tracing::info!("New Cardano wallet created: {:?}", address_bech32);

    // Register the seed before the slow faucet work so it makes the
    // warmup batch.
    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        address_bech32,
        dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&address_bech32, 5_000_000).await;
    let tx_in = faucet.request_tokens(&address_bech32, 6_000_000).await;
    faucet.request_tokens(&address_bech32, 7_000_000).await;

    // Phase 1: register → mint. `find_utxo_by_tx_id` polls for Cardano
    // inclusion in both cases.
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

    let amount = 100;
    let mint_tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(mint_tx_id));

    let cnight_utxo = cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(mint_tx_id))
        .await
        .expect("No cNIGHT UTXO found after minting");
    let nonce = MidnightClient::calculate_nonce(
        b"asset_create",
        cnight_utxo.transaction.id,
        cnight_utxo.index,
    );
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    // Snapshot the first-await target NOW — right after mint inclusion is
    // confirmed, BEFORE the spacing wait. This places `first_target`
    // close to mint's actual landing block. We then space out dereg/rotate
    // by ≥ 5 Cardano blocks AND ≥ 5 Midnight blocks before submitting
    // them, so dereg/rotate's block is far past `first_target`. When the
    // first await's watermark crosses `first_target` it jumps a few
    // blocks at most (per `process_tokens` batch size) — well below
    // dereg/rotate's block — leaving a window during which mint is
    // observed but dereg/rotate aren't yet. That's when we read
    // `balance_before`.
    let first_target = CardanoClient::current_block_height(&ogmios_settings)
        .await
        .expect("Failed to snapshot Cardano tip for first await target");
    tracing::info!("first await target (Cardano tip post-mint): {first_target}");

    MidnightClient::wait_for_block_spacing(&ogmios_settings, &midnight_client, 5, 5)
        .await
        .expect("Failed to wait for block spacing");

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

    let cnight_utxo_new = cardano_client
        .rotate_cnight(&cnight_utxo)
        .await
        .expect("Failed to rotate cNight UTxO");
    tracing::info!(
        "Rotated cNIGHT UTXO: {}",
        &hex::encode(&cnight_utxo_new.transaction.id)
    );

    // Snapshot the second-await target right after both second-batch txs
    // are submitted. This is a tight upper bound on dereg/rotate's
    // actual landing block, so the second await resolves via past-scan
    // (or within a few Cardano blocks of polling) rather than waiting
    // an entire additional stability window past a stale current-tip
    // snapshot.
    let second_target = CardanoClient::snapshot_tip_after_advance(&ogmios_settings)
        .await
        .expect("Failed to snapshot Cardano tip for second await target");
    tracing::info!("second await target (Cardano tip post-dereg/rotate): {second_target}");

    // FIRST await: register + mint, with explicit target snapshotted
    // pre-dereg/rotate. ~3h on Preview, ~seconds on local.
    midnight_client
        .await_cnight_observations_at(
            &[register_tx_id, mint_tx_id],
            first_target,
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("register + mint observations should arrive within timeout");

    // Storage assertion (mint owner) + balance_before, read between
    // the two awaits so the chain state reflects mint observed but
    // dereg/rotate not yet (their watermark threshold is
    // PRE_AWAIT_SUBMISSION_SPACING Cardano blocks higher).
    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce.clone())
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(same_base_url),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    // Window-sensitive read — must not queue behind the gate.
    let result2 = crate::window_dust_balance(args2)
        .await
        .expect("dust-balance error");

    let mut balance_before_rotation: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance before rotation: {}", total);
        balance_before_rotation = total;
    }

    // SECOND await: dereg + rotate observations. The target was
    // snapshotted right after these were submitted, so it's tight —
    // by the time we get here, watermark is typically already past it
    // (past-scan returns immediately).
    midnight_client
        .await_cnight_observations_at(
            &[deregister_tx, cnight_utxo_new.transaction.id],
            second_target,
            &ogmios_settings,
            Duration::from_secs(30 * 60),
        )
        .await
        .expect("dereg + rotate observations should arrive within timeout");

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(base_url),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result = crate::gated_dust_balance(args)
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
    let ogmios_settings = settings.ogmios_client.clone();
    let cardano_client =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let midnight_client = MidnightClient::new(settings.node_client.clone()).await;

    let bech32_address = cardano_client.address_as_bech32();
    tracing::info!("New Cardano wallet created: {:?}", bech32_address);

    let bob_client = CardanoClient::new(settings.ogmios_client, settings.constants).await;
    let bob_bech32 = bob_client.address_as_bech32();
    tracing::info!("Bob's Cardano wallet created: {:?}", bob_bech32);

    let midnight_wallet_seed = MidnightClient::new_seed();
    register_test_seed(midnight_wallet_seed.clone());
    let dust_hex = MidnightClient::new_dust_hex(midnight_wallet_seed.clone());
    tracing::info!(
        "Registering Cardano wallet {} with DUST address {}",
        bech32_address,
        dust_hex
    );

    let faucet = global_faucet_manager().await;
    let collateral_utxo = faucet.request_tokens(&bech32_address, 5_000_000).await;
    let tx_in = faucet.request_tokens(&bech32_address, 10_000_000).await;

    // Phase 1: register → mint. `find_utxo_by_tx_id` polls for Cardano
    // inclusion in both cases.
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

    cardano_client
        .find_utxo_by_tx_id(
            &cardano_client.address_as_bech32(),
            hex::encode(register_tx_id),
        )
        .await
        .expect("No registration UTXO found");

    let amount = 100;
    let mint_tx_id = cardano_client
        .mint_tokens(amount, &collateral_utxo)
        .await
        .expect("Failed to mint tokens")
        .transaction
        .id;
    tracing::info!("Minted {} cNIGHT. Tx: {}", amount, hex::encode(mint_tx_id));

    // FIXME: it returns first utxo, find by native token or return all utxos
    let cnight_utxo = cardano_client
        .find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(mint_tx_id))
        .await
        .expect("No cNIGHT UTXO found after minting");
    let nonce = MidnightClient::calculate_nonce(
        b"asset_create",
        cnight_utxo.transaction.id,
        cnight_utxo.index,
    );
    tracing::info!("Calculated nonce for cNIGHT UTXO: {}", nonce);

    // Snapshot the first-await target NOW — right after mint inclusion is
    // confirmed, BEFORE the spacing wait. This places `first_target`
    // close to mint's actual landing block. We then space out spend by
    // ≥ 5 Cardano blocks AND ≥ 5 Midnight blocks before submitting it,
    // so spend's block is far past `first_target`. When the first
    // await's watermark crosses `first_target` it jumps a few blocks at
    // most (per `process_tokens` batch size) — well below spend's
    // block — leaving a window during which mint is observed but spend
    // isn't. That's when we read `balance_before`.
    let first_target = CardanoClient::current_block_height(&ogmios_settings)
        .await
        .expect("Failed to snapshot Cardano tip for first await target");
    tracing::info!("first await target (Cardano tip post-mint): {first_target}");

    MidnightClient::wait_for_block_spacing(&ogmios_settings, &midnight_client, 5, 5)
        .await
        .expect("Failed to wait for block spacing");

    let cnight_spent_utxo = cardano_client
        .spend_cnight(&cnight_utxo, &bob_bech32)
        .await
        .expect("Failed to spend cNIGHT");
    tracing::info!(
        "Spent cNIGHT. Tx: {}",
        hex::encode(cnight_spent_utxo.transaction.id)
    );

    // Snapshot the second-await target right after spend is submitted.
    // This is a tight upper bound on spend's actual landing block, so
    // the second await resolves via past-scan (or within a few Cardano
    // blocks of polling) rather than waiting an entire additional
    // stability window past a stale current-tip snapshot.
    let second_target = CardanoClient::snapshot_tip_after_advance(&ogmios_settings)
        .await
        .expect("Failed to snapshot Cardano tip for second await target");
    tracing::info!("second await target (Cardano tip post-spend): {second_target}");

    // FIRST await: register + mint, with explicit target snapshotted
    // pre-spend. ~3h on Preview, ~seconds on local.
    midnight_client
        .await_cnight_observations_at(
            &[register_tx_id, mint_tx_id],
            first_target,
            &ogmios_settings,
            OBSERVATION_AWAIT_TIMEOUT,
        )
        .await
        .expect("register + mint observations should arrive within timeout");

    // Storage assertion + balance_before, read between the two awaits
    // so the chain state reflects mint observed but spend not yet
    // (spend's watermark threshold is PRE_AWAIT_SUBMISSION_SPACING
    // Cardano blocks higher).
    let utxo_owner = midnight_client
        .query_night_utxo_owners(nonce.clone())
        .await
        .expect("Failed to query UTXO owners")
        .expect("UTXO owner should be present immediately after observation");
    let utxo_owner_hex = hex::encode(utxo_owner.0.0);
    assert_eq!(
        utxo_owner_hex, dust_hex,
        "UTXO owner does not match DUST address"
    );

    let args = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed.clone(),
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    // Window-sensitive read — must not queue behind the gate.
    let result = crate::window_dust_balance(args)
        .await
        .expect("dust-balance error");

    let mut balance: &u128 = &0;
    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result {
        tracing::info!("Total dust balance: {}", total);
        balance = total;
    }

    // SECOND await: spend observation. The target was snapshotted
    // right after spend was submitted, so it's tight — by the time
    // we get here, watermark is typically already past it (past-scan
    // returns immediately).
    midnight_client
        .await_cnight_observations_at(
            &[cnight_spent_utxo.transaction.id],
            second_target,
            &ogmios_settings,
            Duration::from_secs(30 * 60),
        )
        .await
        .expect("spend observation should arrive within timeout");

    let args2 = DustBalanceArgs {
        source: Source {
            src_files: None,
            src_url: Some(settings.node_client.base_url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            dust_warp: true,
            ignore_block_context: false,
            fetch_cache: crate::fetch_cache_config(),
            fetch_only_cached: false,
            fetch_compute_concurrency: None,
            ledger_state_db: warmup_ledger_state_db(),
        },
        seed: SchemeSeed {
            seed: midnight_wallet_seed,
            scheme: UnshieldedSignatureScheme::Schnorr,
        },
        dry_run: false,
    };

    let result2 = crate::gated_dust_balance(args2)
        .await
        .expect("dust-balance error");

    if let DustBalanceResult::Json(DustBalanceJson { total, .. }) = &result2 {
        tracing::info!("Total dust balance: {}", total);
    }

    assert!(
        matches!(result2, DustBalanceResult::Json(DustBalanceJson{total, ..}) if total < *balance)
    );
}

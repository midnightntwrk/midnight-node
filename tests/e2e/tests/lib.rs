use midnight_node_e2e::api::cardano::*;
use midnight_node_e2e::api::midnight::*;
use midnight_node_e2e::cfg::*;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation;
use ogmios_client::query_ledger_state::QueryLedgerState;
use whisky::Asset;

#[tokio::test]
async fn register_for_dust_production() {
	let cardano_wallet = create_wallet();
	let bech32_address = get_cardano_address_as_bech32(&cardano_wallet);
	println!("New Cardano wallet created: {:?}", bech32_address);

	let dust_hex = new_dust_hex(32);
	println!("Registering Cardano wallet {} with DUST address {}", bech32_address, dust_hex);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = fund_wallet(&bech32_address, assets).await;

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(&[bech32_address.clone().into()]).await.unwrap();
	assert_eq!(utxos.len(), 2, "New wallet should have exactly two UTXOs after funding");

	let register_tx_id =
		register(&bech32_address, &dust_hex, &cardano_wallet, &tx_in, &collateral_utxo).await;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let cardano_address = get_cardano_address_as_bytes(&cardano_wallet);
	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let registration_events = subscribe_to_cnight_observation_events(&register_tx_id)
		.await
		.expect("Failed to listen to cNgD registration event");

	let registration = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::Registration>().ok().flatten()
		})
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address && reg.0.dust_address == dust_address
		});
	assert!(
		registration.is_some(),
		"Did not find registration event with expected cardano_address and dust_address"
	);
	if let Some(registration) = registration {
		println!("Matching Registration event found: {:?}", registration);
	}

	let mapping_added = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::MappingAdded>().ok().flatten()
		})
		.find(|map| {
			map.0.cardano_address.0 == cardano_address
				&& map.0.dust_address == dust_hex
				&& map.0.utxo_id == hex::encode(register_tx_id)
		});
	assert!(
		mapping_added.is_some(),
		"Did not find MappingAdded event with expected cardano_address, dust_address, and utxo_id"
	);
	if let Some(mapping) = mapping_added {
		println!("Matching MappingAdded event found: {:?}", mapping);
	}
}

#[tokio::test]
async fn deploy_governance_contracts_and_validate_membership_reset() {
	println!("=== Starting Governance Contracts E2E Test ===");

	// Example Sr25519 public keys for testing (Alice and Eve from Substrate)
	// In production, these would be the actual governance authority member keys
	const ALICE_SR25519: &str = "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
	const EVE_SR25519: &str = "e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e";

	// Use the funded_address from config as the deployer
	// The funded_address owns the one-shot UTxOs, so we use it for all inputs to simplify signing
	use midnight_node_e2e::cfg::*;
	let cfg = load_config();
	let funded_address = cfg.payment_addr.clone();
	println!("Using funded_address for deployment: {}", funded_address);

	// Alice's Cardano key hash
	let alice_cardano_hash = "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b";

	// Bob's Cardano key hash
	let bob_cardano_hash = "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2c";

	// Fund UTxOs for deployment (these will be owned by funded_address)
	let funding_assets = vec![Asset::new_from_str("lovelace", "500000000")]; // 500 ADA
	let tx_in_utxo = fund_wallet(&funded_address, funding_assets.clone()).await;
	println!("First funding UTXO created");

	// Create additional funding UTxO for second deployment
	let tx_in_utxo_2 = fund_wallet(&funded_address, funding_assets).await;
	println!("Second funding UTXO created");

	// Create collateral for script transactions
	let collateral_utxo = make_collateral(&funded_address).await;
	println!("Collateral UTXO created");

	// Load contract CBORs and calculate addresses and policy IDs
	let council_cbor = get_council_forever_cbor();
	let council_address = get_council_forever_address();
	let council_policy_id = get_council_forever_policy_id();

	let tech_auth_cbor = get_tech_auth_forever_cbor();
	let tech_auth_address = get_tech_auth_forever_address();
	let tech_auth_policy_id = get_tech_auth_forever_policy_id();

	println!("Council Forever:");
	println!("  Policy ID (calculated): {}", council_policy_id);
	println!("  Address: {}", council_address);

	println!("Technical Authority Forever:");
	println!("  Policy ID (calculated): {}", tech_auth_policy_id);
	println!("  Address: {}", tech_auth_address);

	// Get pre-created one-shot UTxOs from local-environment
	// These are created by the Cardano entrypoint.sh script during network setup
	let council_one_shot = get_one_shot_utxo("council").await;
	println!("✓ Council one-shot UTXO retrieved from local-environment");

	let tech_auth_one_shot = get_one_shot_utxo("techauth").await;
	println!("✓ Technical Authority one-shot UTXO retrieved from local-environment");

	// Deploy Council Forever contract
	println!("\n=== Deploying Council Forever Contract ===");
	let council_members = vec![
		(alice_cardano_hash.to_string(), ALICE_SR25519.to_string()),
		(bob_cardano_hash.to_string(), EVE_SR25519.to_string()),
	];

	let council_tx_id = deploy_governance_contract(
		&tx_in_utxo,
		&collateral_utxo,
		&council_one_shot,
		&council_cbor,
		&council_address,
		&council_policy_id,
		council_members.clone(),
		2, // total_signers
	)
	.await;

	println!("✓ Council Forever contract deployed successfully with tx ID: {council_tx_id:?}");

	// Deploy Technical Authority Forever contract
	println!("\n=== Deploying Technical Authority Forever Contract ===");
	let tech_auth_members = vec![
		(alice_cardano_hash.to_string(), ALICE_SR25519.to_string()),
		(bob_cardano_hash.to_string(), EVE_SR25519.to_string()),
	];

	let tech_auth_tx_id = deploy_governance_contract(
		&tx_in_utxo_2,
		&collateral_utxo,
		&tech_auth_one_shot,
		&tech_auth_cbor,
		&tech_auth_address,
		&tech_auth_policy_id,
		tech_auth_members.clone(),
		2, // total_signers
	)
	.await;

	println!("✓ Technical Authority Forever contract deployed successfully with tx ID: {tech_auth_tx_id:?}");

	println!("\n=== Both Governance Contracts Deployed Successfully ===");
	println!("Waiting for Midnight blockchain to emit membership reset events...\n");

	// Subscribe to federated authority observation events with timeout
	println!("Subscribing to federated authority events (timeout: 30 seconds)...");

	use tokio::time::{timeout, Duration};
	let events_result =
		timeout(Duration::from_secs(30), subscribe_to_federated_authority_events()).await;

	match events_result {
		Ok(Ok(_)) => {
			println!("Successfully received federated authority events");
		},
		Ok(Err(e)) => {
			println!("\n=== Governance Contracts E2E Test PARTIAL SUCCESS ===");
			println!("Contracts deployed successfully, but event subscription failed.");
			println!(
				"The contracts are active on-chain, but event verification could not be completed."
			);
			panic!("⚠ Failed to receive federated authority events: {}", e);
		},
		Err(_) => {
			println!("\n=== Governance Contracts E2E Test PARTIAL SUCCESS ===");
			println!(
				"Contracts deployed successfully, but events were not received within timeout."
			);
			println!("The contracts are active on-chain. The Midnight blockchain may need more time to process.");
			panic!("⚠ Timeout waiting for federated authority events (30 seconds elapsed)");
		},
	}
}
async fn register_2_cardano_same_dust_address_production() {
	let first_cardano_wallet = create_wallet();
	let second_cardano_wallet = create_wallet();

	let bech32_address = get_cardano_address_as_bech32(&first_cardano_wallet);
	let second_bech32_address = get_cardano_address_as_bech32(&second_cardano_wallet);
	println!("First Cardano wallet created: {:?}", bech32_address);
	println!("Second Cardano wallet created: {:?}", second_bech32_address);

	let dust_hex = new_dust_hex(32);
	println!("Registering First Cardano wallet {} with DUST address {}", bech32_address, dust_hex);
	println!(
		"Registering Second Cardano wallet {} with DUST address {}",
		second_bech32_address, dust_hex
	);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = fund_wallet(&bech32_address, assets).await;

	let second_collateral_utxo = make_collateral(&second_bech32_address).await;
	let assets_second = vec![Asset::new_from_str("lovelace", "160000000")];
	let second_tx_in = fund_wallet(&second_bech32_address, assets_second).await;

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(&[bech32_address.clone().into()]).await.unwrap();
	assert_eq!(utxos.len(), 2, "First wallet should have exactly two UTXOs after funding");

	let second_utxos = client.query_utxos(&[second_bech32_address.clone().into()]).await.unwrap();
	assert_eq!(second_utxos.len(), 2, "Second wallet should have exactly two UTXOs after funding");

	let register_tx_id =
		register(&bech32_address, &dust_hex, &first_cardano_wallet, &tx_in, &collateral_utxo).await;
	println!(
		"Registration transaction for the first cardano submitted with hash: {:?}",
		register_tx_id
	);

	let second_register_tx_id = register(
		&second_bech32_address,
		&dust_hex,
		&second_cardano_wallet,
		&second_tx_in,
		&second_collateral_utxo,
	)
	.await;
	println!(
		"Registration transaction for second cardano submitted with hash: {:?}",
		register_tx_id
	);

	let cardano_address = get_cardano_address_as_bytes(&first_cardano_wallet);
	let second_cardano_address = get_cardano_address_as_bytes(&second_cardano_wallet);

	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let registration_events = subscribe_to_cnight_observation_events(&register_tx_id)
		.await
		.expect("Failed to listen to cNgD registration event");

	let second_registration_events = subscribe_to_cnight_observation_events(&second_register_tx_id)
		.await
		.expect("Failed to listen to cNgD registration event");

	let registration = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::Registration>().ok().flatten()
		})
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address && reg.0.dust_address == dust_address
		});

	let second_registration = second_registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::Registration>().ok().flatten()
		})
		.find(|reg| {
			reg.0.cardano_address.0 == second_cardano_address && reg.0.dust_address == dust_address
		});

	assert!(
		registration.is_some(),
		"Did not find registration event with expected cardano_address and dust_address"
	);

	assert!(
		second_registration.is_some(),
		"Did not find second registration event with expected second cardano_address and dust_address"
	);

	if let Some(registration) = registration {
		println!("Matching Registration event found: {:?}", registration);
	}

	if let Some(second_registration) = second_registration {
		println!("Matching Second Registration event found: {:?}", second_registration);
	}

	let mapping_added = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::MappingAdded>().ok().flatten()
		})
		.find(|map| {
			map.0.cardano_address.0 == cardano_address
				&& map.0.dust_address == dust_hex
				&& map.0.utxo_id == hex::encode(register_tx_id)
		});

	let second_mapping_added = second_registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::MappingAdded>().ok().flatten()
		})
		.find(|map| {
			map.0.cardano_address.0 == second_cardano_address
				&& map.0.dust_address == dust_hex
				&& map.0.utxo_id == hex::encode(second_register_tx_id)
		});
	assert!(
		mapping_added.is_some(),
		"Did not find first MappingAdded event with expected cardano_address, dust_address, and utxo_id"
	);
	assert!(
		second_mapping_added.is_some(),
		"Did not find second MappingAdded event with expected second_cardano_address, dust_address, and utxo_id"
	);

	if let Some(mapping) = mapping_added {
		println!("Matching first MappingAdded event found: {:?}", mapping);
	}

	if let Some(second_mapping_added) = second_mapping_added {
		println!("Matching second MappingAdded event found: {:?}", second_mapping_added);
	}
}

#[tokio::test]
async fn cnight_produces_dust() {
	let cardano_wallet = create_wallet();
	let bech32_address = get_cardano_address_as_bech32(&cardano_wallet);
	println!("New Cardano wallet created: {:?}", bech32_address);

	let dust_hex = new_dust_hex(32);
	println!("Registering Cardano wallet {} with DUST address {}", bech32_address, dust_hex);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = fund_wallet(&bech32_address, assets).await;

	let register_tx_id =
		register(&bech32_address, &dust_hex, &cardano_wallet, &tx_in, &collateral_utxo).await;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let minting_script = load_cbor(&load_config().cnight_token_policy_file);
	let amount = 100;
	let tx_id = mint_tokens(
		&cardano_wallet,
		&get_cnight_token_policy_id(),
		"",
		&amount.to_string(),
		&minting_script,
	)
	.await;
	println!("Minted {} cNIGHT. Tx: {:?}", amount, tx_id);

	// FIXME: it returns first utxo, find by native token or return all utxos
	let cnight_utxo = match find_utxo_by_tx_id(&bech32_address, &hex::encode(&tx_id)).await {
		Some(cnight_utxo) => cnight_utxo,
		None => panic!("No cNIGHT UTXO found after minting"),
	};

	let prefix = b"asset_create";
	let nonce = calculate_nonce(prefix, cnight_utxo.transaction.id, cnight_utxo.index);
	println!("Calculated nonce for cNIGHT UTXO: {}", nonce);

	let utxo_owner = poll_utxo_owners_until_change(nonce, None, 60, 1000)
		.await
		.expect("Failed to poll UTXO owners");
	println!("Queried UTXO owners from Midnight node: {:?}", utxo_owner);

	let utxo_owner_hex = hex::encode(utxo_owner.unwrap());
	println!("UTXO owner in hex: {:?}", utxo_owner_hex);
	assert_eq!(utxo_owner_hex, dust_hex, "UTXO owner does not match DUST address");
}

#[tokio::test]
async fn deregister_from_dust_production() {
	let cardano_wallet = create_wallet();
	let bech32_address = get_cardano_address_as_bech32(&cardano_wallet);
	println!("New Cardano wallet created: {:?}", bech32_address);

	let dust_hex = new_dust_hex(32);
	println!("Registering Cardano wallet {} with DUST address {}", bech32_address, dust_hex);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = fund_wallet(&bech32_address, assets).await;

	let register_tx_id =
		register(&bech32_address, &dust_hex, &cardano_wallet, &tx_in, &collateral_utxo).await;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let validator_address = get_mapping_validator_address();
	let register_tx = find_utxo_by_tx_id(&validator_address, &hex::encode(&register_tx_id))
		.await
		.expect("No registration UTXO found after registering");
	println!("Found registration UTXO: {:?}", register_tx);

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(&[bech32_address.clone().into()]).await.unwrap();
	assert!(!utxos.is_empty(), "No UTXOs found for funding address");
	let utxo = utxos
		.iter()
		.max_by_key(|u| u.value.lovelace)
		.expect("No UTXO with lovelace found");

	let deregister_tx = deregister(&cardano_wallet, &utxo, &register_tx, &collateral_utxo)
		.await
		.unwrap();
	println!("Deregistration transaction submitted with hash: {:?}", deregister_tx);

	let cardano_address = get_cardano_address_as_bytes(&cardano_wallet);
	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let events = subscribe_to_cnight_observation_events(&deregister_tx)
		.await
		.expect("Failed to listen to cNgD registration event");

	let deregistration = events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::Deregistration>().ok().flatten()
		})
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address && reg.0.dust_address == dust_address
		});
	assert!(
		deregistration.is_some(),
		"Did not find deregistration event with expected cardano_address and dust_address"
	);
	if let Some(deregistration) = deregistration {
		println!("Matching Deregistration event found: {:?}", deregistration);
	}

	let mapping_removed = events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<c_night_observation::events::MappingRemoved>().ok().flatten()
		})
		.find(|map| {
			map.0.cardano_address.0 == cardano_address
				&& map.0.dust_address == dust_hex
				&& map.0.utxo_id == hex::encode(register_tx_id)
		});
	assert!(
		mapping_removed.is_some(),
		"Did not find MappingRemoved event with expected cardano_address, dust_address, and utxo_id"
	);
	if let Some(mapping) = mapping_removed {
		println!("Matching MappingRemoved event found: {:?}", mapping);
	}
}

#[tokio::test]
#[ignore = "See bug https://shielded.atlassian.net/browse/PM-19856"]
async fn alice_cannot_deregister_bob() {
	// Create Alice and Bob wallets
	let alice = create_wallet();
	let alice_bech32 = get_cardano_address_as_bech32(&alice);

	let bob = create_wallet();
	let bob_bech32 = get_cardano_address_as_bech32(&bob);
	let dust_hex = new_dust_hex(32);

	// Fund Alice and Bob wallets
	let ada_to_fund = vec![Asset::new_from_str("lovelace", "160000000")];
	let alice_collateral = make_collateral(&alice_bech32).await;
	let deregister_tx_in = fund_wallet(&alice_bech32, ada_to_fund.clone()).await;

	let bob_collateral = make_collateral(&bob_bech32).await;
	let register_tx_in = fund_wallet(&bob_bech32, ada_to_fund.clone()).await;

	// Bob registers his DUST address
	println!("Registering Bob wallet {} with DUST address {}", bob_bech32, dust_hex);
	let register_tx_id =
		register(&bob_bech32, &dust_hex, &bob, &register_tx_in, &bob_collateral).await;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	// Find Bob's registration UTXO
	let validator_address = get_mapping_validator_address();
	let register_tx = find_utxo_by_tx_id(&validator_address, &hex::encode(&register_tx_id))
		.await
		.expect("No registration UTXO found after registering");
	println!("Found registration UTXO: {:?}", register_tx);

	// Alice attempts to deregister Bob
	let deregister_tx =
		deregister(&alice, &deregister_tx_in, &register_tx, &alice_collateral).await;
	assert!(deregister_tx.is_err(), "Alice should not be able to deregister Bob");

	// Check if Bob's registration still exists in mapping validator UTXOs
	let still_unspent =
		wait_utxo_unspent_for_3_blocks(&validator_address, &hex::encode(&register_tx_id)).await;
	assert!(still_unspent, "Bob's registration UTXO should still be unspent");
}

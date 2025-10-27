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

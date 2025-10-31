use midnight_node_e2e::api::cardano::CardanoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::configuration::get_configuration;
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::events::{
	Deregistration, MappingAdded, MappingRemoved, Registration,
};
use ogmios_client::query_ledger_state::QueryLedgerState;
use whisky::Asset;

#[tokio::test]
async fn register_for_dust_production() {
	let config = get_configuration().expect("Failed to get configuration");
	let cardano_client = CardanoClient::new(config.ogmios_client, config.test_files).await;
	let midnight_client = MidnightClient::new(config.node_client).await;
	let address_bech32 = cardano_client.address_as_bech32();

	let dust_hex = MidnightClient::new_dust_hex();
	let dust_bytes: Vec<u8> = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	println!("Registering Cardano wallet {} with DUST address {}", address_bech32, dust_hex);

	let collateral_utxo =
		cardano_client.make_collateral().await.expect("Failed to make collateral");
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = cardano_client.fund_wallet(assets).await.expect("Failed to fund wallet");

	let utxos = cardano_client
		.ogmios_clients
		.query_utxos(&[address_bech32.clone().into()])
		.await
		.expect("Failed to query utxos");
	assert_eq!(utxos.len(), 2, "New wallet should have exactly two UTXOs after funding");

	let register_tx_id = cardano_client
		.register(&dust_hex, &tx_in, &collateral_utxo)
		.await
		.expect("Failed to execute registration transaction")
		.transaction
		.id;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let cardano_address = cardano_client.address.to_bytes();
	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let registration_events = midnight_client
		.subscribe_to_cnight_observation_events(&register_tx_id)
		.await
		.expect("Failed to listen to cNgD registration event");

	let registration = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address && reg.0.dust_address == dust_address
		});
	assert!(
		registration.is_some(),
		"Did not find registration event with expected cardano_address and dust_address"
	);
	println!("Matching Registration event found: {:?}", registration.unwrap());

	let mapping_added = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
		.find(|map| {
			map.0.cardano_address.0 == cardano_address
				&& &map.0.dust_address == &dust_bytes
				&& map.0.utxo_id == register_tx_id
		});
	assert!(
		mapping_added.is_some(),
		"Did not find MappingAdded event with expected cardano_address, dust_address, and utxo_id"
	);
	println!("Matching MappingAdded event found: {:?}", mapping_added.unwrap());
}

#[tokio::test]
async fn register_2_cardano_same_dust_address_production() {
	let config = get_configuration().expect("Failed to get configuration");
	let cardano_client_1 =
		CardanoClient::new(config.clone().ogmios_client, config.clone().test_files).await;
	let cardano_client_2 = CardanoClient::new(config.ogmios_client, config.test_files).await;
	let midnight_client = MidnightClient::new(config.node_client).await;

	let address_bech32_1 = cardano_client_1.address_as_bech32();
	let address_bech32_2 = cardano_client_2.address_as_bech32();
	println!("First Cardano wallet created: {:?}", address_bech32_1);
	println!("Second Cardano wallet created: {:?}", address_bech32_2);

	let dust_hex = MidnightClient::new_dust_hex();
	let dust_bytes: Vec<u8> = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	println!(
		"Registering First Cardano wallet {} with DUST address {}",
		address_bech32_1, dust_hex
	);
	println!(
		"Registering Second Cardano wallet {} with DUST address {}",
		address_bech32_2, dust_hex
	);

	let collateral_utxo_1 = cardano_client_1
		.make_collateral()
		.await
		.expect("Failed to make collateral for first cardano wallet");
	let assets_1 = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in_1 = cardano_client_1
		.fund_wallet(assets_1)
		.await
		.expect("Failed to fund first cardano wallet");

	let collateral_utxo_2 = cardano_client_2
		.make_collateral()
		.await
		.expect("Failed to make collateral for second cardano wallet");
	let assets_2 = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in_2 = cardano_client_2
		.fund_wallet(assets_2)
		.await
		.expect("Failed to fund second cardano wallet");

	let utxos_1 = cardano_client_1
		.ogmios_clients
		.query_utxos(&[address_bech32_1.clone().into()])
		.await
		.expect("Failed to query utxos");
	assert_eq!(utxos_1.len(), 2, "First wallet should have exactly two UTXOs after funding");

	let utxos_2 = cardano_client_2
		.ogmios_clients
		.query_utxos(&[address_bech32_2.clone().into()])
		.await
		.expect("Failed to query utxos");
	assert_eq!(utxos_2.len(), 2, "Second wallet should have exactly two UTXOs after funding");

	let register_tx_id_1 = cardano_client_1
		.register(&dust_hex, &tx_in_1, &collateral_utxo_1)
		.await
		.expect("Failed to register first cardano transaction")
		.transaction
		.id;
	println!(
		"Registration transaction for the first cardano submitted with hash: {:?}",
		register_tx_id_1
	);

	let register_tx_id_2 = cardano_client_2
		.register(&dust_hex, &tx_in_2, &collateral_utxo_2)
		.await
		.expect("Failed to register second cardano transaction")
		.transaction
		.id;
	println!(
		"Registration transaction for second cardano submitted with hash: {:?}",
		register_tx_id_2
	);

	let cardano_address_bytes_1 = cardano_client_1.address.to_bytes();
	let cardano_address_bytes_2 = cardano_client_2.address.to_bytes();

	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
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
			reg.0.cardano_address.0 == cardano_address_bytes_1 && reg.0.dust_address == dust_address
		});

	let registration_2 = registration_events_2
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<Registration>().ok().flatten())
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address_bytes_2 && reg.0.dust_address == dust_address
		});

	assert!(
		registration_1.is_some(),
		"Did not find registration event with expected cardano_address and dust_address"
	);

	assert!(
		registration_2.is_some(),
		"Did not find second registration event with expected second cardano_address and dust_address"
	);

	println!("Matching Registration event found: {:?}", registration_1.unwrap());

	println!("Matching Second Registration event found: {:?}", registration_2.unwrap());

	let mapping_added_1 = registration_events_1
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
		.find(|map| {
			map.0.cardano_address.0 == cardano_address_bytes_1
				&& &map.0.dust_address == &dust_bytes
				&& map.0.utxo_id == register_tx_id_1
		});

	let mapping_added_2 = registration_events_2
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<MappingAdded>().ok().flatten())
		.find(|map| {
			map.0.cardano_address.0 == cardano_address_bytes_2
				&& &map.0.dust_address == &dust_bytes
				&& map.0.utxo_id == register_tx_id_2
		});
	assert!(
		mapping_added_1.is_some(),
		"Did not find first MappingAdded event with expected cardano_address, dust_address, and utxo_id"
	);
	assert!(
		mapping_added_2.is_some(),
		"Did not find second MappingAdded event with expected second_cardano_address, dust_address, and utxo_id"
	);

	println!("Matching first MappingAdded event found: {:?}", mapping_added_1.unwrap());
	println!("Matching second MappingAdded event found: {:?}", mapping_added_2.unwrap());
}

#[tokio::test]
async fn cnight_produces_dust() {
	let config = get_configuration().expect("Failed to get configuration");
	let cardano_client = CardanoClient::new(config.ogmios_client, config.test_files).await;
	let midnight_client = MidnightClient::new(config.node_client).await;
	let address_bech32 = cardano_client.address_as_bech32();
	println!("New Cardano wallet created: {:?}", address_bech32);

	let dust_hex = MidnightClient::new_dust_hex();
	println!("Registering Cardano wallet {} with DUST address {}", address_bech32, dust_hex);

	let collateral_utxo =
		cardano_client.make_collateral().await.expect("Failed to make collateral");
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = cardano_client.fund_wallet(assets).await.expect("Failed to fund a wallet");

	let register_tx_id = cardano_client
		.register(&dust_hex, &tx_in, &collateral_utxo)
		.await
		.expect("Failed to register a wallet")
		.transaction
		.id;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let amount = 100;
	let tx_id = cardano_client
		.mint_tokens(amount)
		.await
		.expect("Failed to mint a token")
		.transaction
		.id;
	println!("Minted {} cNIGHT. Tx: {:?}", amount, tx_id);

	// FIXME: it returns first utxo, find by native token or return all utxos
	let cnight_utxo = match cardano_client
		.find_utxo_by_tx_id(&cardano_client.address_as_bech32(), hex::encode(&tx_id))
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

	let utxo_owner_hex = hex::encode(utxo_owner.expect("Failed to get utxo owner"));
	println!("UTXO owner in hex: {:?}", utxo_owner_hex);
	assert_eq!(utxo_owner_hex, dust_hex, "UTXO owner does not match DUST address");
}

#[tokio::test]
async fn deregister_from_dust_production() {
	let config = get_configuration().expect("Failed to get configuration");
	let cardano_client = CardanoClient::new(config.ogmios_client, config.test_files).await;
	let midnight_client = MidnightClient::new(config.node_client).await;

	let address_bech32 = cardano_client.address_as_bech32();
	println!("New Cardano wallet created: {:?}", address_bech32);

	let dust_hex = MidnightClient::new_dust_hex();
	let dust_bytes: Vec<u8> = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	println!("Registering Cardano wallet {} with DUST address {}", address_bech32, dust_hex);

	let collateral_utxo =
		cardano_client.make_collateral().await.expect("Failed to make a collateral");
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = cardano_client.fund_wallet(assets).await.expect("Failed to fund a wallet");

	let register_tx_id = cardano_client
		.register(&dust_hex, &tx_in, &collateral_utxo)
		.await
		.expect("Failed to register a transaction")
		.transaction
		.id;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	let validator_address = cardano_client
		.test_files
		.policies
		.mapping_validator_address(cardano_client.network_info.network_id());
	let register_tx = cardano_client
		.find_utxo_by_tx_id(&validator_address, hex::encode(&register_tx_id))
		.await
		.expect("No registration UTXO found after registering");
	println!("Found registration UTXO: {:?}", register_tx);

	let utxos = cardano_client
		.ogmios_clients
		.query_utxos(&[address_bech32.clone().into()])
		.await
		.expect("Failed to query utxos");
	assert!(!utxos.is_empty(), "No UTXOs found for funding address");
	let utxo = utxos
		.iter()
		.max_by_key(|u| u.value.lovelace)
		.expect("No UTXO with lovelace found");

	let deregister_tx = cardano_client
		.deregister(&utxo, &register_tx, &collateral_utxo)
		.await
		.expect("Failed to deregister a transaction")
		.transaction
		.id;
	println!("Deregistration transaction submitted with hash: {:?}", deregister_tx);

	let cardano_address_bytes = cardano_client.address.to_bytes();
	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let events = midnight_client
		.subscribe_to_cnight_observation_events(&deregister_tx)
		.await
		.expect("Failed to listen to cNgD registration event");

	let deregistration = events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<Deregistration>().ok().flatten())
		.find(|reg| {
			reg.0.cardano_address.0 == cardano_address_bytes && reg.0.dust_address == dust_address
		});
	assert!(
		deregistration.is_some(),
		"Did not find deregistration event with expected cardano_address and dust_address"
	);
	println!("Matching Deregistration event found: {:?}", deregistration.unwrap());

	let mapping_removed = events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| evt.as_event::<MappingRemoved>().ok().flatten())
		.find(|map| {
			map.0.cardano_address.0 == cardano_address_bytes
				&& &map.0.dust_address == &dust_bytes
				&& map.0.utxo_id == register_tx_id
		});
	assert!(
		mapping_removed.is_some(),
		"Did not find MappingRemoved event with expected cardano_address, dust_address, and utxo_id"
	);
	println!("Matching MappingRemoved event found: {:?}", mapping_removed.unwrap());
}

#[tokio::test]
#[ignore = "See bug https://shielded.atlassian.net/browse/PM-19856"]
async fn alice_cannot_deregister_bob() {
	let config = get_configuration().expect("Failed to get configuration");
	// Create Alice and Bob wallets
	let alice = CardanoClient::new(config.clone().ogmios_client, config.clone().test_files).await;

	let bob = CardanoClient::new(config.ogmios_client, config.test_files).await;
	let bob_address_bech32 = bob.address_as_bech32();
	let dust_hex = MidnightClient::new_dust_hex();

	// Fund Alice and Bob wallets
	let ada_to_fund = vec![Asset::new_from_str("lovelace", "160000000")];
	let alice_collateral = alice.make_collateral().await.expect("No alice collateral");
	let deregister_tx_in = alice
		.fund_wallet(ada_to_fund.clone())
		.await
		.expect("Funding Alice's wallet failed");

	let bob_collateral = bob.make_collateral().await.expect("No bob collateral");
	let register_tx_in =
		bob.fund_wallet(ada_to_fund.clone()).await.expect("Funding Bob's wallet failed");

	// Bob registers his DUST address
	println!("Registering Bob wallet {} with DUST address {}", bob_address_bech32, dust_hex);
	let register_tx_id = bob
		.register(&dust_hex, &register_tx_in, &bob_collateral)
		.await
		.expect("Registering Bob wallet failed")
		.transaction
		.id;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);

	// Find Bob's registration UTXO
	let validator_address =
		bob.test_files.policies.mapping_validator_address(bob.network_info.network_id());
	let register_tx = bob
		.find_utxo_by_tx_id(&validator_address, hex::encode(&register_tx_id))
		.await
		.expect("No registration UTXO found after registering");
	println!("Found registration UTXO: {:?}", register_tx);

	// Alice attempts to deregister Bob
	let deregister_tx = alice.deregister(&deregister_tx_in, &register_tx, &alice_collateral).await;
	assert!(deregister_tx.is_err(), "Alice should not be able to deregister Bob");

	// Check if Bob's registration still exists in mapping validator UTXOs
	let still_unspent = bob
		.is_utxo_unspent_for_3_blocks(&validator_address, &hex::encode(&register_tx_id))
		.await;
	assert!(still_unspent, "Bob's registration UTXO should still be unspent");
}

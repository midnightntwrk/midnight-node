use midnight_node_e2e::api::cardano::*;
use midnight_node_e2e::api::midnight::*;
use midnight_node_e2e::cfg::*;
use midnight_node_metadata::midnight_metadata::native_token_observation;
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
	let registration_events = subscribe_to_cngd_registration_extrinsic(&register_tx_id)
		.await
		.expect("Failed to listen to cNgD registration event");

	let registration = registration_events
		.iter()
		.filter_map(|e| e.ok())
		.filter_map(|evt| {
			evt.as_event::<native_token_observation::events::Registration>().ok().flatten()
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
			evt.as_event::<native_token_observation::events::MappingAdded>().ok().flatten()
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
	println!("Calculated nonce for cNIGHT minting: {}", nonce);
	// TODO assert utxoOwners
}

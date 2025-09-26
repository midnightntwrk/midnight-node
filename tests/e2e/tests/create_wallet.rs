#[path = "../config/mod.rs"]
mod config;
use midnight_node_e2e::api::cardano;
use midnight_node_e2e::api::cardano::*;
use midnight_node_e2e::api::midnight::*;
use midnight_node_metadata::midnight_metadata::{
	native_token_observation,
};
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

	let register_tx_id = register(&bech32_address, &dust_hex, &cardano_wallet, &tx_in, &collateral_utxo).await;
	println!("Registration transaction submitted with hash: {:?}", register_tx_id);


	let cardano_address = get_cardano_address_as_bytes(&cardano_wallet);
	let dust_address = hex::decode(&dust_hex).expect("Failed to decode DUST hex");
	let registration = subscribe_to_cngd_registration_event(&register_tx_id).await.expect("failed to listen to cngd registration event");
	for evt in registration.iter().filter_map(|e| e.ok()) {
		if let Some(registration) = evt.as_event::<native_token_observation::events::Registration>().ok().flatten() {
			println!("Registration event found: {:?}", registration);
			assert_eq!(registration.0.cardano_address.0, cardano_address, "Registered Cardano address does not match");
			assert_eq!(registration.0.dust_address, dust_address, "Registered DUST address does not match");
			return;
		}
	}
	assert!(false, "Did not find registration event in the returned events");
}

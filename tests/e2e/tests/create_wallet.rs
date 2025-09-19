#[path = "../config/mod.rs"]
mod config;
use config::load_config;
use midnight_node_e2e::api::cardano::*;
use whisky::Asset;
use ogmios_client::{query_ledger_state::QueryLedgerState};


#[tokio::test]
async fn register_for_dust_production() {
	let wallet = create_wallet();
	let bech32_address = get_wallet_address(&wallet);
	println!("New wallet: {:?}", bech32_address);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let cfg = load_config();
	let client = get_ogmios_client().await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_id = send(&bech32_address, assets).await;
	let tx_in = match find_utxo_by_tx_id(&client, &bech32_address, &tx_id).await {
		Some(utxo) => utxo,
		None => panic!("UTXO not found after funding"),
	};

	let utxos = client.query_utxos(&[bech32_address.clone().into()]).await.unwrap();
	assert_eq!(utxos.len(), 2, "New wallet should have exactly two UTXOs after funding");

	let dust_hex = new_dust_hex(32);
	println!("Generated dust hex: {}", dust_hex);

	register(&cfg.payment_addr, &dust_hex, &wallet, &tx_in, &collateral_utxo).await;
}

#[path = "../config/mod.rs"]
mod config;
use midnight_node_e2e::api::cardano::*;
use midnight_node_e2e::api::midnight::*;
use ogmios_client::query_ledger_state::QueryLedgerState;
use whisky::Asset;

#[tokio::test]
async fn register_for_dust_production() {
	let cardano_wallet = create_wallet();
	let bech32_address = get_wallet_address(&cardano_wallet);
	println!("New Cardano wallet created: {:?}", bech32_address);

	let dust_hex = new_dust_hex(32);
	println!("Registering Cardano wallet {} with DUST address {}", bech32_address, dust_hex);

	let collateral_utxo = make_collateral(&bech32_address).await;
	let assets = vec![Asset::new_from_str("lovelace", "160000000")];
	let tx_in = fund_wallet(&bech32_address, assets).await;

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(&[bech32_address.clone().into()]).await.unwrap();
	assert_eq!(utxos.len(), 2, "New wallet should have exactly two UTXOs after funding");

	register(&bech32_address, &dust_hex, &cardano_wallet, &tx_in, &collateral_utxo).await;
	subscribe_to_blocks().await.expect("failed to listen to blocks");
}

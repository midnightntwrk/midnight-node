#[path = "../../config/mod.rs"]
mod config;
use config::load_config;
use config::get_mapping_validator_address;
use config::get_auth_token_policy_id;
use config::get_local_env_cost_models;
use config::load_cbor;
use config::AppConfig;

use rand::RngCore;
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::{
	jsonrpsee::client_for_url, jsonrpsee::OgmiosClients, query_ledger_state::QueryLedgerState, transactions::*,
	types::OgmiosUtxo,
};

use std::fs;
use std::time::Duration;
use whisky::*;
use whisky::builder::*;
use whisky::csl::*;
use whisky::data::constr0;
use uplc::tx::script_context::SlotConfig;

/// Generate a random hex string of the given byte length (default 32 bytes)
pub fn new_dust_hex(bytes: usize) -> String {
	let mut a = vec![0u8; bytes];
	rand::rng().fill_bytes(&mut a);
	a.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

pub async fn find_utxo_by_tx_id(client: &OgmiosClients, address: &str, tx_id: &str) -> Option<OgmiosUtxo> {
	let mut attempts = 0;
	while attempts < 10 {
		let utxos = client.query_utxos(&[address.into()]).await.unwrap();
		println!("Checking for UTXO, attempt {}: {:?}", attempts + 1, utxos);
		for utxo in &utxos {
			if hex::encode(utxo.transaction.id) == tx_id {
				println!("Found UTXO: {:?}", utxo);
				return Some(utxo.clone());
			}
		}
		attempts += 1;
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	None
}

pub async fn get_ogmios_client() -> OgmiosClients {
    let cfg = load_config();
	let ogmios_url = cfg.ogmios_url.clone();
	client_for_url(&ogmios_url, Duration::from_secs(5)).await.unwrap()
}

pub fn build_asset_vector(ogmios_utxo: &OgmiosUtxo) -> Vec<Asset> {
	let mut input_assets: Vec<Asset> = Vec::new();
	input_assets.push(Asset::new_from_str("lovelace", &ogmios_utxo.value.lovelace.to_string()));
	for (policy_id, assets) in &ogmios_utxo.value.native_tokens {
		let policy_id_str = hex::encode(policy_id);
		for asset in assets {
			input_assets.push(Asset::new_from_str(&policy_id_str, asset.amount.to_string().as_str()));
		}
	}
	println!("Input assets: {:?}", input_assets);
	input_assets
}

pub async fn send(address: &str, assets: Vec<Asset>) -> String {
	let cfg = load_config();
	let payment_addr = cfg.payment_addr.clone();
	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(&[payment_addr.clone().into()]).await.unwrap();
	assert!(!utxos.is_empty(), "No UTXOs found for funding address");
	println!("payment_addr: {}", payment_addr);
	println!("address: {}", address);
	let env = std::env::var("ENV").unwrap_or_else(|_| "local".to_string());
	let payment_skey_path = format!("./config/{}/payment.skey", env);
	let skey_json = fs::read_to_string(&payment_skey_path).expect("Failed to read payment.skey");
	let skey_value: serde_json::Value = serde_json::from_str(&skey_json).expect("Invalid skey JSON");
	let cbor_hex = skey_value["cborHex"].as_str().expect("No cborHex in skey JSON");
	let utxo = &utxos[0];
	println!("UTXO: {:?}", utxo);
	let input_tx_hash = hex::encode(utxo.transaction.id);
	let input_index = utxo.index;
	let input_assets = build_asset_vector(&utxo);
	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.tx_in(&input_tx_hash, input_index.into(), &input_assets, address)
		.tx_out(address, &assets)
		.change_address(&payment_addr)
		.signing_key(&cbor_hex)
		.complete_sync(None)
		.unwrap()
		.complete_signing()
		.unwrap();
	println!("Signed tx hex: {}", tx_builder.tx_hex());
	let tx_bytes = hex::decode(tx_builder.tx_hex()).expect("Failed to decode hex string");
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	println!("Transaction submitted, response: {:?}", response);
	hex::encode(response.transaction.id)
}

pub async fn register(cardano_address: &str, midnight_address_hex: &str, signing_wallet: &Wallet, tx_in: &OgmiosUtxo, collateral_utxo: &OgmiosUtxo) {
    let cardano_address_hex = Address::from_bech32(cardano_address).unwrap().to_hex();
	let validator_address = get_mapping_validator_address();
	println!("Validator address: {}", validator_address);
	let client = get_ogmios_client().await;
	println!("cardano_address: {}", cardano_address_hex);
	println!("midnight_address: {}", midnight_address_hex);
	println!("UTXO: {:?}", tx_in);
	let payment_addr = get_wallet_address(signing_wallet);
	println!("Using payment address: {}", payment_addr);
	let input_tx_hash = hex::encode(tx_in.transaction.id);
	let input_index = tx_in.index;
	let input_assets = build_asset_vector(&tx_in);
	let auth_token_policy_id = get_auth_token_policy_id();
	let send_assets = vec![Asset::new_from_str("lovelace", "150000000"), Asset::new_from_str(&auth_token_policy_id, "1")];
	let datum = serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": [{ "bytes": cardano_address_hex }, { "bytes": midnight_address_hex }]})).unwrap();
    let cfg = load_config();
	let minting_script = load_cbor(&cfg.auth_token_policy_file);
	let network = Network::Custom(get_local_env_cost_models());
	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		.tx_in(&input_tx_hash, input_index.into(), &input_assets, &payment_addr)
		.tx_in_collateral(&hex::encode(collateral_utxo.transaction.id), collateral_utxo.index.into(), &build_asset_vector(&collateral_utxo), &payment_addr)
		.tx_out(&validator_address, &send_assets)
		.tx_out_inline_datum_value(&WData::JSON(datum))
		.mint_plutus_script_v2()
		.mint(1, &auth_token_policy_id, "")
		.minting_script(&minting_script)
		.mint_redeemer_value(&WRedeemer {
                data: WData::JSON(constr0(serde_json::json!([])).to_string()),
                ex_units: Budget { mem: 376570, steps: 94156294 },
            })
		.change_address(&payment_addr)
		.complete_sync(None)
		.unwrap();
	println!("{:?}", tx_builder.tx_builder_body);
	let signed_tx = signing_wallet.sign_tx(&tx_builder.tx_hex());
	println!("Signed tx hex: {:?}", signed_tx);
	let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	println!("Transaction submitted, response: {:?}", response);
}

pub fn create_wallet() -> Wallet {
	let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);
	let phrase = mnemonic.phrase();
	let wallet = Wallet::new_mnemonic(phrase).unwrap();
	wallet
}

pub fn get_wallet_address(wallet: &Wallet) -> String {
	let pub_key_hash = wallet.account.public_key.hash();
	let payment_cred = whisky::csl::Credential::from_keyhash(&pub_key_hash);
	let network = NetworkInfo::testnet_preview().network_id();
	let address = whisky::csl::EnterpriseAddress::new(network, &payment_cred);
	let bech32_address = address.to_address().to_bech32(None).unwrap();
	bech32_address
}

pub async fn make_collateral(address: &str) -> OgmiosUtxo {
	let assets = vec![Asset::new_from_str("lovelace", "5000000")];
	let tx_id = send(&address, assets).await;
	println!("Collateral funding transaction ID: {}", tx_id);
	let client = get_ogmios_client().await;
	match find_utxo_by_tx_id(&client, address, &tx_id).await {
		Some(utxo) => utxo,
		None => panic!("Collateral UTXO not found after funding"),
	}
}

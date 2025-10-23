use crate::cfg::*;
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::{
	jsonrpsee::client_for_url, jsonrpsee::OgmiosClients, query_ledger_state::QueryLedgerState,
	transactions::*, types::OgmiosUtxo,
};
use std::fs;
use std::time::Duration;
use whisky::csl::*;
use whisky::data::constr0;
use whisky::*;

pub async fn find_utxo_by_tx_id(address: &str, tx_id: &str) -> Option<OgmiosUtxo> {
	let client = get_ogmios_client().await;
	let mut attempts = 0;
	while attempts < 10 {
		let utxos = client.query_utxos(&[address.into()]).await.unwrap();
		println!("Waiting for UTXO with tx_id {}, attempt {}: {:?}", tx_id, attempts + 1, utxos);
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
			input_assets
				.push(Asset::new_from_str(&policy_id_str, asset.amount.to_string().as_str()));
		}
	}
	input_assets
}

pub async fn send(address: &str, assets: Vec<Asset>) -> String {
	let cfg = load_config();
	let payment_addr = cfg.payment_addr.clone();
	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(std::slice::from_ref(&payment_addr)).await.unwrap();
	assert!(!utxos.is_empty(), "No UTXOs found for funding address");
	let utxo = utxos
		.iter()
		.max_by_key(|u| u.value.lovelace)
		.expect("No UTXO with lovelace found");
	let skey_json =
		fs::read_to_string(&cfg.payment_skey_file).expect("Failed to read payment.skey");
	let skey_value: serde_json::Value =
		serde_json::from_str(&skey_json).expect("Invalid skey JSON");
	let cbor_hex = skey_value["cborHex"].as_str().expect("No cborHex in skey JSON");
	let input_tx_hash = hex::encode(utxo.transaction.id);
	let input_index = utxo.index;
	let input_assets = build_asset_vector(utxo);
	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.tx_in(&input_tx_hash, input_index.into(), &input_assets, address)
		.tx_out(address, &assets)
		.change_address(&payment_addr)
		.signing_key(cbor_hex)
		.complete_sync(None)
		.unwrap()
		.complete_signing()
		.unwrap();
	let tx_bytes = hex::decode(tx_builder.tx_hex()).expect("Failed to decode hex string");
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	hex::encode(response.transaction.id)
}

pub async fn register(
	cardano_address: &str,
	midnight_address_hex: &str,
	signing_wallet: &Wallet,
	tx_in: &OgmiosUtxo,
	collateral_utxo: &OgmiosUtxo,
) -> [u8; 32] {
	let validator_address = get_mapping_validator_address();
	let cardano_address_hex = Address::from_bech32(cardano_address).unwrap().to_hex();
	let datum = serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": [{ "bytes": cardano_address_hex }, { "bytes": midnight_address_hex }]})).unwrap();
	let payment_addr = get_cardano_address_as_bech32(signing_wallet);
	let auth_token_policy_id = get_auth_token_policy_id();
	let send_assets = vec![
		Asset::new_from_str("lovelace", "150000000"),
		Asset::new_from_str(&auth_token_policy_id, "1"),
	];
	let cfg = load_config();
	let minting_script = load_cbor(&cfg.auth_token_policy_file);
	let network = Network::Custom(get_local_env_cost_models());

	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		.tx_in(
			&hex::encode(tx_in.transaction.id),
			tx_in.index.into(),
			&build_asset_vector(tx_in),
			&payment_addr,
		)
		.tx_in_collateral(
			&hex::encode(collateral_utxo.transaction.id),
			collateral_utxo.index.into(),
			&build_asset_vector(collateral_utxo),
			&payment_addr,
		)
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

	let signed_tx = signing_wallet.sign_tx(&tx_builder.tx_hex());
	let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
	let client = get_ogmios_client().await;
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	println!("Transaction submitted, response: {:?}", response);
	response.transaction.id
}

pub fn create_wallet() -> Wallet {
	let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);
	let phrase = mnemonic.phrase();
	Wallet::new_mnemonic(phrase).unwrap()
}

pub fn get_cardano_address(wallet: &Wallet) -> Address {
	let pub_key_hash = wallet.account.public_key.hash();
	let payment_cred = whisky::csl::Credential::from_keyhash(&pub_key_hash);
	let network = NetworkInfo::testnet_preview().network_id();
	whisky::csl::EnterpriseAddress::new(network, &payment_cred).to_address()
}

pub fn get_cardano_address_as_bech32(wallet: &Wallet) -> String {
	let address = get_cardano_address(wallet);
	address.to_bech32(None).unwrap()
}

pub fn get_cardano_address_as_bytes(wallet: &Wallet) -> Vec<u8> {
	let address = get_cardano_address(wallet);
	address.to_bytes()
}

pub async fn make_collateral(address: &str) -> OgmiosUtxo {
	let assets = vec![Asset::new_from_str("lovelace", "5000000")];
	let tx_id = send(address, assets).await;
	println!("Collateral transaction ID: {}", tx_id);
	match find_utxo_by_tx_id(address, &tx_id).await {
		Some(utxo) => utxo,
		None => panic!("Collateral UTXO not found after funding"),
	}
}

pub async fn fund_wallet(address: &str, assets: Vec<Asset>) -> OgmiosUtxo {
	let tx_id = send(address, assets).await;
	println!("Funding transaction ID: {}", tx_id);
	match find_utxo_by_tx_id(address, &tx_id).await {
		Some(utxo) => utxo,
		None => panic!("Wallet UTXO not found after funding"),
	}
}

pub async fn mint_tokens(
	wallet: &Wallet,
	policy_id: &str,
	amount: &str,
	minting_script: &str,
) -> [u8; 32] {
	let network = Network::Custom(get_local_env_cost_models());
	let payment_addr = get_cardano_address_as_bech32(wallet);
	let collateral_utxo = make_collateral(&payment_addr).await;

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(std::slice::from_ref(&payment_addr)).await.unwrap();
	assert!(!utxos.is_empty(), "No UTXOs found for payment address {}", payment_addr);
	let utxo = utxos
		.iter()
		.max_by_key(|u| u.value.lovelace)
		.expect("No UTXO with lovelace found");
	let input_tx_hash = hex::encode(utxo.transaction.id);
	let input_index = utxo.index;
	let input_assets = build_asset_vector(utxo);

	let assets =
		vec![Asset::new_from_str("lovelace", "1500000"), Asset::new_from_str(policy_id, amount)];

	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		.tx_in(&input_tx_hash, input_index.into(), &input_assets, &payment_addr)
		.tx_in_collateral(
			&hex::encode(collateral_utxo.transaction.id),
			collateral_utxo.index.into(),
			&build_asset_vector(&collateral_utxo),
			&payment_addr,
		)
		.tx_out(&payment_addr, &assets)
		.mint_plutus_script_v2()
		.mint(amount.parse().unwrap(), policy_id, "")
		.minting_script(minting_script)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(constr0(serde_json::json!([])).to_string()),
			ex_units: Budget { mem: 376570, steps: 94156294 },
		})
		.change_address(&payment_addr)
		.complete_sync(None)
		.unwrap();

	let signed_tx = wallet.sign_tx(&tx_builder.tx_hex());
	let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	println!("Transaction submitted, response: {:?}", response);
	response.transaction.id
}

#[path = "../config/mod.rs"]
mod config;
use config::load_config;
use config::get_mapping_validator_address;
use config::get_auth_token_policy_id;
use whisky::csl::Address;
use whisky::Protocol;

use ogmios_client::{
	jsonrpsee::client_for_url, jsonrpsee::OgmiosClients, query_ledger_state::QueryLedgerState, transactions::*,
	types::OgmiosUtxo,
};
use std::time::Duration;

#[tokio::test]
async fn test_load_config() {
	let cfg = load_config();
	println!("Loaded config: {:?}", cfg);
	assert!(!cfg.ogmios_url.is_empty(), "ogmios_url must be set in config");
	assert!(!cfg.payment_addr.is_empty(), "payment_addr must be set in config");
	assert!(!cfg.mapping_validator_policy_file.is_empty(), "mapping_validator_policy_file must be set in config");
	assert!(!cfg.auth_token_policy_file.is_empty(), "auth_token_policy_file must be set in config");
}

#[tokio::test]
async fn bech32_address_to_hex() {
	let cfg = load_config();
	let bech32_addr = &cfg.payment_addr;
	let address = Address::from_bech32(bech32_addr).expect("Invalid bech32 address");
	println!("Parsed address: {:?}", address);
	let hex_addr = address.to_hex();
	println!("Hex address: {}", hex_addr);
	let expected_hex_addr = "60e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b";
	assert_eq!(hex_addr, expected_hex_addr, "Hex address does not match expected value
		Expected: {}, Actual: {}", expected_hex_addr, hex_addr);
}

#[tokio::test]
async fn mapping_validator_address() {
	let validator_address = get_mapping_validator_address();
	println!("Mapping Validator Address: {}", validator_address);
	let expected_mapping_validator_address = "addr_test1wral0lzw5kpjytmw0gmsdcgctx09au24nt85zma38py8g3crwvpwe";
	assert_eq!(validator_address, expected_mapping_validator_address, "Derived mapping validator address does not match");
}

#[tokio::test]
async fn auth_token_policy_id() {
	let policy_id = get_auth_token_policy_id();
	println!("Auth Token Policy ID: {}", policy_id);
	let expected_policy_id = "5152458e042159beca8f5efe14e4848444a5ead9c49cf3d389f449f5";
	assert_eq!(policy_id, expected_policy_id, "Derived auth token policy ID does not match");
}

// #[tokio::test]
// async fn protocol_parameters() {
// 	let cfg = load_config();
// 	let ogmios_url = cfg.ogmios_url.clone();
// 	let client = client_for_url(&ogmios_url, Duration::from_secs(5)).await.unwrap();
// 	let protocol_params = client.query_protocol_parameters().await.unwrap();
// 	println!("Protocol Parameters: {:?}", protocol_params);
// 	let protocol = Protocol {
// 		epoch: 60,
// 		min_fee_a: 44,
// 		min_fee_b: 155381,
// 		max_block_size: 65536,
// 		max_tx_size: 16384,
// 		max_block_header_size: 1100,
// 		key_deposit: 400000,
// 		pool_deposit: 500000000,
// 		decentralisation: 0.8,
// 		min_pool_cost: 0.to_string(),
// 		price_mem: 5.77e-2,
// 		price_step: 7.21e-5,
// 		max_tx_ex_mem: "14000000".to_string(),
// 		max_tx_ex_steps: "10000000000".to_string(),
// 		max_block_ex_mem: "62000000".to_string(),
// 		max_block_ex_steps: "40000000000".to_string(),
// 		max_val_size: 5000,
// 		collateral_percent: 150.0,
// 		max_collateral_inputs: 3,
// 		coins_per_utxo_size: 34482,
// 		min_fee_ref_script_cost_per_byte: 1,
// 	};
// 	println!("Expected Protocol Parameters: {:?}", protocol);
// }

#[tokio::test]
async fn cost_models() {
	let cost_models = whisky::constants::get_preview_cost_models();
	println!("Cost Models: {:?}", cost_models);
}

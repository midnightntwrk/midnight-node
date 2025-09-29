#[path = "../config/mod.rs"]
mod config;
use config::get_auth_token_policy_id;
use config::get_mapping_validator_address;
use config::load_config;
use whisky::csl::Address;

#[tokio::test]
async fn test_load_config() {
	let cfg = load_config();
	println!("Loaded config: {:?}", cfg);
	assert!(!cfg.ogmios_url.is_empty(), "ogmios_url must be set in config");
	assert!(!cfg.payment_addr.is_empty(), "payment_addr must be set in config");
	assert!(
		!cfg.mapping_validator_policy_file.is_empty(),
		"mapping_validator_policy_file must be set in config"
	);
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
	assert_eq!(
		hex_addr, expected_hex_addr,
		"Hex address does not match expected value
		Expected: {}, Actual: {}",
		expected_hex_addr, hex_addr
	);
}

#[tokio::test]
async fn mapping_validator_address() {
	let validator_address = get_mapping_validator_address();
	println!("Mapping Validator Address: {}", validator_address);
	let expected_mapping_validator_address =
		"addr_test1wral0lzw5kpjytmw0gmsdcgctx09au24nt85zma38py8g3crwvpwe";
	assert_eq!(
		validator_address, expected_mapping_validator_address,
		"Derived mapping validator address does not match"
	);
}

#[tokio::test]
async fn auth_token_policy_id() {
	let policy_id = get_auth_token_policy_id();
	println!("Auth Token Policy ID: {}", policy_id);
	let expected_policy_id = "5152458e042159beca8f5efe14e4848444a5ead9c49cf3d389f449f5";
	assert_eq!(policy_id, expected_policy_id, "Derived auth token policy ID does not match");
}

#[tokio::test]
async fn cost_models() {
	let cost_models = whisky::constants::get_preview_cost_models();
	println!("Cost Models: {:?}", cost_models);
}

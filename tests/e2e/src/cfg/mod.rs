use config as config_rs;
use serde::Deserialize;
use std::fs;
use whisky::csl::NetworkInfo;
use whisky::LanguageVersion;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
	pub node_url: String,
	pub ogmios_url: String,
	pub payment_addr: String,
	pub payment_addr_file: String,
	pub payment_skey_file: String,
	pub mapping_validator_policy_file: String,
	pub auth_token_policy_file: String,
	pub council_forever_file: String,
	pub tech_auth_forever_file: String,
}

pub fn load_config() -> AppConfig {
	let env = std::env::var("ENV").unwrap_or_else(|_| "local".to_string());
	let config_path = format!("./src/cfg/{}/config.toml", env);
	let settings = config_rs::Config::builder()
		.add_source(config_rs::File::with_name(&config_path))
		.build();
	let mut cfg: AppConfig = settings.unwrap().try_deserialize().unwrap();
	cfg.payment_addr = fs::read_to_string(&cfg.payment_addr_file)
		.expect("Failed to read payment address file")
		.trim()
		.to_string();
	cfg
}

pub fn load_cbor(path: &str) -> String {
	let file_content = std::fs::read_to_string(path).expect("Failed to read file");
	match serde_json::from_str::<serde_json::Value>(&file_content) {
		Ok(json) => json
			.get("cborHex")
			.and_then(|v| v.as_str())
			.map(|s| s.to_string())
			.expect("No cborHex in file"),
		Err(_) => panic!("File is not valid JSON"),
	}
}

/// Add CBOR wrapper to a script for use in transactions
/// Cardano requires double CBOR encoding: the script itself is CBOR-encoded,
/// then that is wrapped in another CBOR bytestring
pub fn wrap_script_cbor(inner_cbor_hex: &str) -> String {
	// Decode the inner CBOR hex to bytes
	let inner_bytes = hex::decode(inner_cbor_hex).expect("Invalid hex string");

	// CBOR encode as bytestring: 0x58 for bytestring with 1-byte length prefix
	// or 0x59 for 2-byte length, 0x5a for 4-byte length
	let len = inner_bytes.len();
	let mut wrapped = Vec::new();

	if len <= 23 {
		// Tiny bytestring: length in the type byte itself (0x40 + len)
		wrapped.push(0x40 + len as u8);
	} else if len <= 255 {
		// Short bytestring: 0x58 + 1-byte length
		wrapped.push(0x58);
		wrapped.push(len as u8);
	} else if len <= 65535 {
		// Medium bytestring: 0x59 + 2-byte length (big-endian)
		wrapped.push(0x59);
		wrapped.extend_from_slice(&(len as u16).to_be_bytes());
	} else {
		// Large bytestring: 0x5a + 4-byte length (big-endian)
		wrapped.push(0x5a);
		wrapped.extend_from_slice(&(len as u32).to_be_bytes());
	}

	wrapped.extend_from_slice(&inner_bytes);
	hex::encode(wrapped)
}

pub fn load_script_hash(path: &str) -> String {
	let file_content = std::fs::read_to_string(path).expect("Failed to read file");
	match serde_json::from_str::<serde_json::Value>(&file_content) {
		Ok(json) => json
			.get("hash")
			.and_then(|v| v.as_str())
			.map(|s| s.to_string())
			.expect("No hash in file"),
		Err(_) => panic!("File is not valid JSON"),
	}
}

pub fn get_mapping_validator_address() -> String {
	let cfg = load_config();
	let cbor_hex = load_cbor(&cfg.mapping_validator_policy_file);
	let script_hash = whisky::get_script_hash(&cbor_hex, LanguageVersion::V2);
	let network = NetworkInfo::testnet_preview().network_id();
	whisky::script_to_address(network, &script_hash.unwrap(), None)
}

pub fn get_auth_token_policy_id() -> String {
	let cfg = load_config();
	let cbor_hex = load_cbor(&cfg.auth_token_policy_file);
	let script_hash = whisky::get_script_hash(&cbor_hex, LanguageVersion::V2);
	script_hash.expect("Error calculating `auth_token_policy_id`")
}

pub fn get_council_forever_cbor() -> String {
	let cfg = load_config();
	let inner_cbor = load_cbor(&cfg.council_forever_file);
	// V3 scripts from Aiken need double CBOR encoding
	wrap_script_cbor(&inner_cbor)
}

pub fn get_council_forever_policy_id() -> String {
	let cbor_hex = get_council_forever_cbor();
	let script_hash = whisky::get_script_hash(&cbor_hex, LanguageVersion::V3);
	script_hash.expect("Error calculating `council_forever_policy_id`")
}

pub fn get_council_forever_address() -> String {
	let script_hash = get_council_forever_policy_id();
	let network = NetworkInfo::testnet_preview().network_id();
	whisky::script_to_address(network, &script_hash, None)
}

pub fn get_tech_auth_forever_cbor() -> String {
	let cfg = load_config();
	let inner_cbor = load_cbor(&cfg.tech_auth_forever_file);
	// V3 scripts from Aiken need double CBOR encoding
	wrap_script_cbor(&inner_cbor)
}

pub fn get_tech_auth_forever_policy_id() -> String {
	let cbor_hex = get_tech_auth_forever_cbor();
	let script_hash = whisky::get_script_hash(&cbor_hex, LanguageVersion::V3);
	script_hash.expect("Error calculating `tech_auth_forever_policy_id`")
}

pub fn get_tech_auth_forever_address() -> String {
	let script_hash = get_tech_auth_forever_policy_id();
	let network = NetworkInfo::testnet_preview().network_id();
	whisky::script_to_address(network, &script_hash, None)
}

pub fn get_local_env_cost_models() -> Vec<Vec<i64>> {
	vec![
		vec![
			100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4,
			16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100,
			16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769,
			4, 2, 85848, 228465, 122, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148,
			27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32,
			76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541,
			1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 228465, 122,
			0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848, 228465, 122,
			0, 1, 1, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420,
			1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32, 24623, 32,
			53384111, 14333, 10,
		],
		vec![
			100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4,
			16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100,
			16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769,
			4, 2, 85848, 228465, 122, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148,
			27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32,
			76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541,
			1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 228465, 122,
			0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848, 228465, 122,
			0, 1, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0,
			141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32,
			25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10, 100000,
			100000, 100000, 100000, 100000, 100000, 100000, 100000, 100000, 100000,
		],
		// Plutus V3 cost models (from local-environment genesis.conway.json)
		vec![
			100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4,
			16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100,
			16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769,
			4, 2, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 1, 1000, 42921, 4, 2,
			24548, 29498, 38, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895,
			32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1,
			43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32,
			11546, 32, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 90434, 519, 0, 1,
			74433, 32, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 1, 85848, 123203,
			7305, -900, 1716, 549, 57, 85848, 0, 1, 955506, 213312, 0, 2, 270652, 22588, 4,
			1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32,
			20142, 32, 24588, 32, 20744, 32, 25933, 32, 24623, 32, 43053543, 10, 53384111, 14333,
			10, 43574283, 26308, 10, 16000, 100, 16000, 100, 962335, 18, 2780678, 6, 442008, 1,
			52538055, 3756, 18, 267929, 18, 76433006, 8868, 18, 52948122, 18, 1995836, 36, 3227919,
			12, 901022, 1, 166917843, 4307, 36, 284546, 36, 158221314, 26549, 36, 74698472, 36,
			333849714, 1, 254006273, 72, 2174038, 72, 2261318, 64571, 4, 207616, 8310, 4, 1293828,
			28716, 63, 0, 1, 1006041, 43623, 251, 0, 1,
		],
	]
}

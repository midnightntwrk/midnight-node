use super::*;
use crate::ogmios::config::tests::{
	default_ogmios_service_config, establish_ogmios_configuration_io,
	prompt_ogmios_configuration_io,
};
use crate::ogmios::test_values::preview_shelley_config;
use crate::ogmios::{OgmiosRequest, OgmiosResponse};
use crate::select_utxo::tests::{mock_7_valid_utxos_rows, mock_result_7_valid, query_utxos_io};
use crate::tests::{
	CHAIN_CONFIG_FILE_PATH, MockIO, MockIOContext, OffchainMock, OffchainMocks,
	RESOURCES_CONFIG_FILE_PATH,
};
use crate::verify_json;
use hex_literal::hex;
use serde_json::json;
use sidechain_domain::{
	AdaBasedStaking, CandidateKeys, CandidateRegistration, MainchainKeyHash, McTxHash, UtxoId,
};

const NODE_URL: &str = "http://localhost:9944";
const PAYMENT_VKEY_PATH: &str = "payment.vkey";
const GENESIS_UTXO: &str = "0000000000000000000000000000000000000000000000000000000000000001#0";
const REGISTRATION_UTXO: &str =
	"4704a903b01514645067d851382efd4a6ed5d2ff07cf30a538acc78fed7c4c02#93";
const PARTNER_CHAINS_KEY: &str =
	"0x031e75acbf45ef8df98bbe24b19b28fff807be32bf88838c30c0564d7bec5301f6";
// deterministic ECDSA signature over the RegisterValidatorMessage built from the values above
const PC_SIGNATURE: &str = "6e295e36a6b11d8b1c5ec01ac8a639b466fbfbdda94b39ea82b0992e303d58543341345fc705e09c7838786ba0bc746d9038036f66a36d1127d924c4a0228bec";
const ECDSA_KEY_PATH: &str = "/path/to/data/keystore/63726368031e75acbf45ef8df98bbe24b19b28fff807be32bf88838c30c0564d7bec5301f6";
const ECDSA_KEY_FILE_CONTENT: &str =
	"\"end fury stamp spatial focus tired video tumble good critic tail hood\"";

fn rotated_keys_blob() -> Vec<u8> {
	vec![1, 2, 3]
}

fn new_aura_hex() -> String {
	"aa".repeat(32)
}

fn new_gran_hex() -> String {
	"bb".repeat(32)
}

fn decoded_keys() -> Vec<(KeyTypeId, Vec<u8>)> {
	vec![(KeyTypeId(*b"aura"), [0xaa; 32].to_vec()), (KeyTypeId(*b"gran"), [0xbb; 32].to_vec())]
}

fn rotate_keys_cmd(
	node_url: Option<&str>,
	mainchain_signing_key_file: Option<&str>,
	payment_signing_key_file: Option<&str>,
) -> RotateKeysCmd {
	RotateKeysCmd {
		common_arguments: crate::CommonArguments { retry_delay_seconds: 5, retry_count: 59 },
		node_url: node_url.map(String::from),
		mainchain_signing_key_file: mainchain_signing_key_file.map(String::from),
		payment_signing_key_file: payment_signing_key_file.map(String::from),
	}
}

fn chain_config_content() -> serde_json::Value {
	json!({
		"chain_parameters": {
			"genesis_utxo": GENESIS_UTXO,
		},
		"cardano": {
			"network": "testnet"
		}
	})
}

fn resource_config_content() -> serde_json::Value {
	json!({
		"substrate_node_base_path": "/path/to/data",
		"cardano_payment_verification_key_file": PAYMENT_VKEY_PATH,
	})
}

fn generated_keys_file_content() -> serde_json::Value {
	json!({
		"partner_chains_key": PARTNER_CHAINS_KEY,
		"keys": {
			"aura": "0xdf883ee0648f33b6103017b61be702017742d501b8fe73b1d69ca0157460b777",
			"gran": "0x5a091a06abd64f245db11d2987b03218c6bd83d64c262fe10e3a2a1230e90327"
		}
	})
}

fn expected_keys_file_json() -> serde_json::Value {
	json!({
		"partner_chains_key": PARTNER_CHAINS_KEY,
		"keys": {
			"aura": format!("0x{}", new_aura_hex()),
			"gran": format!("0x{}", new_gran_hex()),
		}
	})
}

fn expected_keys_file_content() -> String {
	serde_json::to_string_pretty(&PermissionedCandidateKeys {
		partner_chains_key: ByteString(
			hex!("031e75acbf45ef8df98bbe24b19b28fff807be32bf88838c30c0564d7bec5301f6").to_vec(),
		),
		keys: [
			("aura".to_string(), ByteString([0xaa; 32].to_vec())),
			("gran".to_string(), ByteString([0xbb; 32].to_vec())),
		]
		.into_iter()
		.collect(),
	})
	.unwrap()
}

const PAYMENT_VKEY_CONTENT: &str = r#"
{
    "type": "PaymentVerificationKeyShelley_ed25519",
    "description": "Payment Verification Key",
    "cborHex": "5820a35ef86f1622172816bb9e916aea86903b2c8d32c728ad5c9b9472be7e3c5e88"
}
"#;

fn intro_io() -> Vec<MockIO> {
	vec![
		MockIO::print("⚙️ Rotating session keys of a registered committee candidate"),
		MockIO::print(
			"This wizard generates fresh session keys on a RUNNING partner chain node and re-registers the candidate with them. The cross-chain identity key is never rotated.",
		),
		MockIO::print(
			"The node must accept the 'author_rotateKeys' RPC call. It is allowed by default for localhost connections; otherwise the node must be run with '--rpc-methods=unsafe'.",
		),
		MockIO::enewline(),
	]
}

fn load_base_path_io() -> Vec<MockIO> {
	vec![MockIO::eprint(
		"🛠️ Loaded node base path from config (test-pc-resources-config.json): /path/to/data",
	)]
}

fn node_url_prompt_io() -> Vec<MockIO> {
	vec![MockIO::prompt(
		"Enter the URL of the partner chain node RPC endpoint",
		Some(NODE_URL),
		NODE_URL,
	)]
}

fn rotate_io() -> Vec<MockIO> {
	vec![
		MockIO::eprint("⚙️ Rotating session keys via http://localhost:9944"),
		MockIO::substrate_rpc(
			NODE_URL,
			SubstrateRpcRequest::AuthorRotateKeys,
			Ok(SubstrateRpcResponse::RotatedKeys(rotated_keys_blob())),
		),
		MockIO::eprint("🔑 New session keys (opaque): 0x010203"),
		MockIO::substrate_rpc(
			NODE_URL,
			SubstrateRpcRequest::DecodeSessionKeys { encoded: rotated_keys_blob() },
			Ok(SubstrateRpcResponse::DecodedKeys(Some(decoded_keys()))),
		),
	]
}

fn write_keys_file_io() -> Vec<MockIO> {
	vec![
		MockIO::prompt_yes_no(
			"keys file partner-chains-public-keys.json exists - overwrite it?",
			false,
			true,
		),
		MockIO::eprint(
			"🔑 The following public keys were generated and saved to the partner-chains-public-keys.json file:",
		),
		MockIO::print(&expected_keys_file_content()),
		MockIO::enewline(),
	]
}

fn select_utxo_io() -> Vec<MockIO> {
	vec![
		MockIO::print(
			"This wizard will query your UTXOs using address derived from the payment verification key and Ogmios service",
		),
		prompt_ogmios_configuration_io(
			&default_ogmios_service_config(),
			&default_ogmios_service_config(),
		),
		MockIO::ogmios_request(
			"http://localhost:1337",
			OgmiosRequest::QueryNetworkShelleyGenesis,
			Ok(OgmiosResponse::QueryNetworkShelleyGenesis(preview_shelley_config())),
		),
		MockIO::prompt(
			"Enter the path to the payment verification file",
			Some(PAYMENT_VKEY_PATH),
			PAYMENT_VKEY_PATH,
		),
		query_utxos_io(
			"addr_test1vqezxrh24ts0775hulcg3ejcwj7hns8792vnn8met6z9gwsxt87zy",
			"http://localhost:1337",
			mock_result_7_valid(),
		),
		MockIO::prompt_multi_option(
			"Select UTXO to use for registration",
			mock_7_valid_utxos_rows(),
			&format!("{REGISTRATION_UTXO} (1100000 lovelace)"),
		),
		MockIO::print(
			"Please do not spend this UTXO, it needs to be consumed by the registration transaction.",
		),
		MockIO::print(""),
	]
}

fn staged_mode_output_io() -> Vec<MockIO> {
	vec![
		MockIO::prompt_yes_no(
			"Is your SPO cold signing key available on this machine?",
			false,
			false,
		),
		MockIO::print(
			"Run the following command to generate signatures on the next step. It has to be executed on the machine with your SPO cold signing key.",
		),
		MockIO::print(""),
		MockIO::print(&format!(
			"<mock executable> wizards register2 \\
--genesis-utxo {GENESIS_UTXO} \\
--registration-utxo {REGISTRATION_UTXO} \\
--partner-chain-pub-key {PARTNER_CHAINS_KEY} \\
--partner-chain-signature {PC_SIGNATURE} \\
--keys aura:{} \\
--keys gran:{}",
			new_aura_hex(),
			new_gran_hex()
		)),
	]
}

fn final_warnings_io() -> Vec<MockIO> {
	vec![
		MockIO::enewline(),
		MockIO::eprint(
			"⚠️ The new session keys take effect only after the updated registration is observed on Cardano and a committee using it is selected: registrations included in mainchain epoch N become effective in epoch N+2.",
		),
		MockIO::eprint(
			"⚠️ 'author_rotateKeys' adds new keys to the node keystore without deleting the previous ones. Keep the old keys in the keystore until the last committee selected with them has finished.",
		),
	]
}

#[test]
fn happy_path_staged_mode() {
	let mock_context = MockIOContext::new()
		.with_json_file(CHAIN_CONFIG_FILE_PATH, chain_config_content())
		.with_json_file(RESOURCES_CONFIG_FILE_PATH, resource_config_content())
		.with_json_file(KEYS_FILE_PATH, generated_keys_file_content())
		.with_file(ECDSA_KEY_PATH, ECDSA_KEY_FILE_CONTENT)
		.with_file(PAYMENT_VKEY_PATH, PAYMENT_VKEY_CONTENT)
		.with_expected_io(
			vec![
				intro_io(),
				load_base_path_io(),
				node_url_prompt_io(),
				rotate_io(),
				write_keys_file_io(),
				select_utxo_io(),
				staged_mode_output_io(),
				final_warnings_io(),
			]
			.into_iter()
			.flatten()
			.collect::<Vec<MockIO>>(),
		);

	let result = rotate_keys_cmd(None, None, None).run(&mock_context);
	result.expect("should succeed");
	verify_json!(mock_context, KEYS_FILE_PATH, expected_keys_file_json());
}

#[test]
fn happy_path_full_mode() {
	let registration_message = RegisterValidatorMessage {
		genesis_utxo: GENESIS_UTXO.parse().unwrap(),
		sidechain_pub_key: SidechainPublicKey(
			hex!("031e75acbf45ef8df98bbe24b19b28fff807be32bf88838c30c0564d7bec5301f6").to_vec(),
		),
		registration_utxo: REGISTRATION_UTXO.parse().unwrap(),
	};
	let cold_signing_key = ed25519_zebra::SigningKey::from(hex!(
		"0c049bb92212b779ee8ba9550536d8103cc1892634f0d21dcaa8944f5e4bf718"
	));
	let (spo_public_key, spo_signature) =
		cardano_spo_public_key_and_signature(cold_signing_key, registration_message);

	let candidate_registration = CandidateRegistration {
		stake_ownership: AdaBasedStaking { pub_key: spo_public_key, signature: spo_signature },
		partner_chain_pub_key: SidechainPublicKey(
			hex!("031e75acbf45ef8df98bbe24b19b28fff807be32bf88838c30c0564d7bec5301f6").to_vec(),
		),
		partner_chain_signature: SidechainSignature(hex::decode(PC_SIGNATURE).unwrap()),
		own_pkh: MainchainKeyHash(hex!("7fa48bb8fb5d6804fad26237738ce490d849e4567161e38ab8415ff3")),
		registration_utxo: REGISTRATION_UTXO.parse::<UtxoId>().unwrap(),
		keys: CandidateKeys(vec![
			CandidateKeyParam::new(*b"aura", [0xaa; 32].to_vec()).into(),
			CandidateKeyParam::new(*b"gran", [0xbb; 32].to_vec()).into(),
		]),
	};

	let offchain_mock = OffchainMock::new().with_register(
		GENESIS_UTXO.parse().unwrap(),
		candidate_registration,
		hex!("d75c630516c33a66b11b3444a70b65083aeb21353bd919cc5e3daa02c9732a84").to_vec(),
		Ok(Some(McTxHash::default())),
	);

	let mock_context = MockIOContext::new()
		.with_json_file(CHAIN_CONFIG_FILE_PATH, chain_config_content())
		.with_json_file(RESOURCES_CONFIG_FILE_PATH, resource_config_content())
		.with_json_file(KEYS_FILE_PATH, generated_keys_file_content())
		.with_json_file("/path/to/cold.skey", coldkey_content())
		.with_json_file("/path/to/payment.skey", payment_skey_content())
		.with_file(ECDSA_KEY_PATH, ECDSA_KEY_FILE_CONTENT)
		.with_file(PAYMENT_VKEY_PATH, PAYMENT_VKEY_CONTENT)
		.with_offchain_mocks(OffchainMocks::new_with_mock("http://localhost:1337", offchain_mock))
		.with_expected_io(
			vec![
				intro_io(),
				load_base_path_io(),
				rotate_io(),
				write_keys_file_io(),
				select_utxo_io(),
				submit_registration_io(),
				final_warnings_io(),
			]
			.into_iter()
			.flatten()
			.collect::<Vec<MockIO>>(),
		);

	let result =
		rotate_keys_cmd(Some(NODE_URL), Some("/path/to/cold.skey"), None).run(&mock_context);
	result.expect("should succeed");
	verify_json!(mock_context, KEYS_FILE_PATH, expected_keys_file_json());
}

fn submit_registration_io() -> Vec<MockIO> {
	vec![
		MockIO::print(
			"To proceed with the next command, a payment signing key is required. Please note that this key will not be stored or communicated over the network.",
		),
		MockIO::prompt(
			"Enter the path to the payment signing key file",
			Some("payment.skey"),
			"/path/to/payment.skey",
		),
		establish_ogmios_configuration_io(
			Some(default_ogmios_service_config()),
			default_ogmios_service_config(),
		),
		MockIO::prompt_yes_no("Show registration status?", true, false),
	]
}

fn coldkey_content() -> serde_json::Value {
	json!({
		"type": "StakePoolSigningKey_ed25519",
		"description": "Stake Pool Operator Signing Key",
		"cborHex": "58200c049bb92212b779ee8ba9550536d8103cc1892634f0d21dcaa8944f5e4bf718"
	})
}

fn payment_skey_content() -> serde_json::Value {
	json!({
		"type": "PaymentSigningKeyShelley_ed25519",
		"description": "Payment Signing Key",
		"cborHex": "5820d75c630516c33a66b11b3444a70b65083aeb21353bd919cc5e3daa02c9732a84"
	})
}

#[test]
fn reports_error_when_rotate_keys_rpc_fails() {
	let mock_context = MockIOContext::new()
		.with_json_file(CHAIN_CONFIG_FILE_PATH, chain_config_content())
		.with_json_file(RESOURCES_CONFIG_FILE_PATH, resource_config_content())
		.with_json_file(KEYS_FILE_PATH, generated_keys_file_content())
		.with_file(ECDSA_KEY_PATH, ECDSA_KEY_FILE_CONTENT)
		.with_expected_io(
			vec![
				intro_io(),
				load_base_path_io(),
				node_url_prompt_io(),
				vec![
					MockIO::eprint("⚙️ Rotating session keys via http://localhost:9944"),
					MockIO::substrate_rpc(
						NODE_URL,
						SubstrateRpcRequest::AuthorRotateKeys,
						Err(anyhow!(
							"'author_rotateKeys' RPC call to {NODE_URL} was rejected as unsafe: Unsafe RPC called. The node allows unsafe RPC methods for localhost connections by default; for other addresses it must be started with '--rpc-methods=unsafe'."
						)),
					),
				],
			]
			.into_iter()
			.flatten()
			.collect::<Vec<MockIO>>(),
		);

	let result = rotate_keys_cmd(None, None, None).run(&mock_context);
	let error = result.expect_err("should return error");
	assert!(error.to_string().contains("unsafe"));
}

#[test]
fn reports_error_when_runtime_cannot_decode_session_keys() {
	let mock_context = MockIOContext::new()
		.with_json_file(CHAIN_CONFIG_FILE_PATH, chain_config_content())
		.with_json_file(RESOURCES_CONFIG_FILE_PATH, resource_config_content())
		.with_json_file(KEYS_FILE_PATH, generated_keys_file_content())
		.with_file(ECDSA_KEY_PATH, ECDSA_KEY_FILE_CONTENT)
		.with_expected_io(
			vec![
				intro_io(),
				load_base_path_io(),
				node_url_prompt_io(),
				vec![
					MockIO::eprint("⚙️ Rotating session keys via http://localhost:9944"),
					MockIO::substrate_rpc(
						NODE_URL,
						SubstrateRpcRequest::AuthorRotateKeys,
						Ok(SubstrateRpcResponse::RotatedKeys(rotated_keys_blob())),
					),
					MockIO::eprint("🔑 New session keys (opaque): 0x010203"),
					MockIO::substrate_rpc(
						NODE_URL,
						SubstrateRpcRequest::DecodeSessionKeys { encoded: rotated_keys_blob() },
						Ok(SubstrateRpcResponse::DecodedKeys(None)),
					),
				],
			]
			.into_iter()
			.flatten()
			.collect::<Vec<MockIO>>(),
		);

	let result = rotate_keys_cmd(None, None, None).run(&mock_context);
	let error = result.expect_err("should return error");
	assert!(error.to_string().contains("could not decode the rotated session keys"));
}

#[test]
fn reports_error_when_keystore_key_does_not_match_keys_file() {
	let mock_context = MockIOContext::new()
		.with_json_file(CHAIN_CONFIG_FILE_PATH, chain_config_content())
		.with_json_file(RESOURCES_CONFIG_FILE_PATH, resource_config_content())
		.with_json_file(KEYS_FILE_PATH, generated_keys_file_content())
		// valid seed phrase, but it derives a different public key than the keys file claims
		.with_file(ECDSA_KEY_PATH, "\"//Alice\"")
		.with_expected_io(
			vec![intro_io(), load_base_path_io()]
				.into_iter()
				.flatten()
				.collect::<Vec<MockIO>>(),
		);

	let result = rotate_keys_cmd(None, None, None).run(&mock_context);
	let error = result.expect_err("should return error");
	assert!(error.to_string().contains("does not match"));
}

#[test]
fn refusing_to_overwrite_keys_file_aborts_before_any_on_chain_action() {
	let mock_context = MockIOContext::new()
		.with_json_file(CHAIN_CONFIG_FILE_PATH, chain_config_content())
		.with_json_file(RESOURCES_CONFIG_FILE_PATH, resource_config_content())
		.with_json_file(KEYS_FILE_PATH, generated_keys_file_content())
		.with_file(ECDSA_KEY_PATH, ECDSA_KEY_FILE_CONTENT)
		.with_expected_io(
			vec![
				intro_io(),
				load_base_path_io(),
				node_url_prompt_io(),
				rotate_io(),
				vec![
					MockIO::prompt_yes_no(
						"keys file partner-chains-public-keys.json exists - overwrite it?",
						false,
						false,
					),
					MockIO::eprint(
						"Refusing to overwrite keys file - aborting. Please note that the rotated keys have already been added to the node keystore.",
					),
				],
			]
			.into_iter()
			.flatten()
			.collect::<Vec<MockIO>>(),
		);

	let result = rotate_keys_cmd(None, None, None).run(&mock_context);
	result.expect_err("should return error");
	// keys file is left untouched
	verify_json!(mock_context, KEYS_FILE_PATH, generated_keys_file_content());
}

mod candidate_key_params_from_decoded {
	use super::*;

	#[test]
	fn filters_out_the_cross_chain_key_and_sorts_by_key_type() {
		let keys = candidate_key_params_from_decoded(vec![
			(KeyTypeId(*b"gran"), [2u8; 32].to_vec()),
			(KeyTypeId(*b"crch"), [9u8; 33].to_vec()),
			(KeyTypeId(*b"aura"), [1u8; 32].to_vec()),
		])
		.expect("should succeed");
		assert_eq!(
			keys.iter().map(CandidateKeyParam::to_string).collect::<Vec<_>>(),
			vec![format!("aura:{}", "01".repeat(32)), format!("gran:{}", "02".repeat(32)),]
		);
	}

	#[test]
	fn reports_error_for_non_utf8_key_type_id() {
		let result =
			candidate_key_params_from_decoded(vec![(KeyTypeId([0xff, 0xfe, 0x00, 0x01]), vec![0])]);
		let error = result.expect_err("should return error");
		assert!(error.to_string().contains("not valid UTF-8"));
	}

	#[test]
	fn reports_error_for_duplicate_key_type_id() {
		let result = candidate_key_params_from_decoded(vec![
			(KeyTypeId(*b"aura"), [1u8; 32].to_vec()),
			(KeyTypeId(*b"aura"), [2u8; 32].to_vec()),
		]);
		let error = result.expect_err("should return error");
		assert!(error.to_string().contains("Duplicate key type id"));
	}
}

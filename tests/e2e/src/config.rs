use whisky::csl::NetworkInfo as CardanoNetworkInfo;
use whisky::{LanguageVersion, Network as CardanoNetwork};

fn runtime_values_dir() -> String {
    std::env::var("RUNTIME_VALUES_DIR").unwrap_or_else(|_| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        format!("{manifest_dir}/../../local-environment/src/networks/local-env/runtime-values")
    })
}

fn read_contracts_info_entry(name: &str) -> serde_json::Value {
    let path = format!("{}/contracts-info.json", runtime_values_dir());
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read contracts-info.json at {path}: {e}"));
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(&content).expect("Failed to parse contracts-info.json");
    entries
        .into_iter()
        .find(|entry| entry["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("{name} not found in contracts-info.json"))
}

fn read_plutus_compiled_code(title: &str) -> String {
    let path = format!("{}/plutus-local.json", runtime_values_dir());
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read plutus-local.json at {path}: {e}"));
    let blueprint: serde_json::Value =
        serde_json::from_str(&content).expect("Failed to parse plutus-local.json");
    blueprint["validators"]
        .as_array()
        .expect("validators should be an array")
        .iter()
        .find(|v| v["title"].as_str() == Some(title))
        .unwrap_or_else(|| panic!("{title} not found in plutus-local.json"))["compiledCode"]
        .as_str()
        .expect("compiledCode should be a string")
        .to_string()
}

#[derive(Clone)]
pub struct Settings {
    pub node_client: NodeClientSettings,
    pub ogmios_client: OgmiosClientSettings,
    pub constants: Constants,
}

impl Settings {
    pub fn new() -> Self {
        {
            let network_info = CardanoNetworkInfo::testnet_preview();
            Self {
                node_client: NodeClientSettings {
                    #[cfg(feature = "local-dev")]
                    base_url: "ws://127.0.0.1:9944".into(),

                    #[cfg(feature = "local")]
                    base_url: "ws://127.0.0.1:9933".into(),

                    #[cfg(feature = "local-ci")]
                    base_url: "ws://172.17.0.1:9933".into(),

                    #[cfg(feature = "qanet")]
                    base_url: "wss://rpc.qanet.dev.midnight.network".into(),
                },
                ogmios_client: OgmiosClientSettings {
                    #[cfg(any(feature = "local", feature = "local-dev"))]
                    base_url: "ws://127.0.0.1:1337".into(),
                    #[cfg(feature = "local-ci")]
                    base_url: "ws://172.17.0.1:1337".into(),
                    #[cfg(feature = "qanet")]
                    base_url: "wss://ogmios.qanet.dev.midnight.network".into(),
                    timeout_seconds: 180,
                    network: CardanoNetwork::Preview,
                    network_info,
                },
                constants: Constants {
                    payments: Payments {
                        funded_address:
                            "addr_test1vr5vxqpnpl3325cu4zw55tnapjqzzx78pdrnk8k5j7wl72c6y08nd".into(),
                        funded_address_skey_cbor:
                            "5820d0a6c5c921266d15dc8d1ce1e51a01e929a686ed3ec1a9be1145727c224bf386"
                                .into(),
                        funded_address_vkey_cbor:
                            "5820fc014cb5f071f5d6a36cb5a7e5f168c86555989445a23d4abec33d280f71aca4"
                                .into(),
                    },
                    cost_model: vec![
                        vec![
                            100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32,
                            201305, 8356, 4, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000,
                            100, 16000, 100, 100, 100, 16000, 100, 94375, 32, 132994, 32, 61462, 4,
                            72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 228465, 122, 0,
                            1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775,
                            558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049,
                            1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1,
                            44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32,
                            11546, 32, 85848, 228465, 122, 0, 1, 1, 90434, 519, 0, 1, 74433, 32,
                            85848, 228465, 122, 0, 1, 1, 85848, 228465, 122, 0, 1, 1, 270652,
                            22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420,
                            1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32,
                            24623, 32, 53384111, 14333, 10,
                        ],
                        vec![
                            100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32,
                            201305, 8356, 4, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000,
                            100, 16000, 100, 100, 100, 16000, 100, 94375, 32, 132994, 32, 61462, 4,
                            72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 228465, 122, 0,
                            1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775,
                            558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049,
                            1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1,
                            44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32,
                            11546, 32, 85848, 228465, 122, 0, 1, 1, 90434, 519, 0, 1, 74433, 32,
                            85848, 228465, 122, 0, 1, 1, 85848, 228465, 122, 0, 1, 1, 955506,
                            213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0,
                            141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588,
                            32, 20744, 32, 25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10,
                            43574283, 26308, 10, 100000, 100000, 100000, 100000, 100000, 100000,
                            100000, 100000, 100000, 100000,
                        ],
                        // Plutus V3 cost models (from local-environment genesis.conway.json)
                        vec![
                            100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32,
                            201305, 8356, 4, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000,
                            100, 16000, 100, 100, 100, 16000, 100, 94375, 32, 132994, 32, 61462, 4,
                            72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 123203, 7305,
                            -900, 1716, 549, 57, 85848, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498,
                            38, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895,
                            32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1,
                            28999, 74, 1, 43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32,
                            72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 123203, 7305, -900,
                            1716, 549, 57, 85848, 0, 1, 90434, 519, 0, 1, 74433, 32, 85848, 123203,
                            7305, -900, 1716, 549, 57, 85848, 0, 1, 1, 85848, 123203, 7305, -900,
                            1716, 549, 57, 85848, 0, 1, 955506, 213312, 0, 2, 270652, 22588, 4,
                            1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1,
                            81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32,
                            24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10,
                            16000, 100, 16000, 100, 962335, 18, 2780678, 6, 442008, 1, 52538055,
                            3756, 18, 267929, 18, 76433006, 8868, 18, 52948122, 18, 1995836, 36,
                            3227919, 12, 901022, 1, 166917843, 4307, 36, 284546, 36, 158221314,
                            26549, 36, 74698472, 36, 333849714, 1, 254006273, 72, 2174038, 72,
                            2261318, 64571, 4, 207616, 8310, 4, 1293828, 28716, 63, 0, 1, 1006041,
                            43623, 251, 0, 1,
                        ],
                    ],
                },
            }
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone)]
pub struct NodeClientSettings {
    pub base_url: String,
}

#[derive(Clone)]
pub struct OgmiosClientSettings {
    pub base_url: String,
    pub timeout_seconds: u64,
    pub network: CardanoNetwork,
    pub network_info: CardanoNetworkInfo,
}

#[derive(Clone)]
pub struct Constants {
    pub payments: Payments,
    pub cost_model: Vec<Vec<i64>>,
}
#[derive(Clone)]
pub struct Payments {
    pub funded_address: String,
    pub funded_address_skey_cbor: String,
    pub funded_address_vkey_cbor: String,
}
pub fn mapping_validator_address() -> String {
    let entry = read_contracts_info_entry("cNIGHT Generates Dust");
    entry["address"]
        .as_str()
        .expect("address should be a string")
        .to_string()
}

pub fn mapping_validator_policy_id() -> String {
    let entry = read_contracts_info_entry("cNIGHT Generates Dust");
    entry["scriptHash"]
        .as_str()
        .expect("scriptHash should be a string")
        .to_string()
}

pub fn mapping_validator_cbor_double_encoding() -> String {
    let cbor = read_plutus_compiled_code("cnight_generates_dust.cnight_generates_dust.else");
    whisky::apply_double_cbor_encoding(&cbor).expect("Failed to encode mapping_validator script")
}

pub fn council_forever_policy_id() -> String {
    let entry = read_contracts_info_entry("Council Forever");
    entry["scriptHash"]
        .as_str()
        .expect("scriptHash should be a string")
        .to_string()
}

pub fn council_forever_address() -> String {
    let entry = read_contracts_info_entry("Council Forever");
    entry["address"]
        .as_str()
        .expect("address should be a string")
        .to_string()
}

pub fn tech_auth_forever_policy_id() -> String {
    let entry = read_contracts_info_entry("Tech Auth Forever");
    entry["scriptHash"]
        .as_str()
        .expect("scriptHash should be a string")
        .to_string()
}

pub fn tech_auth_forever_address() -> String {
    let entry = read_contracts_info_entry("Tech Auth Forever");
    entry["address"]
        .as_str()
        .expect("address should be a string")
        .to_string()
}

pub fn federated_ops_forever_policy_id() -> String {
    let entry = read_contracts_info_entry("Federated Ops Forever");
    entry["scriptHash"]
        .as_str()
        .expect("scriptHash should be a string")
        .to_string()
}

pub fn federated_ops_forever_address() -> String {
    let entry = read_contracts_info_entry("Federated Ops Forever");
    entry["address"]
        .as_str()
        .expect("address should be a string")
        .to_string()
}

pub fn cnight_token_cbor_double_encoding() -> String {
    let cbor = read_plutus_compiled_code("test_cnight_no_audit.tcnight_mint_infinite.else");
    whisky::apply_double_cbor_encoding(&cbor).expect("Failed to encode cnight token script")
}

pub fn cnight_token_policy_id() -> String {
    let cbor_double_encoded = cnight_token_cbor_double_encoding();
    let script_hash = whisky::get_script_hash(&cbor_double_encoded, LanguageVersion::V3);
    script_hash.expect("Error calculating `cnight_token_policy_id`")
}

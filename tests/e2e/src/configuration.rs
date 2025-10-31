use serde::Deserialize;
use serde_aux::field_attributes::deserialize_number_from_string;
use std::fs;
use std::{
    io,
    path::{Path, PathBuf},
};
use whisky::{LanguageVersion, Network as CardanoNetwork};

#[derive(serde::Deserialize, Clone, Debug)]
pub struct Settings {
	pub node_client: NodeClientSettings,
	pub ogmios_client: OgmiosClientSettings,
	pub test_files: TestFiles, // TODO: these shouldn't be here, but let's keep it for now for the sake of this experiment
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct NodeClientSettings {
	pub base_url: String,
	#[serde(deserialize_with = "deserialize_midnight_network")]
	pub network: MidnightNetwork,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct OgmiosClientSettings {
	pub base_url: String,
	#[serde(deserialize_with = "deserialize_number_from_string")]
	pub timeout_seconds: u64,
	pub network: CardanoNetwork,
}

trait BaseFileReader {
	fn base_dir(&self) -> &Path;

	fn resolve(&self, filename: &str) -> PathBuf {
		std::env::current_dir()
			.expect("cannot get CWD")
			.join(self.base_dir())
			.join(filename)
	}

	fn read_file(&self, filename: &str) -> io::Result<String> {
		fs::read_to_string(self.resolve(filename)).map(|s| s.trim().to_string())
	}

	fn load_cbor(file_content: String) -> String {
		match serde_json::from_str::<serde_json::Value>(&file_content) {
			Ok(json) => json["cborHex"].as_str().expect("No cborHex in file").to_string(),
			Err(_) => panic!("File is not a valid JSON"),
		}
	}
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct TestFiles {
	pub payments: PaymentsFiles,
	pub policies: PoliciesFiles,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct PaymentsFiles {
	pub base_dir: PathBuf,
	pub addr_file: String,
	pub skey_file: String,
}

impl BaseFileReader for PaymentsFiles {
	fn base_dir(&self) -> &Path {
		&self.base_dir
	}
}
impl PaymentsFiles {
	pub fn addr(&self) -> io::Result<String> {
		self.read_file(&self.addr_file)
	}
	pub fn skey(&self) -> io::Result<String> {
		self.read_file(&self.skey_file)
	}
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct PoliciesFiles {
	pub base_dir: PathBuf,
	pub mapping_validator: String,
	pub auth_token: String,
	pub cnight_token: String,
}

impl BaseFileReader for PoliciesFiles {
	fn base_dir(&self) -> &Path {
		&self.base_dir
	}
}
impl PoliciesFiles {
	pub fn mapping_validator_address(&self, network_id: u8) -> String {
		let file_content = self.mapping_validator().expect("mapping validator file not readable");
		let script_hash = self.script_hash_from_content(file_content);
		whisky::script_to_address(network_id, &script_hash, None)
	}

	pub fn mapping_validator_cbor(&self) -> String {
		let file_content = self.mapping_validator().expect("mapping validator file not readable");
		PoliciesFiles::load_cbor(file_content)
	}

	pub fn auth_token_policy_id(&self) -> String {
		let file_content = self.auth_token().expect("auth token policy not set");
		self.script_hash_from_content(file_content)
	}

	pub fn cnight_token_policy_id(&self) -> String {
		let file_content = self.cnight_token().expect("cnight token not set");
		self.script_hash_from_content(file_content)
	}

	pub fn auth_token_policy(&self) -> String {
		let file_content = self.auth_token().expect("auth token mint not set");
		PoliciesFiles::load_cbor(file_content)
	}

	pub fn cnight_token_policy(&self) -> String {
		let file_content = self.cnight_token().expect("cnight_token token mint not set");
		PoliciesFiles::load_cbor(file_content)
	}

	fn script_hash_from_content(&self, file_content: String) -> String {
		let cbor_hex = PoliciesFiles::load_cbor(file_content);
		whisky::get_script_hash(&cbor_hex, LanguageVersion::V2).unwrap()
	}

	fn mapping_validator(&self) -> io::Result<String> {
		self.read_file(&self.mapping_validator)
	}
	fn auth_token(&self) -> io::Result<String> {
		self.read_file(&self.auth_token)
	}
	fn cnight_token(&self) -> io::Result<String> {
		self.read_file(&self.cnight_token)
	}
}

#[derive(Clone, Debug)]
pub enum MidnightNetwork {
	Local,
	Devnet,
}

impl MidnightNetwork {
	pub fn as_str(&self) -> &'static str {
		match self {
			MidnightNetwork::Local => "local",
			MidnightNetwork::Devnet => "devnet",
		}
	}
}

impl TryFrom<String> for MidnightNetwork {
	type Error = String;
	fn try_from(value: String) -> Result<Self, Self::Error> {
		match value.to_lowercase().as_str() {
			"local" => Ok(Self::Local),
			"devnet" => Ok(Self::Devnet),
			other => {
				Err(format!("{} is not a valid environment. Use either `local` or `devnet`", other))
			},
		}
	}
}

fn deserialize_midnight_network<'de, D>(deserializer: D) -> Result<MidnightNetwork, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s = String::deserialize(deserializer)?;
	MidnightNetwork::try_from(s).map_err(serde::de::Error::custom)
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
	let base_path = std::env::current_dir().expect("Failed to determine current working directory");
	let configuration_directory = base_path.join("configuration");

	let environment: MidnightNetwork = std::env::var("ENV")
		.unwrap_or_else(|_| "local".into())
		.try_into()
		.expect("Failed to parse ENV environment variable.");

	let environment_filename = format!("{}.yaml", environment.as_str());
	let settings = config::Config::builder()
		.add_source(config::File::from(configuration_directory.join("base.yaml")))
		.add_source(config::File::from(configuration_directory.join(environment_filename)))
		.build()?;

	settings.try_deserialize::<Settings>()
}

pub fn local_env_cost_models() -> Vec<Vec<i64>> {
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
	]
}

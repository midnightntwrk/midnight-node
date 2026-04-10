#![allow(dead_code)]

use std::path::{Path, PathBuf};

use midnight_node_toolkit::{
	cli_parsers,
	commands::{
		contract_address::{self, ContractAddressArgs},
		contract_state::{self, ContractStateArgs},
		generate_intent::{
			self, CircuitCommandArgs, DeployCommandArgs, GenerateIntentArgs, JsCommand,
		},
		generate_txs::{self, GenerateTxsArgs},
		send_intent::{self, SendIntentArgs},
		show_address::{self, ShowAddress, ShowAddressArgs, SpecificAddressTypeArgs},
		show_transaction::{self, ShowTransactionArgs},
	},
	toolkit_js::{CircuitArgs, DeployArgs, RelativePath, ToolkitJs},
	tx_generator::{
		builder::{Builder, CustomContractArgs},
		destination::Destination,
		source::{FetchCacheConfig, Source},
	},
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const DEFAULT_FETCH_CONCURRENCY: usize = 1;
const COMPACTC_VERSION_FILE: &str = "../../COMPACTC_VERSION";
const TOOLKIT_JS_RELATIVE_PATH: &str = "../toolkit-js";
const CONTRACTS_RELATIVE_PATH: &str = "tests/contracts";
const DEFAULT_NETWORK: &str = "undeployed";
const COMPILE_CACHE_DIR: &str = "midnight-toolkit-contract-cache";

pub struct DeployOutput {
	pub intent: PathBuf,
	pub private_state: PathBuf,
	pub zswap_state: PathBuf,
}

pub struct CircuitOutput {
	pub intent: PathBuf,
	pub private_state: PathBuf,
	pub zswap_state: PathBuf,
}

pub struct CircuitCall<'a> {
	pub circuit_id: &'a str,
	pub call_args: &'a [&'a str],
}

pub struct ToolkitTestHelper {
	node_ws: String,
	toolkit_js_path: PathBuf,
	contracts_dir: PathBuf,
	network: String,
	pub work_dir: TempDir,
}

fn path_to_string(path: &Path) -> String {
	path.to_string_lossy().to_string()
}

fn default_source() -> Source {
	Source {
		src_url: None,
		fetch_only_cached: false,
		fetch_concurrency: 0,
		fetch_compute_concurrency: None,
		src_files: None,
		dust_warp: true,
		ignore_block_context: false,
		fetch_cache: FetchCacheConfig::InMemory,
		ledger_state_db: String::new(),
	}
}

fn manifest_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn compile_cache_key(source: &str, compactc_version: &str) -> String {
	let mut hasher = Sha256::new();
	hasher.update(source.as_bytes());
	hasher.update(b"\0");
	hasher.update(compactc_version.as_bytes());
	hex::encode(hasher.finalize())
}

impl ToolkitTestHelper {
	pub fn new(node_ws: &str) -> Self {
		let base = manifest_dir();
		Self {
			node_ws: node_ws.to_string(),
			toolkit_js_path: std::env::var("TOOLKIT_JS_PATH")
				.map(PathBuf::from)
				.unwrap_or_else(|_| base.join(TOOLKIT_JS_RELATIVE_PATH)),
			contracts_dir: base.join(CONTRACTS_RELATIVE_PATH),
			network: DEFAULT_NETWORK.to_string(),
			work_dir: TempDir::new().expect("failed to create temp dir"),
		}
	}

	pub fn compactc_version() -> String {
		std::env::var("COMPACTC_VERSION").unwrap_or_else(|_| {
			let version_file = manifest_dir().join(COMPACTC_VERSION_FILE);
			std::fs::read_to_string(&version_file)
				.map(|s| s.trim().to_string())
				.unwrap_or_else(|e| panic!("failed to read {}: {e}", version_file.display()))
		})
	}

	pub fn prerequisites_ready(&self) -> bool {
		let compactc_version = Self::compactc_version();
		let required_paths = [
			self.toolkit_js_path.join("dist/bin.js"),
			self.toolkit_js_path.join("node_modules/@midnight-ntwrk/node-toolkit-v8"),
			self.toolkit_js_path.join(format!(
				"node_modules/@midnight-ntwrk/midnight-js-compact/managed/{compactc_version}"
			)),
		];

		if let Some(missing) = required_paths.iter().find(|path| !path.exists()) {
			eprintln!(
				"Skipping contract integration tests: missing {}\n\
                 Setup: cd util/toolkit-js && npm install && npm run build && \
                 npx fetch-compactc --version={compactc_version}",
				missing.display()
			);
			return false;
		}

		true
	}

	fn toolkit_js(&self) -> ToolkitJs {
		ToolkitJs { path: path_to_string(&self.toolkit_js_path) }
	}

	fn source_from_url(&self) -> Source {
		Source {
			src_url: Some(self.node_ws.clone()),
			fetch_concurrency: DEFAULT_FETCH_CONCURRENCY,
			..default_source()
		}
	}

	fn source_from_file(&self, file: &Path) -> Source {
		Source { src_files: Some(vec![path_to_string(file)]), ..default_source() }
	}

	fn dest_to_file(&self, file: &Path) -> Destination {
		Destination {
			dest_urls: vec![],
			rate: 0.0,
			dest_file: Some(path_to_string(file)),
			no_watch_progress: false,
		}
	}

	fn dest_to_url(&self) -> Destination {
		Destination {
			dest_urls: vec![self.node_ws.clone()],
			rate: 1.0,
			dest_file: None,
			no_watch_progress: false,
		}
	}

	pub fn load_contract_file(&self, name: &str) -> String {
		let path = self.contracts_dir.join(name);
		std::fs::read_to_string(&path)
			.unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
	}

	pub fn load_template(&self, name: &str, vars: &[(&str, &str)]) -> String {
		let mut content = self.load_contract_file(name);
		for (key, value) in vars {
			content = content.replace(&format!("{{{{{key}}}}}"), value);
		}
		content
	}

	pub async fn compile_contract(
		&self,
		compact_source: &str,
		name: &str,
	) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
		let compactc_version = Self::compactc_version();
		let cache_key = compile_cache_key(compact_source, &compactc_version);
		let cache_dir = std::env::temp_dir().join(COMPILE_CACHE_DIR).join(&cache_key);
		let cached_out = cache_dir.join("out");

		let work_contract_dir = self.work_dir.path().join(name);
		std::fs::create_dir_all(&work_contract_dir)?;
		std::os::unix::fs::symlink(
			self.toolkit_js_path.join("node_modules"),
			work_contract_dir.join("node_modules"),
		)?;
		std::os::unix::fs::symlink(&cached_out, work_contract_dir.join("out"))?;

		if cached_out.join("contract").exists() {
			return Ok(cached_out);
		}

		std::fs::create_dir_all(&cache_dir)?;
		let source_file = cache_dir.join(format!("{name}.compact"));
		std::fs::write(&source_file, compact_source)?;

		let output = tokio::process::Command::new("npx")
			.arg("run-compactc")
			.arg(&source_file)
			.arg(&cached_out)
			.env("COMPACTC_VERSION", &compactc_version)
			.current_dir(&self.toolkit_js_path)
			.output()
			.await?;

		if !output.status.success() {
			return Err(format!(
				"Contract compilation failed:\nstdout: {}\nstderr: {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr),
			)
			.into());
		}

		assert!(
			cached_out.join("contract").exists(),
			"Compilation succeeded but output directory not found"
		);

		Ok(cached_out)
	}

	pub fn write_config(&self, content: &str, name: &str) -> PathBuf {
		let path = self.work_dir.path().join(name);
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).expect("failed to create config parent dir");
		}
		std::fs::write(&path, content).expect("failed to write config file");
		path
	}

	pub fn show_address_coin_public(&self, seed: &str) -> String {
		let args = ShowAddressArgs {
			network: self.network.clone(),
			seed: cli_parsers::wallet_seed_decode(seed).expect("invalid wallet seed"),
			specific_address: SpecificAddressTypeArgs { coin_public: true, ..Default::default() },
		};
		match show_address::execute(args) {
			ShowAddress::SingleAddress(addr) => addr,
			ShowAddress::Addresses(_) => panic!("expected single address"),
		}
	}

	pub async fn generate_intent_deploy(
		&self,
		config_file: &Path,
		coin_public: &str,
	) -> Result<DeployOutput, Box<dyn std::error::Error + Send + Sync>> {
		let intent = self.work_dir.path().join("deploy_intent.bin");
		let private_state = self.work_dir.path().join("deploy_private_state.json");
		let zswap_state = self.work_dir.path().join("deploy_zswap_state.json");

		let args = GenerateIntentArgs {
			js_command: JsCommand::Deploy(DeployCommandArgs {
				toolkit_js: self.toolkit_js(),
				deploy: DeployArgs {
					config: RelativePath(config_file.to_path_buf()),
					network: self.network.clone(),
					coin_public: cli_parsers::coin_public_decode(coin_public)
						.expect("invalid coin public key"),
					authority_seed: None,
					output_intent: RelativePath(intent.clone()),
					output_private_state: RelativePath(private_state.clone()),
					output_zswap_state: RelativePath(zswap_state.clone()),
					constructor_args: vec![],
				},
				dry_run: false,
			}),
		};
		generate_intent::execute(args).await?;

		Ok(DeployOutput { intent, private_state, zswap_state })
	}

	pub async fn generate_intent_circuit(
		&self,
		config_file: &Path,
		coin_public: &str,
		onchain_state: &Path,
		private_state: &Path,
		contract_address: &str,
		call: CircuitCall<'_>,
	) -> Result<CircuitOutput, Box<dyn std::error::Error + Send + Sync>> {
		let CircuitCall { circuit_id, call_args } = call;
		let out_intent = self.work_dir.path().join(format!("{circuit_id}_intent.bin"));
		let out_private_state =
			self.work_dir.path().join(format!("{circuit_id}_private_state.json"));
		let out_zswap_state = self.work_dir.path().join(format!("{circuit_id}_zswap_state.json"));

		let args = GenerateIntentArgs {
			js_command: JsCommand::Circuit(CircuitCommandArgs {
				source: self.source_from_url(),
				wallet_seed: None,
				toolkit_js: self.toolkit_js(),
				circuit_call: CircuitArgs {
					config: RelativePath(config_file.to_path_buf()),
					contract_address: cli_parsers::contract_address_decode(contract_address)
						.expect("invalid contract address"),
					network: self.network.clone(),
					coin_public: cli_parsers::coin_public_decode(coin_public)
						.expect("invalid coin public key"),
					input_onchain_state: RelativePath(onchain_state.to_path_buf()),
					input_private_state: RelativePath(private_state.to_path_buf()),
					input_zswap_state: None,
					output_intent: RelativePath(out_intent.clone()),
					output_onchain_state: None,
					output_private_state: RelativePath(out_private_state.clone()),
					output_zswap_state: RelativePath(out_zswap_state.clone()),
					output_result: None,
					circuit_id: circuit_id.to_string(),
					call_args: call_args.iter().map(|s| s.to_string()).collect(),
				},
				custom_ledger_parameters: None,
				dry_run: false,
			}),
		};
		generate_intent::execute(args).await?;

		Ok(CircuitOutput {
			intent: out_intent,
			private_state: out_private_state,
			zswap_state: out_zswap_state,
		})
	}

	pub async fn send_intent(
		&self,
		intent_file: &Path,
		compiled_dir: &Path,
		funding_seed: &str,
		zswap_state_file: Option<&Path>,
	) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
		let output = self
			.work_dir
			.path()
			.join(intent_file.file_stem().unwrap().to_string_lossy().replace("_intent", "_tx.mn"));

		let args = SendIntentArgs {
			source: self.source_from_url(),
			destination: self.dest_to_file(&output),
			proof_server: None,
			contract_args: CustomContractArgs {
				funding_seed: funding_seed.to_string(),
				rng_seed: None,
				compiled_contract_dirs: vec![path_to_string(compiled_dir)],
				intent_files: vec![path_to_string(intent_file)],
				utxo_inputs: vec![],
				zswap_state_file: zswap_state_file.map(path_to_string),
				shielded_destinations: vec![],
			},
			dry_run: false,
		};
		send_intent::execute(args).await?;

		Ok(output)
	}

	pub async fn submit_tx(
		&self,
		src_file: &Path,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let args = GenerateTxsArgs {
			builder: Builder::Send,
			source: self.source_from_file(src_file),
			destination: self.dest_to_url(),
			proof_server: None,
			dry_run: false,
		};
		generate_txs::execute(args).await?;
		Ok(())
	}

	pub fn contract_address(
		&self,
		src_file: &Path,
	) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
		let args = ContractAddressArgs {
			tagged: false,
			untagged: false,
			src_file: path_to_string(src_file),
		};
		Ok(contract_address::execute(args)?)
	}

	pub async fn contract_state(
		&self,
		address: &str,
		dest_file: &Path,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let args = ContractStateArgs {
			source: self.source_from_url(),
			contract_address: cli_parsers::contract_address_decode(address)
				.expect("invalid contract address"),
			dest_file: Some(path_to_string(dest_file)),
			dry_run: false,
		};
		contract_state::execute(args).await
	}

	pub fn show_transaction(
		&self,
		src_file: &Path,
	) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
		let args = ShowTransactionArgs { src_file: path_to_string(src_file) };
		let result = show_transaction::execute(args)?;
		Ok(format!("{result}"))
	}

	pub fn assert_secret_not_in_tx(&self, tx_file: &Path, secret: &str, label: &str) {
		let tx_dump = self.show_transaction(tx_file).expect("show transaction failed");
		assert!(
			!tx_dump.to_lowercase().contains(secret),
			"PRIVACY BROKEN: secret key found in {label} transaction dump"
		);

		let raw_contents = std::fs::read_to_string(tx_file).expect("failed to read tx file");
		assert!(
			!raw_contents.to_lowercase().contains(secret),
			"PRIVACY BROKEN: secret key found in raw {label} .mn file"
		);
	}
}

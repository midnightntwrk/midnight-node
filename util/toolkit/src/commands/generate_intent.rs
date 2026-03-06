use crate::client::MidnightNodeClient;
use crate::toolkit_js;
use crate::toolkit_js::{EncodedZswapLocalState, RelativePath};
use crate::tx_generator::builder::build_fork_aware_context_raw;
use crate::tx_generator::source::Source;
use crate::{cli_parsers as cli, tx_generator::TxGenerator};
use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::{
	CoinPublicKey, DefaultDB, LedgerParameters, WalletSeed, WalletState, deserialize, serialize,
};
use std::io::Write;

#[derive(Subcommand)]
pub enum JsCommand {
	Deploy(DeployCommandArgs),
	Circuit(CircuitCommandArgs),
	MaintainContract(MaintainContractCommandArgs),
	MaintainCircuit(MaintainCircuitCommandArgs),
}

#[derive(Args, Debug)]
pub struct CircuitCommandArgs {
	#[command(flatten)]
	source: Source,

	/// Seed for the source wallet zswap state
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	wallet_seed: Option<WalletSeed>,

	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	circuit_call: toolkit_js::CircuitArgs,

	/// Custom serialized ledger parameters, otherwise the latest will be fetched.
	#[arg(long)]
	custom_ledger_parameters: Option<String>,

	/// Dry-run - don't generate intent, just print out settings
	#[arg(long, global = true)]
	dry_run: bool,
}

#[derive(Args, Debug)]
pub struct DeployCommandArgs {
	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	deploy: toolkit_js::DeployArgs,

	/// Dry-run - don't generate intent, just print out settings
	#[arg(long, global = true)]
	dry_run: bool,
}

#[derive(Args, Debug)]
pub struct MaintainContractCommandArgs {
	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	maintain: toolkit_js::MaintainContractArgs,

	/// Dry-run - don't generate intent, just print out settings
	#[arg(long, global = true)]
	dry_run: bool,
}

#[derive(Args, Debug)]
pub struct MaintainCircuitCommandArgs {
	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	maintain: toolkit_js::MaintainCircuitArgs,

	/// Dry-run - don't generate intent, just print out settings
	#[arg(long, global = true)]
	dry_run: bool,
}

#[derive(Args)]
pub struct GenerateIntentArgs {
	/// Supported commands
	#[clap(subcommand)]
	js_command: JsCommand,
}

pub async fn fetch_zswap_state(
	source: Source,
	wallet_seed: WalletSeed,
	coin_public: CoinPublicKey,
	dry_run: bool,
) -> Result<EncodedZswapLocalState, Box<dyn std::error::Error + Send + Sync>> {
	let source = TxGenerator::source(source, dry_run).await?;
	if dry_run {
		println!("Dry-run: fetching zswap state for wallet seed {:?}", wallet_seed);
		println!("Dry-run: attributing to coin-public {:?}", coin_public);
		return Ok(EncodedZswapLocalState::from_zswap_state(
			WalletState::<DefaultDB>::default(),
			coin_public,
		));
	}

	let received_tx = source.get_txs().await?;
	let fork_ctx = build_fork_aware_context_raw(&received_tx, &[wallet_seed]);

	Ok(fork_ctx.dispatch(
		|ctx| {
			let seed_v7 =
				crate::tx_generator::builder::builders::ledger_7::type_convert::convert_wallet_seed(
					wallet_seed,
				);
			let cpk_v7 =
				crate::tx_generator::builder::builders::ledger_7::type_convert::convert_coin_public_key(
					coin_public,
				);
			crate::commands::fork::ledger_7::generate_intent::fetch_zswap_state_from_context(
				&ctx, seed_v7, cpk_v7,
			)
		},
		|ctx| {
			crate::commands::fork::ledger_8::generate_intent::fetch_zswap_state_from_context(
				&ctx,
				wallet_seed,
				coin_public,
			)
		},
	))
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateIntentError {
	#[error("missing transaction source")]
	MissingSource,
	#[error("missing source url")]
	MissingSourceUrl,
	#[error("failed to create temporary dir for toolkit-js file interop")]
	FailedToCreateTempDir(std::io::Error),
	#[error("failed to decode ledger parameters: {0}")]
	DecodeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to deserialize ledger parameters: {0}")]
	DeserializeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
}

pub async fn execute(
	args: GenerateIntentArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	println!("Executing generate-intent");
	match args.js_command {
		JsCommand::Deploy(args) => {
			if args.dry_run {
				println!("Dry-run: toolkit-js path: {:?}", &args.toolkit_js.path);
				println!("Dry-run: generate deploy intent: {:?}", &args.deploy);
				return Ok(());
			}
			let command = toolkit_js::Command::Deploy(args.deploy);
			args.toolkit_js.execute(command)?;
		},
		JsCommand::Circuit(args) => {
			if args.dry_run {
				println!("Dry-run: toolkit-js path: {:?}", &args.toolkit_js.path);
				println!("Dry-run: generate circuit call intent: {:?}", &args.circuit_call);
			}

			let input_zswap_state = if let Some(wallet_seed) = args.wallet_seed {
				println!("getting input zswap...");
				let encoded_zswap_state = fetch_zswap_state(
					args.source.clone(),
					wallet_seed,
					args.circuit_call.coin_public,
					args.dry_run,
				)
				.await?;
				if args.dry_run {
					return Ok(());
				}
				let temp_dir =
					tempfile::tempdir().map_err(GenerateIntentError::FailedToCreateTempDir)?.keep();
				let (mut encoded_zswap_file, encoded_zswap_path) =
					tempfile::NamedTempFile::new_in(temp_dir)?.keep()?;
				serde_json::to_writer(&mut encoded_zswap_file, &encoded_zswap_state)?;
				Some(RelativePath(encoded_zswap_path))
			} else {
				None
			};
			if args.dry_run {
				return Ok(());
			}

			let ledger_parameters =
				if let Some(serialized_parameters) = args.custom_ledger_parameters {
					let bytes = hex::decode(&serialized_parameters.replace("0x", ""))
						.map_err(|e| GenerateIntentError::DecodeLedgerParameters(e.into()))?;
					let parameters: LedgerParameters = deserialize(&mut &bytes[..])
						.map_err(|e| GenerateIntentError::DeserializeLedgerParameters(e.into()))?;
					parameters
				} else {
					let Some(rpc_url) = args.source.src_url else {
						eprintln!("missing required --src-url argument");
						return Err(GenerateIntentError::MissingSourceUrl.into());
					};

					let client = MidnightNodeClient::new(&rpc_url, None).await?;
					client.get_ledger_parameters().await?
				};

			let temp_dir =
				tempfile::tempdir().map_err(GenerateIntentError::FailedToCreateTempDir)?.keep();
			let (mut encoded_parameters_file, encoded_parameters_path) =
				tempfile::NamedTempFile::new_in(temp_dir)?.keep()?;
			encoded_parameters_file
				.write_all(
					&serialize(&ledger_parameters).expect("Unable to serialize ledger parameters"),
				)
				.expect("failed to write file");
			let ledger_parameters_path = RelativePath(encoded_parameters_path);

			let command = toolkit_js::Command::Circuit {
				args: args.circuit_call,
				input_zswap_state,
				ledger_parameters: ledger_parameters_path,
			};
			args.toolkit_js.execute(command)?;
		},
		JsCommand::MaintainContract(args) => {
			if args.dry_run {
				println!("Dry-run: toolkit-js path: {:?}", &args.toolkit_js.path);
				println!("Dry-run: generate maintain contract intent: {:?}", &args.maintain);
				return Ok(());
			}
			let command =
				toolkit_js::Command::Maintain(toolkit_js::MaintainCommand::Contract(args.maintain));
			args.toolkit_js.execute(command)?;
		},
		JsCommand::MaintainCircuit(args) => {
			if args.dry_run {
				println!("Dry-run: toolkit-js path: {:?}", &args.toolkit_js.path);
				println!("Dry-run: generate maintain circuit intent: {:?}", &args.maintain);
				return Ok(());
			}
			let command =
				toolkit_js::Command::Maintain(toolkit_js::MaintainCommand::Circuit(args.maintain));
			args.toolkit_js.execute(command)?;
		},
	};
	Ok(())
}

/// Make sure to build toolkit-js before running these tests - this can be done with the earthly
/// target:
/// $ earthly +toolkit-js-prep-local
///
/// Test data is checked-in - to re-generate it, run:
/// $ earthly -P +rebuild-genesis-state-undeployed
#[cfg(test)]
mod test {
	use midnight_node_ledger_helpers::{INITIAL_PARAMETERS, Serializable, SigningKey, serialize};
	use std::path::PathBuf;

	use crate::cli::{Cli, run_command};
	use clap::Parser;

	use std::fs;

	fn to_hex<S: Serializable>(value: &S) -> String {
		let mut bytes = vec![];
		value.serialize(&mut bytes).unwrap();
		hex::encode(&bytes)
	}

	fn toolkit_js_prerequisites_ready() -> bool {
		let toolkit_js_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../toolkit-js");
		let required_paths = [
			toolkit_js_dir.join("dist/bin.js"),
			toolkit_js_dir.join("test/contract/managed/counter"),
			toolkit_js_dir.join("node_modules/@tsconfig/node24/tsconfig.json"),
		];

		if let Some(missing) = required_paths.iter().find(|path| !path.exists()) {
			eprintln!("Skipping generate-intent toolkit-js tests: missing {}", missing.display());
			return false;
		}

		true
	}

	#[tokio::test]
	async fn test_generate_deploy() {
		if !toolkit_js_prerequisites_ready() {
			return;
		}

		// as this is inside util/toolkit, the current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();
		let output_private_state = out_dir.path().join("state.json").to_string_lossy().to_string();
		let output_zswap_state = out_dir.path().join("zswap.json").to_string_lossy().to_string();

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"deploy",
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			"--output-intent",
			&output_intent,
			"--output-private-state",
			&output_private_state,
			"--output-zswap-state",
			&output_zswap_state,
			"0",
		];
		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
		assert!(fs::exists(&output_private_state).unwrap());
		assert!(fs::exists(&output_zswap_state).unwrap());
	}

	#[tokio::test]
	async fn test_generate_circuit_call() {
		if !toolkit_js_prerequisites_ready() {
			return;
		}

		// as this is inside util/toolkit, the current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();
		let output_private_state = out_dir.path().join("state.json").to_string_lossy().to_string();
		let output_zswap_state = out_dir.path().join("zswap.json").to_string_lossy().to_string();
		let output_result = out_dir.path().join("output.json").to_string_lossy().to_string();

		let contract_address_hex =
			std::fs::read_to_string("./test-data/contract/counter/contract_address.mn")
				.unwrap()
				.trim()
				.to_string();
		let custom_ledger_parameters = hex::encode(serialize(&INITIAL_PARAMETERS).unwrap()); //to_hex(&INITIAL_PARAMETERS);

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"circuit",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			//			"--src-files",
			//			"./test-data/genesis/genesis_block_undeployed.mn",
			//			"--wallet-seed",
			//			"0000000000000000000000000000000000000000000000000000000000000001",
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--input-onchain-state",
			"./test-data/contract/counter/contract_state.mn",
			"--input-private-state",
			"./test-data/contract/counter/initial_state.json",
			"--output-intent",
			&output_intent,
			"--output-private-state",
			&output_private_state,
			"--output-zswap-state",
			&output_zswap_state,
			"--output-result",
			&output_result,
			"--custom-ledger-parameters",
			&custom_ledger_parameters,
			"--contract-address",
			&contract_address_hex,
			"increment",
		];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
		assert!(fs::exists(&output_private_state).unwrap());
		assert!(fs::exists(&output_zswap_state).unwrap());
		assert!(fs::exists(&output_result).unwrap());
	}

	#[tokio::test]
	async fn test_generate_maintain_contract() {
		if !toolkit_js_prerequisites_ready() {
			return;
		}

		// as this is inside util/toolkit, the current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();

		let contract_address_hex =
			std::fs::read_to_string("./test-data/contract/counter/contract_address.mn")
				.unwrap()
				.trim()
				.to_string();

		let signing_key = SigningKey::sample(rand::thread_rng());
		let signing_key_hex = to_hex(&signing_key);

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"maintain-contract",
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			"--input-onchain-state",
			"./test-data/contract/counter/contract_state.mn",
			"--output-intent",
			&output_intent,
			"--contract-address",
			&contract_address_hex,
			"--signing",
			&signing_key_hex,
			&signing_key_hex,
		];
		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
	}

	#[tokio::test]
	#[ignore = "test failing intermittently - reason unknown"]
	async fn test_generate_maintain_circuit() {
		if !toolkit_js_prerequisites_ready() {
			return;
		}

		// as this is inside util/toolkit, current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();

		let contract_address_hex =
			std::fs::read_to_string("./test-data/contract/counter/contract_address.mn")
				.unwrap()
				.trim()
				.to_string();

		let signing_key = SigningKey::sample(rand::thread_rng());
		let signing_key_hex = to_hex(&signing_key);

		let verifier_path = "./test-data/contract/counter/keys/increment.verifier";

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"maintain-circuit",
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			"--input-onchain-state",
			"./test-data/contract/counter/contract_state.mn",
			"--output-intent",
			&output_intent,
			"--contract-address",
			&contract_address_hex,
			"--signing",
			&signing_key_hex,
			"increment",
			&verifier_path,
		];
		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
	}

	#[tokio::test]
	async fn test_generate_maintain_remove_circuit() {
		if !toolkit_js_prerequisites_ready() {
			return;
		}

		// as this is inside util/toolkit, the current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();

		let contract_address_hex =
			std::fs::read_to_string("./test-data/contract/counter/contract_address.mn")
				.unwrap()
				.trim()
				.to_string();

		let signing_key = SigningKey::sample(rand::thread_rng());
		let signing_key_hex = to_hex(&signing_key);

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"maintain-circuit",
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			"--input-onchain-state",
			"./test-data/contract/counter/contract_state.mn",
			"--output-intent",
			&output_intent,
			"--contract-address",
			&contract_address_hex,
			"--signing",
			&signing_key_hex,
			"increment",
		];
		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
	}
}

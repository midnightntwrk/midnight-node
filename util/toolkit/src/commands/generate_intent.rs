use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::{ContractAddress, NetworkId, deserialize};
use midnight_node_toolkit::cli_parsers as cli;
use std::{fs, path::PathBuf};

const BUILD_DIST: &str = "dist/bin.js";

#[derive(Subcommand)]
pub enum JsCommand {
	Deploy(DeployArgs),
	Circuit(CircuitArgs),
}

fn toolkit_path_parser(input: &str) -> Result<RelativeToolkitPath, clap::Error> {
	Ok(RelativeToolkitPath(input.to_string()))
}

#[derive(Debug, Clone)]
struct RelativeToolkitPath(String);
impl RelativeToolkitPath {
	fn absolute(&self) -> String {
		let input_path = std::path::PathBuf::from(&self.0);
		std::path::absolute(input_path)
			.expect("Failed to create absolute path")
			.to_string_lossy()
			.to_string()
	}
}

#[derive(Args)]
pub struct CircuitArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = toolkit_path_parser)]
	config: RelativeToolkitPath,

	/// Hex-encoded ledger-serialized address of the contract - this should include the network id header
	#[arg(long, short)]
	contract_address: String,

	/// Name of the circuit to invoke
	#[arg(long, short)]
	circuit_id: String,

	/// location of the toolkit-js.
	#[arg(long, short, env = "TOOLKIT_JS_PATH")]
	toolkit_js_path: String,

	/// The output file of the intent
	#[arg(long, short, value_parser = toolkit_path_parser)]
	output_intent: RelativeToolkitPath,

	/// The output file of the private state
	#[arg(long, short, value_parser = toolkit_path_parser)]
	output_private_state: RelativeToolkitPath,

	/// Target network
	#[arg(long, default_value = "undeployed", value_parser = cli::network_id_decode)]
	network: NetworkId,

	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long, value_parser = toolkit_path_parser)]
	coin_public: Option<RelativeToolkitPath>,

	/// Input file containing the current on-chain circuit state
	#[arg(long, short, value_parser = toolkit_path_parser)]
	input_onchain_state: RelativeToolkitPath,

	/// Input file containing the private circuit state
	#[arg(long, short, value_parser = toolkit_path_parser)]
	input_private_state: RelativeToolkitPath,

	/// Arguments to pass to the circuit
	circuit_args: Vec<String>,
}

#[derive(Args, Clone)]
pub struct DeployArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = toolkit_path_parser)]
	config: RelativeToolkitPath,

	/// location of the toolkit-js.
	#[arg(long, short, env = "TOOLKIT_JS_PATH")]
	toolkit_js_path: String,

	/// Target network
	#[arg(long, default_value = "undeployed", value_parser = cli::network_id_decode)]
	network: NetworkId,

	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long, value_parser = toolkit_path_parser)]
	coin_public: Option<RelativeToolkitPath>,

	/// A public BIP-340 signing key, hex encoded.
	#[arg(long, value_parser = toolkit_path_parser)]
	signing: Option<RelativeToolkitPath>,

	/// The output file of the intent
	#[arg(long, value_parser = toolkit_path_parser)]
	output_intent: RelativeToolkitPath,

	/// The output file of the private state
	#[arg(long, value_parser = toolkit_path_parser)]
	output_private_state: RelativeToolkitPath,
}

#[derive(Args)]
pub struct GenerateIntentArgs {
	/// Supported commands
	#[clap(subcommand)]
	js_command: JsCommand,
}

fn get_toolkit_js_cmd(toolkit_js_path: &str) -> String {
	println!("toolkit_js_path: {}", toolkit_js_path);
	if !fs::exists(&toolkit_js_path).expect("failed to read path {}") {
		panic!("toolkit-js is not ready. Please perform npm build.");
	}

	PathBuf::from(toolkit_js_path).join(BUILD_DIST).to_string_lossy().to_string()
}

fn encode_network_id(net_id: NetworkId) -> &'static str {
	match net_id {
		NetworkId::Undeployed => "undeployed",
		NetworkId::DevNet => "devnet",
		NetworkId::TestNet => "testnet",
		NetworkId::MainNet => "mainnet",
		_ => panic!("failed to encode unknown network id"),
	}
}

pub fn execute(args: GenerateIntentArgs) {
	println!("Executing generate-intent");

	match args.js_command {
		JsCommand::Deploy(args) => {
			let cmd = get_toolkit_js_cmd(&args.toolkit_js_path);
			let network_id = encode_network_id(args.network);
			println!("Executing deploy command");
			let config = args.config.absolute();
			let output_intent = args.output_intent.absolute();
			let output_private_state = args.output_private_state.absolute();
			let mut cmd_args = vec![
				"deploy",
				"-c",
				&config,
				"--output",
				&output_intent,
				"--output-ps",
				&output_private_state,
				"--network",
				network_id,
			];
			let coin_public = args.coin_public.map(|c| c.absolute());
			if let Some(ref coin_public) = coin_public {
				cmd_args.extend_from_slice(&["--coin-public", coin_public]);
			}
			let signing = args.signing.map(|s| s.absolute());
			if let Some(ref signing) = signing {
				cmd_args.extend_from_slice(&["--signing", signing]);
			}
			execute_command(&args.toolkit_js_path, &cmd, &cmd_args);

			println!("written:\n{}\n{}", &output_intent, &output_private_state);
		},
		JsCommand::Circuit(args) => {
			let cmd = get_toolkit_js_cmd(&args.toolkit_js_path);
			let network_id = encode_network_id(args.network);

			let contract_address: ContractAddress = deserialize(
				&mut &hex::decode(&args.contract_address)
					.expect("Error hex decoding ContractAddress")[..],
			)
			.expect("Failed deserializing ContractAddress");

			let contract_address_str = hex::encode(contract_address.0.0);
			println!("Executing circuit command");
			let config = args.config.absolute();
			let input_onchain_state = args.input_onchain_state.absolute();
			let input_private_state = args.input_private_state.absolute();
			let output_intent = args.output_intent.absolute();
			let output_private_state = args.output_private_state.absolute();
			let mut cmd_args = vec![
				"circuit",
				"-c",
				&config,
				"--network",
				network_id,
				"--output",
				&output_intent,
				"--output-ps",
				&output_private_state,
				"--state-file-path",
				&input_onchain_state,
				"--ps-state-file-path",
				&input_private_state,
			];
			let coin_public = args.coin_public.map(|c| c.absolute());
			if let Some(ref coin_public) = coin_public {
				cmd_args.extend_from_slice(&["--coin-public", &coin_public]);
			}
			// Add positional args
			cmd_args.extend_from_slice(&[&contract_address_str, &args.circuit_id]);
			cmd_args.extend(args.circuit_args.iter().map(|s| s.as_str()));
			execute_command(&args.toolkit_js_path, &cmd, &cmd_args);

			println!("written:\n{}\n{}", &output_intent, &output_private_state)
		},
	};
}

fn execute_command(working_dir: &str, cmd: &str, args: &[&str]) {
	println!("Executing {cmd} with arguments: {args:?}...");

	let cmd_out = |bytes: Vec<u8>| {
		let cmd_result = String::from_utf8(bytes).expect("failed to convert to string: {}");
		println!("{cmd_result}");
	};

	match std::process::Command::new(cmd).current_dir(working_dir).args(args).output() {
		Ok(output) => {
			if output.status.success() {
				cmd_out(output.stdout);
			} else {
				cmd_out(output.stderr)
			}
		},
		Err(e) => println!("{cmd} failed: {e:?}"),
	}
}

/// Make sure to build toolkit-js before running these tests - this can be done with the earthly
/// target:
/// $ earthly --secret GITHUB_TOKEN=<github-token-here> +toolkit-js-prep-local
///
/// Test data is checked-in - to re-generate it, run:
/// $ earthly -P +rebuild-genesis-state-undeployed
#[cfg(test)]
mod test {
	use crate::commands::generate_intent::{CircuitArgs, DeployArgs};

	use super::{GenerateIntentArgs, JsCommand, RelativeToolkitPath, execute};
	use std::fs;

	#[test]
	fn test_generate_deploy() {
		// as this is inside util/toolkit, current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin");
		let output_private_state = out_dir.path().join("state.json");

		let args = GenerateIntentArgs {
			js_command: JsCommand::Deploy(DeployArgs {
				config: RelativeToolkitPath(config),
				toolkit_js_path,
				output_intent: RelativeToolkitPath(output_intent.to_string_lossy().to_string()),
				output_private_state: RelativeToolkitPath(
					output_private_state.to_string_lossy().to_string(),
				),
				coin_public: None,
				network: midnight_node_ledger_helpers::NetworkId::Undeployed,
				signing: None,
			}),
		};
		execute(args);

		assert!(fs::exists(&output_intent).unwrap());
		assert!(fs::exists(&output_private_state).unwrap());
	}

	#[test]
	fn test_generate_circuit_call() {
		// as this is inside util/toolkit, current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin");
		let output_private_state = out_dir.path().join("state.json");

		let contract_address =
			std::fs::read_to_string("./test-data/contract/counter/contract_address.mn")
				.unwrap()
				.trim()
				.to_string();

		let args = GenerateIntentArgs {
			js_command: JsCommand::Circuit(CircuitArgs {
				config: RelativeToolkitPath(config),
				toolkit_js_path,
				output_intent: RelativeToolkitPath(output_intent.to_string_lossy().to_string()),
				output_private_state: RelativeToolkitPath(
					output_private_state.to_string_lossy().to_string(),
				),
				coin_public: None,
				network: midnight_node_ledger_helpers::NetworkId::Undeployed,
				contract_address,
				circuit_id: "increment".to_string(),
				input_onchain_state: RelativeToolkitPath(
					"./test-data/contract/counter/contract_state.mn".to_string(),
				),
				input_private_state: RelativeToolkitPath(
					"./test-data/contract/counter/initial_state.json".to_string(),
				),
				circuit_args: Vec::new(),
			}),
		};
		execute(args);

		assert!(fs::exists(&output_intent).unwrap());
		assert!(fs::exists(&output_private_state).unwrap());
	}
}

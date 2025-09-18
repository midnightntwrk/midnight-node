use std::path::PathBuf;

use clap::{Args, value_parser};
use midnight_node_ledger_helpers::{CoinPublicKey, ContractAddress, DB, NetworkId, WalletState};
mod encoded_zswap_local_state;
pub use encoded_zswap_local_state::EncodedZswapLocalState;

use crate::cli_parsers as cli;

const BUILD_DIST: &str = "dist/bin.js";

#[derive(Args)]
pub struct ToolkitJs {
	/// location of the toolkit-js.
	#[arg(long = "toolkit-js-path", env = "TOOLKIT_JS_PATH")]
	path: String,
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

/// Adds some protection against accidentally passing relative types to toolkit-js
#[derive(Clone, Debug)]
pub struct RelativePath(pub PathBuf);
impl RelativePath {
	fn absolute(&self) -> String {
		let input_path = std::path::PathBuf::from(&self.0);
		std::path::absolute(input_path)
			.expect("Failed to create absolute path")
			.to_string_lossy()
			.to_string()
	}
}

impl core::fmt::Display for RelativePath {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0.display())
	}
}

impl From<PathBuf> for RelativePath {
	fn from(value: PathBuf) -> Self {
		Self(value)
	}
}

pub enum Command {
	Deploy(DeployArgs),
	Circuit { args: CircuitArgs, input_zswap_state: Option<RelativePath> },
}

#[derive(Args)]
pub struct CircuitArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = value_parser!(PathBuf))]
	config: RelativePath,
	/// Hex-encoded ledger-serialized address of the contract - this should include the network id header
	#[arg(long, short = 'a', value_parser = cli::hex_ledger_serialize_decode::<ContractAddress>)]
	contract_address: ContractAddress,
	/// Target network
	#[arg(long, default_value = "undeployed", value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long, value_parser = cli::hex_ledger_serialize_decode::<CoinPublicKey>)]
	pub coin_public: CoinPublicKey,
	/// Input file containing the current on-chain circuit state
	#[arg(long, value_parser = value_parser!(PathBuf))]
	input_onchain_state: RelativePath,
	/// Input file containing the private circuit state
	#[arg(long, value_parser = value_parser!(PathBuf))]
	input_private_state: RelativePath,
	/// The output file of the intent
	#[arg(long, value_parser = value_parser!(PathBuf))]
	output_intent: RelativePath,
	/// The output file of the private state
	#[arg(long, value_parser = value_parser!(PathBuf))]
	output_private_state: RelativePath,
	/// A file path of where the generated 'ZswapLocalState' data should be written.
	#[arg(long, value_parser = value_parser!(PathBuf))]
	output_zswap_state: RelativePath,
	/// Name of the circuit to invoke
	circuit_id: String,
	/// Arguments to pass to the circuit
	circuit_args: Vec<String>,
}

#[derive(Args)]
pub struct DeployArgs {
	/// a user-defined config.ts file of the contract. See toolkit-js for the example.
	#[arg(long, short, value_parser = value_parser!(PathBuf))]
	config: RelativePath,
	/// Target network
	#[arg(long, default_value = "undeployed", value_parser = cli::network_id_decode)]
	network: NetworkId,
	/// A user public key capable of receiving Zswap coins, hex or Bech32m encoded.
	#[arg(long)]
	coin_public: Option<String>,
	/// A public BIP-340 signing key, hex encoded.
	#[arg(long)]
	signing: Option<String>,
	/// The output file of the intent
	#[arg(long, value_parser = value_parser!(PathBuf))]
	output_intent: RelativePath,
	/// The output file of the private state
	#[arg(long, value_parser = value_parser!(PathBuf))]
	output_private_state: RelativePath,
	/// A file path of where the generated 'ZswapLocalState' data should be written.
	#[arg(long, value_parser = value_parser!(PathBuf))]
	output_zswap_state: RelativePath,
}

#[derive(thiserror::Error, Debug)]
pub enum ToolkitJsError {
	#[error("failed to execute toolkit-js")]
	ExecutionError(std::io::Error),
}

impl ToolkitJs {
	pub fn execute(&self, cmd: Command) -> Result<(), ToolkitJsError> {
		match cmd {
			Command::Deploy(args) => self.execute_deploy(args),
			Command::Circuit { args, input_zswap_state } => {
				self.execute_ciruit(args, input_zswap_state)
			},
		}
	}

	pub fn execute_deploy(&self, args: DeployArgs) -> Result<(), ToolkitJsError> {
		let network_id = encode_network_id(args.network);
		println!("Executing deploy command");
		let config = args.config.absolute();
		let output_intent = args.output_intent.absolute();
		let output_private_state = args.output_private_state.absolute();
		let mut cmd_args = vec![
			"deploy",
			"-c",
			&config,
			"--network",
			network_id,
			"--output",
			&output_intent,
			"--output-ps",
			&output_private_state,
		];
		if let Some(ref coin_public) = args.coin_public {
			cmd_args.extend_from_slice(&["--coin-public", coin_public]);
		}
		if let Some(ref signing) = args.signing {
			cmd_args.extend_from_slice(&["--signing", signing]);
		}
		self.execute_js(&cmd_args)?;
		println!(
			"written: {}, {}, {}",
			args.output_intent, args.output_private_state, args.output_zswap_state
		);
		Ok(())
	}

	pub fn execute_ciruit(
		&self,
		args: CircuitArgs,
		input_zswap_state: Option<RelativePath>,
	) -> Result<(), ToolkitJsError> {
		let contract_address_str = hex::encode(args.contract_address.0.0);
		println!("Executing circuit command");
		let config = args.config.absolute();
		let input_onchain_state = args.input_onchain_state.absolute();
		let input_private_state = args.input_private_state.absolute();
		let output_intent = args.output_intent.absolute();
		let output_private_state = args.output_private_state.absolute();
		let output_zswap_state = args.output_zswap_state.absolute();
		let network_id = encode_network_id(args.network);
		let mut cmd_args = vec![
			"circuit",
			"-c",
			&config,
			"--network",
			network_id,
			"--coin-public",
			&args.coin_public,
			"--state-file-path",
			&input_onchain_state,
			"--ps-state-file-path",
			&input_private_state,
			"--output",
			&output_intent,
			"--output-ps",
			&output_private_state,
			"--output-zswap",
			&output_zswap_state,
		];
		let input_zswap_state = input_zswap_state.map(|s| s.absolute());
		if let Some(ref input_zswap_state) = input_zswap_state {
			cmd_args.extend_from_slice(&["--zswap-state-file-path", &input_zswap_state]);
		}
		// Add positional args
		cmd_args.extend_from_slice(&[&contract_address_str, &args.circuit_id]);
		cmd_args.extend(args.circuit_args.iter().map(|s| s.as_str()));
		self.execute_js(&cmd_args)?;
		println!(
			"written: {}, {}, {}",
			args.output_intent, args.output_private_state, args.output_zswap_state
		);
		Ok(())
	}

	fn execute_js(&self, args: &[&str]) -> Result<(), ToolkitJsError> {
		let cmd = PathBuf::from(&self.path).join(BUILD_DIST).to_string_lossy().to_string();
		println!("Executing {cmd} with arguments: {args:?}...");

		let output = std::process::Command::new(cmd)
			.current_dir(&self.path)
			.args(args)
			.output()
			.map_err(ToolkitJsError::ExecutionError)?;

		println!(
			"stdout: {}, stderr: {}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		Ok(())
	}
}

use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::{CoinPublicKey, LedgerContext, WalletSeed};
use midnight_node_toolkit::toolkit_js::{EncodedZswapLocalState, RelativePath};
use midnight_node_toolkit::tx_generator::source::Source;
use midnight_node_toolkit::{ProofType, SignatureType, toolkit_js};
use midnight_node_toolkit::{cli_parsers as cli, tx_generator::TxGenerator};

#[derive(Subcommand)]
pub enum JsCommand {
	Deploy(DeployArgs),
	Circuit(CircuitArgs),
}

#[derive(Args)]
pub struct SourceWallet {
	#[command(flatten)]
	source: Source,
	/// Seed for the source wallet zswap state
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	wallet_seed: WalletSeed,
}

#[derive(Args)]
pub struct CircuitArgs {
	#[command(flatten)]
	source_wallet: Option<SourceWallet>,

	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	circuit_call: toolkit_js::CircuitArgs,
}

#[derive(Args)]
pub struct DeployArgs {
	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	deploy: toolkit_js::DeployArgs,
}

#[derive(Args)]
pub struct GenerateIntentArgs {
	/// Supported commands
	#[clap(subcommand)]
	js_command: JsCommand,
}

pub async fn fetch_zswap_state(
	src: SourceWallet,
	coin_public: CoinPublicKey,
) -> Result<EncodedZswapLocalState, Box<dyn std::error::Error + Send + Sync>> {
	let source = TxGenerator::<SignatureType, ProofType>::source(src.source).await?;
	let received_tx = source.get_txs().await?;
	let network_id = received_tx.network();
	let context = LedgerContext::new_from_wallet_seeds(network_id, &[src.wallet_seed]);
	for block in received_tx.blocks {
		context.update_from_block(block.transactions, block.context);
	}
	let wallet = context.wallet_from_seed(src.wallet_seed);
	let zswap_local_state = wallet.shielded.state;

	Ok(EncodedZswapLocalState::from_zswap_state(zswap_local_state, coin_public))
}

pub async fn execute(
	args: GenerateIntentArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	println!("Executing generate-intent");
	let temp_dir = tempfile::tempdir()?;

	match args.js_command {
		JsCommand::Deploy(args) => {
			let command = toolkit_js::Command::Deploy(args.deploy);
			args.toolkit_js.execute(command)?;
		},
		JsCommand::Circuit(args) => {
			let input_zswap_state = if let Some(src) = args.source_wallet {
				let encoded_zswap_state =
					fetch_zswap_state(src, args.circuit_call.coin_public).await?;
				let (mut encoded_zswap_file, encoded_zswap_path) =
					tempfile::NamedTempFile::new_in(temp_dir)?.keep()?;
				serde_json::to_writer(&mut encoded_zswap_file, &encoded_zswap_state)?;
				Some(RelativePath(encoded_zswap_path))
			} else {
				None
			};
			let command =
				toolkit_js::Command::Circuit { args: args.circuit_call, input_zswap_state };
			args.toolkit_js.execute(command)?;
		},
	};
	Ok(())
}

/// Make sure to build toolkit-js before running these tests - this can be done with the earthly
/// target:
/// $ earthly --secret GITHUB_TOKEN=<github-token-here> +toolkit-js-prep-local
///
/// Test data is checked-in - to re-generate it, run:
/// $ earthly --secret GITHUB_TOKEN=<github-token-here> +toolkit-generate-test-data
#[cfg(test)]
mod test {
	use clap::Parser as _;

	use crate::{Cli, run_command};

	use std::fs;

	#[tokio::test]
	async fn test_generate_deploy() {
		// as this is inside util/toolkit, current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();
		let output_private_state = out_dir.path().join("state.json").to_string_lossy().to_string();
		let output_zswap_state = out_dir.path().join("zswap.bin").to_string_lossy().to_string();

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"deploy",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			"--output_intent",
			&output_intent,
			"--output-private-state",
			&output_private_state,
			"--output-zswap-state",
			&output_zswap_state,
		];
		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
		assert!(fs::exists(&output_private_state).unwrap());
	}

	#[tokio::test]
	async fn test_generate_circuit_call() {
		// as this is inside util/toolkit, current dir should move a few directories up
		let toolkit_js_path = "../toolkit-js".to_string();
		let config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let out_dir = tempfile::tempdir().unwrap();

		let output_intent = out_dir.path().join("intent.bin").to_string_lossy().to_string();
		let output_private_state = out_dir.path().join("state.json").to_string_lossy().to_string();
		let output_zswap_state = out_dir.path().join("zswap.bin").to_string_lossy().to_string();

		let contract_address_hex =
			std::fs::read_to_string("./test-data/contract/counter/contract_address.mn")
				.unwrap()
				.trim()
				.to_string();

		let args = vec![
			"midnight-node-toolkit",
			"generate-intent",
			"circuit",
			"--toolkit-js-path",
			&toolkit_js_path,
			"--config",
			&config,
			"--input-onchain-state",
			"./test-data/contract/counter/contract_state.mn",
			"--input-private-state",
			"./test-data/contract/counter/initial_state.json",
			"--output_intent",
			&output_intent,
			"--output-private-state",
			&output_private_state,
			"--output-zswap-state",
			&output_zswap_state,
			"--contract_address",
			&contract_address_hex,
			"increment",
		];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_intent).unwrap());
		assert!(fs::exists(&output_private_state).unwrap());
	}
}

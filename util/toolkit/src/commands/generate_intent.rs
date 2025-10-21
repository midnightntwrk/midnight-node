use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::{
	CoinPublicKey, DefaultDB, LedgerContext, WalletSeed, WalletState,
};
use midnight_node_toolkit::toolkit_js::{EncodedZswapLocalState, RelativePath};
use midnight_node_toolkit::tx_generator::source::Source;
use midnight_node_toolkit::{ProofType, SignatureType, toolkit_js};
use midnight_node_toolkit::{cli_parsers as cli, tx_generator::TxGenerator};

#[derive(Subcommand)]
pub enum JsCommand {
	Deploy(DeployCommandArgs),
	Circuit(CircuitCommandArgs),
}

#[derive(Args, Debug)]
pub struct SourceWallet {
	#[command(flatten)]
	source: Option<Source>,
	/// Seed for the source wallet zswap state
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	wallet_seed: Option<WalletSeed>,
}

#[derive(Args, Debug)]
pub struct CircuitCommandArgs {
	#[command(flatten)]
	source_wallet: SourceWallet,

	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	circuit_call: toolkit_js::CircuitArgs,

	/// Dry-run - don't generate intent, just print out settings
	#[arg(long)]
	dry_run: bool,
}

#[derive(Args, Debug)]
pub struct DeployCommandArgs {
	#[command(flatten)]
	toolkit_js: toolkit_js::ToolkitJs,

	#[command(flatten)]
	deploy: toolkit_js::DeployArgs,

	/// Dry-run - don't generate intent, just print out settings
	#[arg(long)]
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
	let source = TxGenerator::<SignatureType, ProofType>::source(source, dry_run).await?;
	if dry_run {
		println!("Dry-run: fetching zswap state for wallet seed {:?}", wallet_seed);
		println!("Dry-run: attributing to coin-public {:?}", coin_public);
		return Ok(EncodedZswapLocalState::from_zswap_state(
			WalletState::<DefaultDB>::default(),
			coin_public,
		));
	}

	let received_tx = source.get_txs().await?;
	let network_id = received_tx.network();
	let context = LedgerContext::new_from_wallet_seeds(network_id, &[wallet_seed]);
	for block in received_tx.blocks {
		context.update_from_block(block.transactions, block.context, block.state_root.clone());
	}
	let wallet = context.wallet_from_seed(wallet_seed);
	let zswap_local_state = wallet.shielded.state;

	Ok(EncodedZswapLocalState::from_zswap_state(zswap_local_state, coin_public))
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateIntentError {
	#[error("missing transaction source")]
	MissingSource,
	#[error("failed to create temporary dir for toolkit-js file interop")]
	FailedToCreateTempDir(std::io::Error),
}

pub async fn execute(
	args: GenerateIntentArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	println!("Executing generate-intent");
	let temp_dir = tempfile::tempdir().map_err(GenerateIntentError::FailedToCreateTempDir)?;

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
			let input_zswap_state = if args.source_wallet.wallet_seed.is_some() {
				let Some(source) = args.source_wallet.source else {
					println!("wallet_seed is present, but source is missing!");
					return Err(GenerateIntentError::MissingSource.into());
				};
				println!("getting input zswap...");
				let encoded_zswap_state = fetch_zswap_state(
					source,
					args.source_wallet.wallet_seed.unwrap(),
					args.circuit_call.coin_public,
					args.dry_run,
				)
				.await?;
				if args.dry_run {
					return Ok(());
				}
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
/// $ earthly -P +rebuild-genesis-state-undeployed
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
		// as this is inside util/toolkit, current dir should move a few directories up
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
}

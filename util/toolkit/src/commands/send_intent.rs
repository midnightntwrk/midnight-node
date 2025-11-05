use clap::Args;
use midnight_node_toolkit::{
	ProofType, SignatureType,
	tx_generator::{
		TxGenerator,
		builder::{Builder, CustomContractArgs},
		destination::Destination,
		source::Source,
	},
};

#[derive(Args)]
pub struct SendIntentArgs {
	#[command(flatten)]
	source: Source,
	#[command(flatten)]
	destination: Destination,
	// Proof Server Host
	#[arg(long, short)]
	proof_server: Option<String>,
	#[command(flatten)]
	contract_args: CustomContractArgs,
	/// Dry-run - don't generate any txs, just print out the settings
	#[arg(long)]
	dry_run: bool,
}

pub async fn execute(args: SendIntentArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let builder = Builder::ContractCustom(args.contract_args);

	let generator = TxGenerator::<SignatureType, ProofType>::new(
		args.source,
		args.destination,
		builder,
		args.proof_server,
		args.dry_run,
	)
	.await?;

	if args.dry_run {
		return Ok(());
	}

	let received_txs = generator.get_txs().await?;
	let generated_txs = generator.build_txs(&received_txs).await?;
	generator.send_txs(&generated_txs).await?;

	Ok(())
}

#[cfg(test)]
mod test {
	use crate::{Cli, run_command};
	use clap::Parser;
	use midnight_node_toolkit::cli_parsers::hex_str_decode;
	use midnight_node_toolkit::tx_generator::builder::{ContractDeployArgs, FUNDING_SEED};
	use std::fs;
	use tempfile::tempdir;

	use super::{CustomContractArgs, Destination, SendIntentArgs, Source, execute};
	#[tokio::test]
	async fn test_send_intent() {
		let rng_seed = "0000000000000000000000000000000000000000000000000000000000000037";
		let src_files = "../../res/genesis/genesis_block_undeployed.mn";
		let compiled_contract_dir = "../../static/contracts/simple-merkle-tree";

		let out_dir = tempdir().expect("failed to create tempdir");
		let out_dir_str = out_dir.path().to_string_lossy().to_string();

		let output_file = out_dir.path().join("output.mn").to_string_lossy().to_string();
		// generate deploy intent
		{
			let args = vec![
				"midnight-node-toolkit",
				"generate-sample-intent",
				"--src-file",
				src_files,
				"--dest-dir",
				&out_dir_str,
				"deploy",
				"--rng-seed",
				rng_seed,
			];
			let cli = Cli::parse_from(args);

			run_command(cli.command).await.expect("should work");
		}

		let intent_file: String = fs::read_dir(&out_dir)
			.expect("directory not found")
			.map(|p| p.unwrap().path().to_string_lossy().to_string())
			.take(1)
			.collect();

		let source = Source {
			src_url: None,
			fetch_concurrency: 0,
			src_files: Some(vec![src_files.to_string()]),
		};

		let destination = Destination {
			dest_urls: vec![],
			rate: 0.0,
			dest_file: Some(output_file.to_string()),
			to_bytes: false,
		};

		let rng_seed = hex_str_decode::<[u8; 32]>(rng_seed).expect("rng_seed failed");
		let info =
			ContractDeployArgs { funding_seed: FUNDING_SEED.to_string(), rng_seed: Some(rng_seed) };

		let contract_args = CustomContractArgs {
			info,
			compiled_contract_dirs: vec![compiled_contract_dir.to_string()],
			intent_files: vec![intent_file],
			utxo_inputs: vec![],
			zswap_state_file: None,
			shielded_destinations: vec![],
		};

		let args = SendIntentArgs {
			source,
			destination,
			proof_server: None,
			contract_args,
			dry_run: false,
		};

		execute(args).await.expect("should work during sending");
		assert!(fs::exists(output_file).expect("should_exist"));
	}

	#[tokio::test]
	async fn test_double_deploy() {
		let out_dir = tempfile::tempdir().unwrap();

		let toolkit_js_path = "../toolkit-js".to_string();
		let compiled_contract_dir_mint = format!("{toolkit_js_path}/mint/out");
		let compiled_contract_dir_counter =
			format!("{toolkit_js_path}/test/contract/managed/counter");

		let double_deploy_tx =
			out_dir.path().join("double_deploy_tx.mn").to_string_lossy().to_string();

		let args = vec![
			"midnight-node-toolkit",
			"send-intent",
			"--src-file",
			"../../res/genesis/genesis_block_undeployed.mn",
			"--intent-file",
			"./test-data/contract/mint/deploy.bin",
			"--intent-file",
			"./test-data/contract/counter/deploy.bin",
			"--compiled-contract-dir",
			&compiled_contract_dir_mint,
			"--compiled-contract-dir",
			&compiled_contract_dir_counter,
			"--to-bytes",
			"--dest-file",
			&double_deploy_tx,
		];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&double_deploy_tx).unwrap());

		// Generate counter intent

		let counter_config = format!("{toolkit_js_path}/test/contract/contract.config.ts");
		let counter_output_intent =
			out_dir.path().join("counter_intent.bin").to_string_lossy().to_string();
		let output_private_state =
			out_dir.path().join("counter_state.json").to_string_lossy().to_string();
		let output_zswap_state =
			out_dir.path().join("counter_zswap.json").to_string_lossy().to_string();
		let output_result =
			out_dir.path().join("counter_output.json").to_string_lossy().to_string();
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
			&counter_config,
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--input-onchain-state",
			"./test-data/contract/counter/contract_state.mn",
			"--input-private-state",
			"./test-data/contract/counter/initial_state.json",
			"--output-intent",
			&counter_output_intent,
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

		// Generate Mint intent
		let mint_config = format!("{toolkit_js_path}/mint/mint.config.ts");
		let mint_output_intent =
			out_dir.path().join("mint_intent.bin").to_string_lossy().to_string();
		let output_private_state =
			out_dir.path().join("mint_state.json").to_string_lossy().to_string();
		let output_zswap_state =
			out_dir.path().join("mint_zswap.json").to_string_lossy().to_string();
		let output_result = out_dir.path().join("mint_output.json").to_string_lossy().to_string();
		let contract_address_hex =
			std::fs::read_to_string("./test-data/contract/mint/contract_address.mn")
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
			&mint_config,
			"--coin-public",
			"aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98",
			"--input-onchain-state",
			"./test-data/contract/mint/contract_state.mn",
			"--input-private-state",
			"./test-data/contract/mint/initial_state.json",
			"--output-intent",
			&mint_output_intent,
			"--output-private-state",
			&output_private_state,
			"--output-zswap-state",
			&output_zswap_state,
			"--output-result",
			&output_result,
			"--contract-address",
			&contract_address_hex,
			"mint",
			"3337000000000000000000000000000000000000000000000000000000000000",
			"feeb000000000000000000000000000000000000000000000000000000000000",
			"1000",
		];
		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		let compiled_contract_dir_mint = format!("{toolkit_js_path}/mint/out");
		let compiled_contract_dir_counter =
			format!("{toolkit_js_path}/test/contract/managed/counter");

		let output_tx = out_dir.path().join("double_call_tx.mn").to_string_lossy().to_string();

		let args = vec![
			"midnight-node-toolkit",
			"send-intent",
			"--src-file",
			"../../res/genesis/genesis_block_undeployed.mn",
			"--src-file",
			&double_deploy_tx,
			"--intent-file",
			&mint_output_intent,
			"--intent-file",
			&counter_output_intent,
			"--compiled-contract-dir",
			&compiled_contract_dir_mint,
			"--compiled-contract-dir",
			&compiled_contract_dir_counter,
			"--to-bytes",
			"--dest-file",
			&output_tx,
		];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_tx).unwrap());
	}
}

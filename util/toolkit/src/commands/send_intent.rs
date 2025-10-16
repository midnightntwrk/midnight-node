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
			compiled_contract_dir: compiled_contract_dir.to_string(),
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
	#[ignore = "due to ledger bug PM-19672, this doesn't work yet"]
	async fn test_mint_tx() {
		let out_dir = tempfile::tempdir().unwrap();

		let toolkit_js_path = "../toolkit-js".to_string();
		let compiled_contract_dir = format!("{toolkit_js_path}/mint/out");
		let output_tx = out_dir.path().join("mint_tx.mn").to_string_lossy().to_string();

		let args = vec![
			"midnight-node-toolkit",
			"send-intent",
			"--src-file",
			"../../res/genesis/genesis_block_undeployed.mn",
			"./test-data/contract/mint/deploy_tx.mn",
			"--intent-file",
			"./test-data/contract/mint/mint.bin",
			"--zswap-state-file",
			"./test-data/contract/mint/mint_zswap.json",
			"--compiled-contract-dir",
			&compiled_contract_dir,
			"--to-bytes",
			"--dest-file",
			&output_tx,
		];

		let cli = Cli::parse_from(args);
		run_command(cli.command).await.expect("should work");

		assert!(fs::exists(&output_tx).unwrap());
	}
}

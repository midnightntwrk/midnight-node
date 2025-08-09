use clap::Args;
use midnight_node_toolkit::{
	ProofType, SignatureType,
	serde_def::DeserializedTransactionsWithContext,
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
	other_args: CustomContractArgs,
}

pub async fn execute(
	args: SendIntentArgs,
) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Box<dyn std::error::Error>>
{
	let builder = Builder::CustomContract(args.other_args);

	let generator = TxGenerator::<SignatureType, ProofType>::new(
		args.source,
		args.destination,
		builder,
		args.proof_server,
	)
	.await?;

	let received_txs = generator.get_txs().await?;
	let generated_txs = generator.build_txs(&received_txs).await?;
	generator.send_txs(&generated_txs).await?;

	Ok(generated_txs)
}

#[cfg(test)]
mod test {
	use crate::{Cli, run_command};
	use clap::Parser;
	use midnight_node_toolkit::cli_parsers::hex_str_decode;
	use midnight_node_toolkit::tx_generator::builder::{ContractDeployArgs, FUNDING_SEED};
	use std::fs;
	use std::fs::{remove_dir_all, remove_file};

	use super::{CustomContractArgs, Destination, SendIntentArgs, Source, execute};
	#[tokio::test]
	async fn test_send_intent() {
		let rng_seed = "0000000000000000000000000000000000000000000000000000000000000037";
		let src_files = "../../res/genesis/genesis_tx_undeployed.mn";
		let artifacts_dir = "../../static/contracts/simple-merkle-tree";

		let output_file = "output.mn";
		// generate deploy intent
		{
			let args = vec![
				"midnight-node-toolkit",
				"generate-intent",
				"--src-files",
				src_files,
				"--dest-file",
				output_file,
				"--dest-dir",
				"../../static/contracts/simple-merkle-tree/intents",
				"deploy",
				"--rng-seed",
				rng_seed,
			];
			let cli = Cli::parse_from(args);

			run_command(cli.command).await.expect("should work");
		}

		let source = Source {
			src_url: None,
			fetch_concurrency: 0,
			src_files: Some(vec![src_files.to_string()]),
		};

		let destination = Destination {
			dest_url: None,
			rate: 0.0,
			dest_file: Some(output_file.to_string()),
			to_bytes: false,
		};

		let rng_seed = hex_str_decode::<[u8; 32]>(rng_seed).expect("rng_seed failed");
		let info = ContractDeployArgs {
			funding_seed: FUNDING_SEED.to_string(),
			rng_seed: Some(rng_seed),
			fee: 0,
		};

		let other_args = CustomContractArgs { info, artifacts_dir: artifacts_dir.to_string() };

		let args = SendIntentArgs { source, destination, proof_server: None, other_args };

		execute(args).await.expect("should work during sending");
		assert!(fs::exists(output_file).expect("should_exist"));

		// remove intent files
		remove_dir_all(format!("{artifacts_dir}/intents"))
			.expect("failed to remove directory and files");
		remove_file(output_file).expect("failed to remove file");
	}
}

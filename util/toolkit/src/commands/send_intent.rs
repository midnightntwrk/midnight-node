use crate::tx_generator::{
	TxGenerator,
	builder::{Builder, CustomContractArgs},
	destination::Destination,
	source::Source,
};
use clap::Args;

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

	let generator =
		TxGenerator::new(args.source, args.destination, builder, args.proof_server, args.dry_run)
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
	use crate::cli::{Cli, run_command};
	use crate::cli_parsers::hex_str_decode;
	use crate::tx_generator::builder::FUNDING_SEED;
	use crate::tx_generator::source::FetchCacheConfig;
	use clap::Parser;
	use std::fs;
	use std::path::Path;
	use tempfile::tempdir;

	use super::{CustomContractArgs, Destination, SendIntentArgs, Source, execute};

	fn ledger_test_artifacts_ready() -> bool {
		let Ok(path) = std::env::var("MIDNIGHT_LEDGER_TEST_STATIC_DIR") else {
			eprintln!(
				"Skipping send-intent contract tests: MIDNIGHT_LEDGER_TEST_STATIC_DIR is not set"
			);
			return false;
		};
		if !Path::new(&path).exists() {
			eprintln!(
				"Skipping send-intent contract tests: MIDNIGHT_LEDGER_TEST_STATIC_DIR does not exist: {}",
				path
			);
			return false;
		}
		true
	}

	#[tokio::test]
	async fn test_send_intent() {
		if !ledger_test_artifacts_ready() {
			return;
		}

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
				"--dust-warp",
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
			fetch_compute_concurrency: None,
			src_files: Some(vec![src_files.to_string()]),
			dust_warp: true,
			ignore_block_context: false,
			fetch_only_cached: false,
			fetch_cache: FetchCacheConfig::InMemory,
			ledger_state_db: String::new(),
		};

		let destination = Destination {
			dest_urls: vec![],
			rate: 0.0,
			dest_file: Some(output_file.to_string()),
			no_watch_progress: false,
		};

		let rng_seed = hex_str_decode::<[u8; 32]>(rng_seed).expect("rng_seed failed");

		let contract_args = CustomContractArgs {
			funding_seed: FUNDING_SEED.to_string(),
			rng_seed: Some(rng_seed),
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
}

use clap::Args;
use midnight_node_toolkit::{
	ProofType, SignatureType,
	tx_generator::{
		TxGenerator,
		builder::{
			ContractCall, IntentToFile,
			builders::{ContractCallBuilder, ContractDeployBuilder, ContractMaintenanceBuilder},
		},
		source::Source,
	},
};

#[derive(Args)]
pub struct GenerateSampleIntentArgs {
	#[clap(subcommand)]
	pub contract_call: ContractCall,
	#[command(flatten)]
	pub source: Source,
	// Proof Server Host
	#[arg(long, short)]
	pub proof_server: Option<String>,
	// Directory to where the intent file will be saved
	#[arg(long)]
	pub dest_dir: String,
	/// Dry-run - don't generate any intents, just print out the settings
	#[arg(long)]
	pub dry_run: bool,
}

pub async fn execute(args: GenerateSampleIntentArgs) {
	println!("Generate a contract and save to file");

	let builder_and_contract_type: (Box<dyn IntentToFile + Send>, &str) =
		match args.contract_call.clone() {
			ContractCall::Deploy(args) => (Box::new(ContractDeployBuilder::new(args)), "deploy"),
			ContractCall::Call(args) => (Box::new(ContractCallBuilder::new(args)), "call"),
			ContractCall::Maintenance(args) => {
				(Box::new(ContractMaintenanceBuilder::new(args)), "maintenance")
			},
		};
	let mut builder = builder_and_contract_type.0;
	let partial_file_name = builder_and_contract_type.1;

	let source = TxGenerator::source(args.source, args.dry_run)
		.await
		.expect("failed to init tx source");
	let prover = TxGenerator::<SignatureType, ProofType>::prover(args.proof_server, args.dry_run);

	if args.dry_run {
		println!("Dry-run: generate intent for contract call {:?}", args.contract_call);
		println!("Dry-run: write files to directory {:?}", args.dest_dir);
		return ();
	}

	let received_txs = source.get_txs().await.expect("should receive txs");

	builder
		.generate_intent_file(received_txs, prover, &args.dest_dir, partial_file_name)
		.await;
}

#[cfg(test)]
mod test {
	use std::fs;
	use std::fs::remove_file;

	use midnight_node_toolkit::cli_parsers::hex_str_decode;
	use midnight_node_toolkit::tx_generator::builder::{ContractDeployArgs, FUNDING_SEED};

	use super::{ContractCall, GenerateSampleIntentArgs, Source, execute};

	#[tokio::test]
	async fn test_generate_sample_intent() {
		let rng_seed = "0000000000000000000000000000000000000000000000000000000000000037";
		let src_files = "../../res/genesis/genesis_block_undeployed.mn";

		let rng_seed = hex_str_decode::<[u8; 32]>(rng_seed).expect("rng_seed failed");
		let deploy_args =
			ContractDeployArgs { funding_seed: FUNDING_SEED.to_string(), rng_seed: Some(rng_seed) };

		let contract_call = ContractCall::Deploy(deploy_args);

		let source = Source {
			src_url: None,
			fetch_concurrency: 0,
			src_files: Some(vec![src_files.to_string()]),
		};

		let args = GenerateSampleIntentArgs {
			contract_call,
			source,
			proof_server: None,
			dest_dir: ".".to_string(),
			dry_run: false,
		};

		execute(args).await;

		let path = "1_deploy_intent.mn";

		let file_exist = fs::exists(path).expect("should return ok");
		assert!(file_exist);
		remove_file(path).expect("It should be removed"); // check that file was created
	}
}

use clap::Args;
use midnight_node_ledger_helpers::NetworkId;
use midnight_node_toolkit::{
	ProofType, SignatureType,
	tx_generator::{
		TxGenerator,
		builder::{
			Builder, ContractCall, IntentToFile,
			builders::{ContractCallBuilder, ContractDeployBuilder, ContractMaintenanceBuilder},
		},
		destination::Destination,
		source::Source,
	},
};

#[derive(Args)]
pub struct GenerateIntentArgs {
	#[clap(subcommand)]
	pub contract_call: ContractCall,
	#[command(flatten)]
	pub source: Source,
	#[command(flatten)]
	pub destination: Destination,
	// Proof Server Host
	#[arg(long, short)]
	pub proof_server: Option<String>,
	// Directory to where the intent file will be saved
	#[arg(long)]
	pub dest_dir: String,
}

pub async fn execute(args: GenerateIntentArgs) {
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

	let generator = TxGenerator::<SignatureType, ProofType>::new(
		args.source,
		args.destination,
		Builder::ContractCalls(args.contract_call),
		args.proof_server,
	)
	.await
	.expect("generator should work");

	let received_txs = generator.get_txs().await.expect("should receive txs");

	builder
		.generate_intent_file(
			received_txs,
			generator.prover.clone(),
			NetworkId::Undeployed,
			&args.dest_dir,
			partial_file_name,
		)
		.await;
}

#[cfg(test)]
mod test {
	use std::fs;
	use std::fs::remove_file;

	use midnight_node_toolkit::cli_parsers::hex_str_decode;
	use midnight_node_toolkit::tx_generator::builder::{ContractDeployArgs, FUNDING_SEED};

	use super::{ContractCall, Destination, GenerateIntentArgs, Source, execute};

	#[tokio::test]
	async fn test_generate_intent() {
		let rng_seed = "0000000000000000000000000000000000000000000000000000000000000037";
		let src_files = "../../res/genesis/genesis_tx_undeployed.mn";

		let rng_seed = hex_str_decode::<[u8; 32]>(rng_seed).expect("rng_seed failed");
		let deploy_args = ContractDeployArgs {
			funding_seed: FUNDING_SEED.to_string(),
			rng_seed: Some(rng_seed),
			fee: 0,
		};

		let contract_call = ContractCall::Deploy(deploy_args);

		let source = Source {
			src_url: None,
			fetch_concurrency: 0,
			src_files: Some(vec![src_files.to_string()]),
		};

		let destination = Destination {
			dest_url: None,
			rate: 0.0,
			dest_file: Some("send_intent.mn".to_string()),
			to_bytes: false,
		};

		let args = GenerateIntentArgs {
			contract_call,
			source,
			destination,
			proof_server: None,
			dest_dir: ".".to_string(),
		};

		execute(args).await;

		let path = "1_deploy_intent.mn";

		let file_exist = fs::exists(path).expect("should return ok");
		assert!(file_exist);
		remove_file(path).expect("It should be removed"); // check that file was created
	}
}

use crate::tx_generator::{
	TxGenerator,
	builder::{ContractCall, ProverConfig, build_fork_aware_context_cached},
	source::{Source, create_file_wallet_cache},
};
use clap::Args;
use midnight_node_ledger_helpers::fork::raw_block_data::LedgerVersion;
use std::sync::Arc;

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
	log::info!("Generate a contract and save to file");

	let ledger_state_db = args.source.ledger_state_db.clone();
	let fetch_cache = args.source.fetch_cache.clone();
	let source = TxGenerator::source(args.source, args.dry_run)
		.await
		.expect("failed to init tx source");
	let prover_config = TxGenerator::prover_config(args.proof_server, args.dry_run);

	if args.dry_run {
		log::info!("Dry-run: generate intent for contract call {:?}", args.contract_call);
		log::info!("Dry-run: write files to directory {:?}", args.dest_dir);
		return ();
	}

	let received_txs = source.get_txs().await.expect("should receive txs");
	let wallet_cache = create_file_wallet_cache(&ledger_state_db, &fetch_cache);

	// Build the context + prover, then construct the appropriate builder
	let funding_seed_str = match &args.contract_call {
		ContractCall::Deploy(a) => &a.funding_seed,
		ContractCall::Call(a) => &a.funding_seed,
		ContractCall::Maintenance(a) => &a.funding_seed,
	};
	let seeds =
		vec![midnight_node_ledger_helpers::Wallet::<midnight_node_ledger_helpers::DefaultDB>::wallet_seed_decode(funding_seed_str)];

	let fork_ctx =
		build_fork_aware_context_cached(&seeds, &received_txs, wallet_cache.as_deref()).await;
	let version = fork_ctx.version();

	if matches!(prover_config, ProverConfig::Remote(_)) {
		panic!("remote prover is not supported for intent generation");
	}

	match version {
		LedgerVersion::Ledger8 => {
			let context = Arc::new(fork_ctx.into_ledger8().expect("expected ledger 8 context"));
			let prover: Arc<
				dyn midnight_node_ledger_helpers::ProofProvider<
						midnight_node_ledger_helpers::DefaultDB,
					>,
			> = Arc::new(midnight_node_ledger_helpers::LocalProofServer::new());

			execute_with_builders_v8(args.contract_call, context, prover, &args.dest_dir).await;
		},
		LedgerVersion::Ledger7 => {
			let context = Arc::new(fork_ctx.into_ledger7().expect("expected ledger 7 context"));
			let prover: Arc<
				dyn midnight_node_ledger_helpers::ledger_7::ProofProvider<
						midnight_node_ledger_helpers::ledger_7::DefaultDB,
					>,
			> = Arc::new(midnight_node_ledger_helpers::ledger_7::LocalProofServer::new());

			execute_with_builders_v7(args.contract_call, context, prover, &args.dest_dir).await;
		},
	}
}

async fn execute_with_builders_v8(
	contract_call: ContractCall,
	context: Arc<
		midnight_node_ledger_helpers::context::LedgerContext<
			midnight_node_ledger_helpers::DefaultDB,
		>,
	>,
	prover: Arc<
		dyn midnight_node_ledger_helpers::ProofProvider<midnight_node_ledger_helpers::DefaultDB>,
	>,
	dest_dir: &str,
) {
	use crate::tx_generator::builder::builders::{
		ContractCallBuilder, ContractDeployBuilder, IntentToFile,
	};
	let (mut builder, partial_file_name): (Box<dyn IntentToFile + Send>, &str) = match contract_call
	{
		ContractCall::Deploy(a) => {
			(Box::new(ContractDeployBuilder::new(a, context, prover)), "deploy")
		},
		ContractCall::Call(a) => (Box::new(ContractCallBuilder::new(a, context, prover)), "call"),
		ContractCall::Maintenance(_) => unimplemented!("not implemented for Maintenance"),
	};

	builder
		.generate_intent_file(dest_dir, partial_file_name)
		.await
		.expect("failed to generate intent file");
}

async fn execute_with_builders_v7(
	contract_call: ContractCall,
	context: Arc<
		midnight_node_ledger_helpers::ledger_7::context::LedgerContext<
			midnight_node_ledger_helpers::ledger_7::DefaultDB,
		>,
	>,
	prover: Arc<
		dyn midnight_node_ledger_helpers::ledger_7::ProofProvider<
				midnight_node_ledger_helpers::ledger_7::DefaultDB,
			>,
	>,
	dest_dir: &str,
) {
	use crate::tx_generator::builder::builders::ledger_7::{
		ContractCallBuilder, ContractDeployBuilder, IntentToFile,
	};
	let (mut builder, partial_file_name): (Box<dyn IntentToFile + Send>, &str) = match contract_call
	{
		ContractCall::Deploy(a) => {
			(Box::new(ContractDeployBuilder::new(a, context, prover)), "deploy")
		},
		ContractCall::Call(a) => (Box::new(ContractCallBuilder::new(a, context, prover)), "call"),
		ContractCall::Maintenance(_) => unimplemented!("not implemented for Maintenance"),
	};

	builder
		.generate_intent_file(dest_dir, partial_file_name)
		.await
		.expect("failed to generate intent file");
}

#[cfg(test)]
mod test {
	use std::fs;
	use std::fs::remove_file;
	use std::path::Path;

	use crate::cli_parsers::hex_str_decode;
	use crate::tx_generator::builder::{ContractDeployArgs, FUNDING_SEED};
	use crate::tx_generator::source::FetchCacheConfig;

	use super::{ContractCall, GenerateSampleIntentArgs, Source, execute};

	fn ledger_test_artifacts_ready() -> bool {
		let Ok(path) = std::env::var("MIDNIGHT_LEDGER_TEST_STATIC_DIR") else {
			eprintln!("Skipping contract intent tests: MIDNIGHT_LEDGER_TEST_STATIC_DIR is not set");
			return false;
		};
		if !Path::new(&path).exists() {
			eprintln!(
				"Skipping contract intent tests: MIDNIGHT_LEDGER_TEST_STATIC_DIR does not exist: {}",
				path
			);
			return false;
		}
		true
	}

	#[tokio::test]
	async fn test_generate_sample_intent() {
		if !ledger_test_artifacts_ready() {
			return;
		}

		let rng_seed = "0000000000000000000000000000000000000000000000000000000000000037";
		let src_files = "../../res/genesis/genesis_block_undeployed.mn";

		let rng_seed = hex_str_decode::<[u8; 32]>(rng_seed).expect("rng_seed failed");
		let deploy_args = ContractDeployArgs {
			funding_seed: FUNDING_SEED.to_string(),
			authority_seeds: vec![],
			authority_threshold: None,
			rng_seed: Some(rng_seed),
		};

		let contract_call = ContractCall::Deploy(deploy_args);

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

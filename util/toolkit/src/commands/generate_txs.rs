use clap::Args;
use midnight_node_ledger_helpers::{ProofMarker, Signature};
use midnight_node_toolkit::{
	ProofType, SignatureType,
	serde_def::{DeserializedTransactionsWithContext, SourceTransactions},
	tx_generator::{
		TxGenerator, TxGeneratorError, builder::Builder, destination::Destination, source::Source,
	},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GenerateTxsError {
	#[error("failed to construct TxGenerator: {0}")]
	Generator(#[from] TxGeneratorError),
	#[error("failed to get transactions: {0}")]
	GetTransactions(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to build transactions: {0}")]
	BuildTransactions(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to build transactions: {0}")]
	SendTransactions(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Args)]
pub struct GenerateTxsArgs {
	#[clap(subcommand)]
	builder: Builder,
	#[command(flatten)]
	source: Source,
	#[command(flatten)]
	destination: Destination,
	// Proof Server Host
	#[arg(long, short, global = true)]
	proof_server: Option<String>,
	/// Dry-run - don't generate any txs, just print out the settings
	#[arg(long, global = true)]
	dry_run: bool,
}

pub async fn execute(args: GenerateTxsArgs) -> Result<(), GenerateTxsError> {
	let generator = TxGenerator::<SignatureType, ProofType>::new(
		args.source,
		args.destination,
		args.builder,
		args.proof_server,
		args.dry_run,
	)
	.await?;

	if args.dry_run {
		return Ok(());
	}

	let received_txs =
		generator.get_txs().await.map_err(|e| GenerateTxsError::GetTransactions(e))?;

	send_txs(&generator, generate_txs(&generator, received_txs).await?).await
}

async fn generate_txs(
	generator: &TxGenerator<SignatureType, ProofType>,
	received_txs: SourceTransactions<Signature, ProofMarker>,
) -> Result<DeserializedTransactionsWithContext<Signature, ProofMarker>, GenerateTxsError> {
	generator
		.build_txs(&received_txs)
		.await
		.map_err(|e| GenerateTxsError::BuildTransactions(e.error))
}

async fn send_txs(
	generator: &TxGenerator<SignatureType, ProofType>,
	generated_txs: DeserializedTransactionsWithContext<Signature, ProofMarker>,
) -> Result<(), GenerateTxsError> {
	generator
		.send_txs(&generated_txs)
		.await
		.map_err(|e| GenerateTxsError::SendTransactions(e))
}

#[cfg(test)]
mod tests {
	use std::str::FromStr;

	use super::*;
	use midnight_node_ledger_helpers::{NIGHT, WalletAddress};
	use midnight_node_toolkit::{
		cli_parsers::contract_address_decode,
		t_token,
		tx_generator::builder::{
			BatchesArgs, ClaimRewardsArgs, ContractCall, ContractCallArgs, ContractDeployArgs,
			SingleTxArgs,
		},
	};
	use test_case::test_case;

	fn resource_file(path: &str) -> String {
		format!("../../res/{path}")
	}

	// TODO: we need to consider using `proptest` here.
	// That would allow us to more robustly test random transactions within our valid bounds

	// TODO: write a better macro for this
	macro_rules! test_fixture {
		($builder:expr, $src_files:expr) => {
			GenerateTxsArgs {
				builder: $builder,
				source: Source {
					src_url: None,
					fetch_concurrency: 20,
					src_files: Some($src_files.map(resource_file).to_vec()),
				},
				destination: Destination {
					dest_urls: vec![],
					rate: 1.0,
					dest_file: Some("out.tx".to_string()),
					to_bytes: true,
				},
				proof_server: None,
				dry_run: false,
			}
		};
	}

	// TODO: There should be expected transactions here, not just an OK state.
	// We also need to define reaonsable errors
	#[test_case(test_fixture!(Builder::SingleTx(SingleTxArgs {
		shielded_amount: Some(0),
		shielded_token_type: t_token(),
		unshielded_amount: Some(100),
		unshielded_token_type: NIGHT,
		source_seed: "0000000000000000000000000000000000000000000000000000000000000001"
			.to_string(),
		destination_address: vec![
			WalletAddress::from_str(
				"mn_addr_undeployed13h0e3c2m7rcfem6wvjljnyjmxy5rkg9kkwcldzt73ya5pv7c4p8skzgqwj",
			)
			.unwrap(),
		],
		rng_seed: None,
	}), ["genesis/genesis_block_undeployed.mn"]) =>
	   matches Ok(..);
		"single-tx"
	)]
	#[test_case(test_fixture!(Builder::Send, ["genesis/genesis_block_undeployed.mn"]) =>
	   matches Ok(..);
		"send-tx"
	)]
	#[test_case(test_fixture!(Builder::ClaimRewards(ClaimRewardsArgs {
		funding_seed: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		rng_seed:None,
		amount: 500_000
	}), ["genesis/genesis_block_undeployed.mn"]) =>
	   matches Ok(..);
		"claim-rewards-tx"
	)]
	#[test_case(test_fixture!(Builder::Batches(BatchesArgs {
		funding_seed: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		num_txs_per_batch: 1,
		num_batches: 1,
		concurrency: None,
		rng_seed: None,
		shielded_token_type: t_token(),
		coin_amount: 100,
		initial_unshielded_intent_value: 500_000_000_000_000,
		unshielded_token_type: NIGHT,
		enable_shielded: false,
	}), ["genesis/genesis_block_undeployed.mn"]) =>
	   matches Ok(..);
		"batches-tx"
	)]
	#[test_case(test_fixture!(Builder::ContractSimple(
	    ContractCall::Deploy(ContractDeployArgs {
					funding_seed: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
					rng_seed: None,
					})
	), ["genesis/genesis_block_undeployed.mn"]) =>
	   matches Ok(..);
		"contract-call-deploy-tx"
	)]
	#[test_case(test_fixture!(Builder::ContractSimple(
	    ContractCall::Call(ContractCallArgs {
					funding_seed:"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
					call_key:"store".to_string(),
					contract_address: contract_address_decode(include_str!("../../../../res/test-contract/contract_address_undeployed.mn")).unwrap(),
					rng_seed: None,
					fee: 1_300_000,
					})
	), ["genesis/genesis_block_undeployed.mn", "test-contract/contract_tx_1_deploy_undeployed.mn"]) =>
	   matches Ok(..);
		"contract-call-call-tx"
	)]
	#[tokio::test]
	async fn test_generation(
		args: GenerateTxsArgs,
	) -> Result<DeserializedTransactionsWithContext<Signature, ProofMarker>, GenerateTxsError> {
		let generator = TxGenerator::<SignatureType, ProofType>::new(
			args.source,
			args.destination,
			args.builder,
			args.proof_server,
			args.dry_run,
		)
		.await?;
		let received_txs =
			generator.get_txs().await.map_err(|e| GenerateTxsError::GetTransactions(e))?;

		super::generate_txs(&generator, received_txs).await
	}
}

use std::{
	collections::HashMap,
	time::{SystemTime, UNIX_EPOCH},
};

use crate::{LedgerContext, ProofType, SignatureType, Source, TxGenerator, WalletSeed};
use clap::Args;
use midnight_node_ledger_helpers::{DustOutput, Timestamp};
use midnight_node_toolkit::{
	cli_parsers::{self as cli},
	serde_def::{DustGenerationInfoSer, QualifiedDustOutputSer},
};

#[derive(Args)]
pub struct DustBalanceArgs {
	#[command(flatten)]
	source: Source,
	/// The seed of the wallet to show wallet state for, including private state
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
	/// Dry-run - don't fetch wallet state, just print out settings
	#[arg(long)]
	dry_run: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct GenerationInfoPair {
	dust_output: QualifiedDustOutputSer,
	generation_info: Option<DustGenerationInfoSer>,
}

#[derive(Debug, serde::Serialize)]
pub struct DustBalanceJson {
	generation_infos: Vec<GenerationInfoPair>,
	source: HashMap<String, u128>,
	total: u128,
}

pub enum DustBalanceResult {
	Json(DustBalanceJson),
	DryRun(()),
}

pub async fn execute(
	args: DustBalanceArgs,
) -> Result<DustBalanceResult, Box<dyn std::error::Error + Send + Sync>> {
	let src = TxGenerator::<SignatureType, ProofType>::source(args.source, args.dry_run).await?;

	if args.dry_run {
		println!("Dry-run: fetching wallet for seed {:?}", args.seed);
		return Ok(DustBalanceResult::DryRun(()));
	}

	let source_blocks = src.get_txs().await?;
	let network_id = source_blocks.network().to_string();

	let context = LedgerContext::new_from_wallet_seeds(network_id, &[args.seed]);

	for block in source_blocks.blocks {
		context.update_from_block(block.transactions, block.context, block.state_root.clone());
	}

	context.with_wallet_from_seed(args.seed, |wallet| {
		let dust_state = wallet.dust.dust_local_state.as_ref().unwrap();

		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_secs();
		let timestamp = Timestamp::from_secs(now);
		let total = dust_state.wallet_balance(timestamp);

		let mut generation_infos = Vec::new();
		let mut source = HashMap::new();
		for dust_output in dust_state.utxos() {
			let dust_output_ser: QualifiedDustOutputSer = dust_output.into();
			let gen_info = dust_state.generation_info(&dust_output);
			let gen_info_pair = GenerationInfoPair {
				dust_output: dust_output_ser.clone(),
				generation_info: gen_info.map(|g| g.into()),
			};
			generation_infos.push(gen_info_pair);

			if let Some(gen_info) = gen_info {
				let balance = DustOutput::from(dust_output).updated_value(
					&gen_info,
					timestamp,
					&dust_state.params,
				);
				source.insert(dust_output_ser.nonce, balance);
			}
		}
		Ok(DustBalanceResult::Json(DustBalanceJson { generation_infos, source, total }))
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use test_case::test_case;

	/// Test data
	fn td(filepath: &str) -> String {
		[env!("CARGO_MANIFEST_DIR"), "/test-data/", &filepath].concat().to_string()
	}

	#[test_case("0000000000000000000000000000000000000000000000000000000000000001", vec![td("genesis/genesis_block_undeployed.mn")]; "when using seed 01")]
	#[tokio::test]
	async fn check_balance_non_zero(seed: &str, src_files: Vec<String>) {
		let seed = WalletSeed::try_from_hex_str(seed).unwrap();
		let args = DustBalanceArgs {
			source: Source { src_url: None, fetch_concurrency: 1, src_files: Some(src_files) },
			seed,
			dry_run: false,
		};

		let res = execute(args).await.expect("result was not Ok");

		assert!(
			matches!(res, DustBalanceResult::Json( DustBalanceJson { total, .. }) if total > 0 )
		);
	}
}

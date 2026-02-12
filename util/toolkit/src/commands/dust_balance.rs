use std::{
	collections::HashMap,
	time::{SystemTime, UNIX_EPOCH},
};

use crate::{LedgerContext, ProofType, SignatureType, TxGenerator, WalletSeed, source::Source};
use crate::{
	cli_parsers::{self as cli},
	serde_def::{DustGenerationInfoSer, QualifiedDustOutputSer},
};
use clap::Args;
use midnight_node_ledger_helpers::{DustOutput, Timestamp};

#[derive(Args)]
pub struct DustBalanceArgs {
	#[command(flatten)]
	pub source: Source,
	/// The seed of the wallet to show wallet state for, including private state
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	pub seed: WalletSeed,
	/// Dry-run - don't fetch wallet state, just print out settings
	#[arg(long)]
	pub dry_run: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct GenerationInfoPair {
	pub dust_output: QualifiedDustOutputSer,
	pub generation_info: Option<DustGenerationInfoSer>,
}

#[derive(Debug, serde::Serialize)]
pub struct DustBalanceJson {
	pub generation_infos: Vec<GenerationInfoPair>,
	pub source: HashMap<String, u128>,
	pub total: u128,
	pub capacity: u128,
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
		context.update_from_block(
			&block.transactions,
			&block.context,
			block.state_root.as_ref(),
			block.state.as_ref(),
		);
	}

	context.with_wallet_from_seed(args.seed, |wallet| {
		let dust_state = wallet.dust.dust_local_state.as_ref().unwrap();

		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_secs();
		let timestamp = Timestamp::from_secs(now);
		let total = dust_state.wallet_balance(timestamp);

		let mut capacity = 0u128;

		let mut generation_infos = Vec::new();
		let mut source = HashMap::new();
		for dust_output in dust_state.utxos() {
			let dust_output_ser: QualifiedDustOutputSer = dust_output.into();
			let gen_info = dust_state.generation_info(&dust_output);
			capacity += gen_info
				.as_ref()
				.map(|g| g.value * dust_state.params.night_dust_ratio as u128)
				.unwrap_or(0);
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
		Ok(DustBalanceResult::Json(DustBalanceJson { generation_infos, source, total, capacity }))
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tx_generator::source::FetchCacheConfig;
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
			source: Source {
				src_url: None,
				fetch_concurrency: 1,
				src_files: Some(src_files),
				dust_warp: true,
				ignore_block_context: false,
				fetch_cache: FetchCacheConfig::InMemory,
			},
			seed,
			dry_run: false,
		};

		let res = execute(args).await.expect("result was not Ok");

		assert!(
			matches!(res, DustBalanceResult::Json( DustBalanceJson { total, .. }) if total > 0 )
		);
	}
}

// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	serde_def::SourceTransactions,
	tx_generator::{
		TxGenerator,
		builder::{
			BuildTxs, Builder, CustomContractArgs, builders::ledger_9::CustomContractBuilder,
		},
		destination::Destination,
		source::Source,
	},
};
use clap::Args;
use midnight_ledger_unsafe_helpers::{
	DefaultDB, IntentCustom, ProofMarker, PureGeneratorPedersen, Resolver, SerdeTransaction,
	Signature, Transaction, deserialize, strictness_without_proofs,
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use std::{fs, path::Path, sync::Arc};

#[derive(Args)]
pub struct SendIntentArgs {
	#[command(flatten)]
	pub source: Source,
	#[command(flatten)]
	pub destination: Destination,
	/// Proof server URL, or `mock` to emit mock proofs (accepted only by `mock-proofs` nodes)
	#[arg(long, short)]
	pub proof_server: Option<String>,
	#[command(flatten)]
	pub contract_args: CustomContractArgs,
	/// Dry-run - don't generate any txs, just print out the settings
	#[arg(long)]
	pub dry_run: bool,
	/// Write the contract's on-chain state as it will be after this transaction, so the next
	/// circuit call can be prepared without waiting for inclusion.
	#[arg(long)]
	pub output_onchain_state: Option<String>,
}

pub async fn execute(args: SendIntentArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let contract_args = args.contract_args.clone();
	let builder = Builder::ContractCustom(args.contract_args);

	let generator =
		TxGenerator::new(args.source, args.destination, builder, args.proof_server, args.dry_run)
			.await?;

	if args.dry_run {
		return Ok(());
	}

	let received_txs = generator.get_txs().await?;
	let generated_txs = match &args.output_onchain_state {
		None => generator.build_txs(&received_txs).await?,
		Some(out) => {
			build_and_predict_state(&generator, &contract_args, &received_txs, out).await?
		},
	};
	generator.send_txs(&generated_txs).await?;

	Ok(())
}

/// `TxGenerator::build_txs` drops its ledger context after building, so this keeps one, applies
/// the built transaction to it and writes the contract's resulting state.
async fn build_and_predict_state(
	generator: &TxGenerator,
	contract_args: &CustomContractArgs,
	received_txs: &SourceTransactions,
	out: &str,
) -> Result<SerializedTxBatches, Box<dyn std::error::Error + Send + Sync>> {
	let ctx = generator
		.build_context(received_txs)
		.await?
		.and_then(|ctx| ctx.into_ledger9())
		.ok_or("--output-onchain-state needs a ledger 9 chain and a funding wallet")?;
	let ctx = Arc::new(ctx);

	let prover = Builder::make_prover(&generator.prover_config);
	let batches = CustomContractBuilder::new(contract_args.clone(), ctx.clone(), prover)
		.build_txs_from(received_txs.clone())
		.await?;

	let raw = batches
		.batches
		.first()
		.and_then(|b| b.first())
		.ok_or("send-intent built no transaction")?;
	let tx: Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB> =
		deserialize(raw.tx.as_bytes())?;
	let block_context = ctx
		.latest_block_context
		.lock()
		.map_err(|e| format!("block context lock poisoned: {e}"))?
		.clone()
		.ok_or("no block context after replay")?;
	// `prove` has already checked the transaction, with proofs unless they are mock.
	ctx.update_from_tx_with_strictness(
		&SerdeTransaction::Midnight(tx),
		&block_context,
		strictness_without_proofs(),
	)?;

	let resolver: &'static Resolver = Box::leak(Box::new(IntentCustom::<DefaultDB>::get_resolver(
		&contract_args.compiled_contract_dirs,
	)?));
	let intent_file =
		contract_args.intent_files.first().ok_or("send-intent needs an intent file")?;
	let address = IntentCustom::<DefaultDB>::new_from_file(intent_file, resolver)?
		.find_contract_address()
		.ok_or("intent names no contract address")?;
	let state = crate::commands::fork::ledger_9::contract_state::get_contract_state(&ctx, address)?;
	if let Some(parent) = Path::new(out).parent() {
		fs::create_dir_all(parent)?;
	}
	fs::write(out, state)?;
	Ok(batches)
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
			fetch_best: false,
			fetch_cache: FetchCacheConfig::InMemory,
			ledger_state_db: String::new(),
			replay_checkpoint_interval: 0,
		};

		let destination = Destination {
			dest_urls: vec![],
			rate: 0.0,
			dest_file: Some(output_file.to_string()),
			no_watch_progress: false,
			no_wait_finality: false,
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
			output_onchain_state: None,
		};

		execute(args).await.expect("should work during sending");
		assert!(fs::exists(output_file).expect("should_exist"));
	}
}

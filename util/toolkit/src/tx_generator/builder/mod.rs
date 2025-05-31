// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use async_trait::async_trait;
use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::*;
use std::{fs, path::Path, sync::Arc};

use crate::{
	ProofType, SignatureType, cli_parsers as cli,
	genesis_generator::MINT_AMOUNT,
	serde_def::{DeserializedTransactionsWithContext, DeserializedTransactionsWithContextBatch},
};

pub mod builders;

const FUNDING_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000001";

#[derive(Args, Clone)]
pub struct BuilderArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	funding_seed: String,
	/// Number of txs that can be sent concurrently
	#[arg(long, short = 'n', default_value = "1")]
	num_txs_per_batch: usize,
	/// Number of batches to generate
	#[arg(long, short = 'b', default_value = "1")]
	num_batches: usize,
	/// Number of transactions to generate in parallel. Default: # Available CPUs
	#[arg(long, short)]
	concurrency: Option<usize>,
	/// Call key to be called in a contract
	#[arg(long, default_value = "store")]
	call_key: String,
	/// File to read the contract address from
	#[arg(long, default_value = "./res/test-contract/contract_address_undeployed.mn")]
	contract_address: String,
	/// Threshold for Maintenance ReplaceAthority
	#[arg(long, short, default_value = "1")]
	threshold: u32,
	/// Counter for Maintenance ReplaceAthority
	#[arg(long, default_value = "0")]
	counter: u32,
	#[arg(
        short,
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	rng_seed: Option<[u8; 32]>,
	// Proof Server Host
	#[arg(long, short)]
	proof_server: Option<String>,
}

#[derive(Subcommand, Clone)]
pub enum ContractCall {
	Deploy(BuilderArgs),
	Call(BuilderArgs),
	Maintenance(BuilderArgs),
}

#[derive(Subcommand, Clone)]
pub enum Builder {
	Batches(BuilderArgs),
	#[clap(subcommand)]
	ContractCalls(ContractCall),
	ClaimMint(BuilderArgs),
	Send,
	Migrate,
}

#[async_trait]
pub trait BuildTxs {
	async fn build_txs_from(
		&self,
		received_tx: DeserializedTransactionsWithContext<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<
		DeserializedTransactionsWithContext<SignatureType, ProofType>,
		Box<dyn std::error::Error>,
	>;

	fn contract_address(&self, file: &String) -> [u8; 32] {
		let path = Path::new(file);
		let hex_str = fs::read_to_string(path)
			.expect("Error reading Contract Address file")
			.trim()
			.to_string();

		let contract_address: [u8; 32] = hex::decode(hex_str)
			.expect("Error hex decoding Contract Address")[3..]
			.try_into()
			.expect("Invalid Contract Address length");
		contract_address
	}
}

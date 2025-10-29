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
use builders::{
	BatchesBuilder, ClaimRewardsBuilder, ContractCallBuilder, ContractDeployBuilder,
	ContractMaintenanceBuilder, CustomContractBuilder, DoNothingBuilder, ReplaceInitialTxBuilder,
	single_tx::SingleTxBuilder,
};
use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::*;
use std::sync::Arc;

use crate::{
	ProofType, SignatureType, cli_parsers as cli,
	serde_def::{
		DeserializedTransactionsWithContext, DeserializedTransactionsWithContextBatch,
		SourceTransactions,
	},
	tx_generator::builder::builders::RegisterDustAddressBuilder,
};

pub mod builders;

pub const FUNDING_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000001";

#[derive(Args, Clone, Debug)]
pub struct ClaimRewardsArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Amount for the claim mint
	#[arg(long, short, default_value_t = 500_000)]
	pub amount: u128,
}

#[derive(Args, Clone, Debug)]
pub struct ContractDeployArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Args, Clone, Debug)]
pub struct CustomContractArgs {
	#[clap(flatten)]
	pub info: ContractDeployArgs,
	/// The directory containing:
	///  * intent directory, containing serialized intent files
	///  * directories with files for the Resolver
	#[arg(short, long)]
	pub compiled_contract_dir: String,
	/// Intent file to include in the transaction. Accepts multiple
	#[arg(long = "intent-file")]
	pub intent_files: Vec<String>,
	/// Input Unshielded UTXOs to include in the transaction. Accepts multiple. UTXOs must be
	/// present in wallet of funding-seed.
	#[arg(long = "input-utxo", value_parser = cli::utxo_id_decode)]
	pub utxo_inputs: Vec<UtxoId>,
	/// Zswap State file containing coin info
	#[arg(long)]
	pub zswap_state_file: Option<String>,
	/// Shielded Destination addresses - used to find encryption keys
	#[arg(long = "shielded-destination", value_parser = cli::wallet_address)]
	pub shielded_destinations: Vec<WalletAddress>,
}

#[derive(Args, Clone, Debug)]
pub struct ContractCallArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	/// Call key to be called in a contract
	#[arg(long, default_value = "store")]
	pub call_key: String,
	/// File to read the contract address from
	#[arg(long, value_parser = cli::contract_address_decode)]
	pub contract_address: ContractAddress,
	#[arg(
        short,
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Transaction fee value
	#[arg(short, long, default_value_t = 1_300_000)]
	pub fee: u128,
}

#[derive(Args, Clone, Debug)]
pub struct ContractMaintenanceArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	/// File to read the contract address from
	#[arg(long, value_parser = cli::contract_address_decode)]
	pub contract_address: ContractAddress,
	/// Threshold for Maintenance ReplaceAthority
	#[arg(long, short, default_value = "1")]
	pub threshold: u32,
	/// Counter for Maintenance ReplaceAthority
	#[arg(long, default_value = "0")]
	pub counter: u32,
	#[arg(
        short,
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Transaction fee value
	#[arg(short, long, default_value_t = 1_300_000)]
	pub fee: u128,
}

#[derive(Args, Clone, Debug)]
pub struct BatchesArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	/// Number of txs that can be sent concurrently
	#[arg(long, short = 'n', default_value = "1")]
	pub num_txs_per_batch: usize,
	/// Number of batches to generate
	#[arg(long, short = 'b', default_value = "1")]
	pub num_batches: usize,
	/// Number of transactions to generate in parallel. Default: # Available CPUs
	#[arg(long)]
	pub concurrency: Option<usize>,
	#[arg(
        short,
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Coin amount per transaction
	#[arg(short, long, default_value_t = 100)]
	pub coin_amount: u128,
	/// Type of shielded token to send
	#[arg(
		long,
		value_parser = cli::token_decode::<ShieldedTokenType>,
		default_value = "0000000000000000000000000000000000000000000000000000000000000000"
	)]
	pub shielded_token_type: ShieldedTokenType,
	/// Initial unshielded offer amount
	#[arg(short, long, default_value_t = 10_000)]
	pub initial_unshielded_intent_value: u128,
	/// Type of unshielded token to send
	#[arg(
		long,
		value_parser = cli::token_decode::<UnshieldedTokenType>,
		default_value = "0000000000000000000000000000000000000000000000000000000000000000"
	)]
	pub unshielded_token_type: UnshieldedTokenType,
	/// Enable Shielded transfers in batches
	#[arg(long)]
	pub enable_shielded: bool,
}

// TODO: TokenIDs for shielded and unshielded
#[derive(Args, Clone, Debug)]
pub struct SingleTxArgs {
	/// Amount to send to each shielded wallet
	#[arg(long)]
	pub shielded_amount: Option<u128>,
	/// Type of shielded token to send
	#[arg(
		long,
		value_parser = cli::token_decode::<ShieldedTokenType>,
		default_value = "0000000000000000000000000000000000000000000000000000000000000000"
	)]
	pub shielded_token_type: ShieldedTokenType,
	/// Amount to send to each unshielded wallet
	#[arg(long)]
	pub unshielded_amount: Option<u128>,
	/// Type of unshielded token to send
	#[arg(
		long,
		value_parser = cli::token_decode::<UnshieldedTokenType>,
		default_value = "0000000000000000000000000000000000000000000000000000000000000000"
	)]
	pub unshielded_token_type: UnshieldedTokenType,
	/// Seed for source wallet
	#[arg(long)]
	pub source_seed: String,
	/// Destination address, both shielded and unshielded
	#[arg(long, required = true)]
	pub destination_address: Vec<WalletAddress>,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}
#[derive(Args, Clone, Debug)]
pub struct RegisterDustAddressArgs {
	/// Seed for source wallet
	#[arg(long)]
	pub wallet_seed: String,
	/// Seed for funding wallet
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Subcommand, Clone, Debug)]
pub enum ContractCall {
	Deploy(ContractDeployArgs),
	Call(ContractCallArgs),
	Maintenance(ContractMaintenanceArgs),
}

#[derive(Subcommand, Clone, Debug)]
pub enum Builder {
	/// Construct batches of transactions
	Batches(BatchesArgs),
	/// Simple built-in contract
	#[clap(subcommand)]
	ContractSimple(ContractCall),
	/// Construct txs from custom contract intents
	ContractCustom(CustomContractArgs),
	/// Claim rewards
	ClaimRewards(ClaimRewardsArgs),
	/// Send single transaction with one-or-many outputs
	SingleTx(SingleTxArgs),
	RegisterDustAddress(RegisterDustAddressArgs),
	/// Send is a no-op here (source is sent directly to destination)
	Send,
	Migrate,
}

pub struct DynamicTransactionBuilder<T: BuildTxs + Send + Sync> {
	builder: T,
}

#[derive(Debug)]
pub struct DynamicError {
	pub error: Box<dyn std::error::Error + Send + Sync + 'static>,
}

#[allow(deprecated)]
impl std::error::Error for DynamicError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		self.error.source()
	}

	fn description(&self) -> &str {
		self.error.description()
	}

	fn cause(&self) -> Option<&dyn std::error::Error> {
		self.error.cause()
	}
}

impl std::fmt::Display for DynamicError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.error, f)
	}
}

#[async_trait]
impl<T: BuildTxs + Send + Sync> BuildTxs for DynamicTransactionBuilder<T> {
	type Error = DynamicError;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		let x = self.builder.build_txs_from(received_tx, prover_arc).await;

		x.map_err(|e| DynamicError { error: Box::new(e) })
	}
}

impl Builder {
	pub fn to_builder(self, dry_run: bool) -> Box<dyn BuildTxs<Error = DynamicError>> {
		fn constr(
			builder: impl BuildTxs + Send + Sync + 'static,
		) -> Box<dyn BuildTxs<Error = DynamicError>> {
			Box::new(DynamicTransactionBuilder { builder })
		}

		if dry_run {
			println!("Dry-run: Builder type: {:?}", &self);
		}

		match self {
			Builder::Batches(args) => constr(BatchesBuilder::new(args)),
			Builder::ContractSimple(call) => match call {
				ContractCall::Deploy(args) => constr(ContractDeployBuilder::new(args)),
				ContractCall::Call(args) => constr(ContractCallBuilder::new(args)),
				ContractCall::Maintenance(args) => constr(ContractMaintenanceBuilder::new(args)),
			},
			Builder::ContractCustom(args) => constr(CustomContractBuilder::new(args)),
			Builder::ClaimRewards(args) => constr(ClaimRewardsBuilder::new(args)),
			Builder::SingleTx(args) => constr(SingleTxBuilder::new(args)),
			Builder::RegisterDustAddress(args) => constr(RegisterDustAddressBuilder::new(args)),
			Builder::Send => constr(DoNothingBuilder::new()),
			Builder::Migrate => constr(ReplaceInitialTxBuilder::new()),
		}
	}
}

#[async_trait]
pub trait BuildTxs {
	type Error: std::error::Error + Send + Sync + 'static;
	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error>;
}

/// An extension to help build transactions
pub trait BuildTxsExt {
	fn funding_seed(&self) -> WalletSeed;

	fn rng_seed(&self) -> Option<[u8; 32]>;

	/// Returns a tuple of an Arc<LedgerContext> and the StandardTransactionInfo
	fn context_and_tx_info(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> (Arc<LedgerContext<DefaultDB>>, StandardTrasactionInfo<DefaultDB>) {
		// - Calculate the funding `WalletSeed` (can be more than one)
		let input_wallets_seeds = vec![self.funding_seed()];

		// Get the network id from the initial TX
		let network_id = received_tx.network();

		// initialize `LedgerContext` with the wallets
		let context = LedgerContext::new_from_wallet_seeds(network_id, &input_wallets_seeds);

		// update the context applying all existing previous txs queried from source (either genesis or live network)
		for block in received_tx.blocks {
			context.update_from_block(block.transactions, block.context, block.state_root.clone());
		}

		let context_arc = Arc::new(context);

		// - Transaction info
		let tx_info = StandardTrasactionInfo::new_from_context(
			context_arc.clone(),
			prover_arc.clone(),
			self.rng_seed(),
			None,
		);

		(context_arc, tx_info)
	}
}

/// Create Intent Info
pub trait CreateIntentInfo {
	fn create_intent_info(&self) -> Box<dyn BuildIntent<DefaultDB>>;
}

/// A trait to save a Contract (serialized`Intent` Structure) into a file
#[async_trait]
pub trait IntentToFile: CreateIntentInfo + BuildTxsExt {
	async fn generate_intent_file(
		&mut self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
		// the directory where to save the file
		dir: &str,
		// partial name of the file
		partial_name: &str,
	) {
		println!("Generate intent file...");
		let (_, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		let intent_info = self.create_intent_info();

		tx_info.add_intent(1, intent_info);

		tx_info.save_intents_to_file(dir, partial_name).await;
	}
}

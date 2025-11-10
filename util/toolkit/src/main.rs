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

use clap::{Args, Parser, Subcommand};
use commands::{
	contract_address::{self, ContractAddressArgs},
	generate_genesis::{self, GenerateGenesisArgs},
	generate_intent::{self, GenerateIntentArgs},
	generate_sample_intent::{self, GenerateSampleIntentArgs},
	generate_txs::{self, GenerateTxsArgs},
	get_tx_from_context::{self, GetTxFromContextArgs},
	random_address::{self, RandomAddressArgs},
	send_intent::{self, SendIntentArgs},
	show_address::{self, ShowAddressArgs},
	show_seed::{self, ShowSeedArgs},
	show_transaction::{self, ShowTransactionArgs},
	show_viewing_key::{self, ShowViewingKeyArgs},
	show_wallet::{self, ShowWalletArgs, ShowWalletResult},
};
use midnight_node_ledger_helpers::*;
use std::{
	error::Error,
	fmt,
	panic::{self, AssertUnwindSafe},
};

use midnight_node_toolkit::{
	ProofType, SignatureType,
	tx_generator::{TxGenerator, source::Source},
};

use crate::commands::{
	contract_state::{self, ContractStateArgs},
	dust_balance::{self, DustBalanceArgs, DustBalanceResult},
	show_address::ShowAddress,
	show_token_type::{self, ShowTokenType, ShowTokenTypeArgs},
};

mod commands;
mod utils;

/// Node Toolkit for Midnight
#[derive(Parser)]
#[command(about, long_about, verbatim_doc_comment)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Generate transactions against a genesis tx file or a live node network.
	///
	/// How you choose to generate transactions will determine in which order they may be sent. For
	/// context:
	///
	/// The ledger state is a merkle tree whose root changes after each transaction is
	/// processed. A valid transaction must be generated against either the current ledger state merkle
	/// tree root, or a past root. This means that if you generate a "tree" of transactions using a
	/// known root of a node e.g. the genesis state, executing any other transactions on the node that
	/// aren't included in your generated transaction tree will result in your generated transactions
	/// failing.
	GenerateTxs(GenerateTxsArgs),
	/// Generates the genesis transaction and state, outputting them to file in the current working
	/// directory. Genesis generation is seeded, so output is deterministic.
	GenerateGenesis(GenerateGenesisArgs),
	GenerateIntent(GenerateIntentArgs),
	/// Generate Intent Files
	GenerateSampleIntent(GenerateSampleIntentArgs),
	/// Sends a custom contract (serialized intent .mn files )
	SendIntent(SendIntentArgs),
	/// Show the state of a wallet using it's seed
	DustBalance(DustBalanceArgs),
	/// Show the state of a wallet using it's seed
	ShowWallet(ShowWalletArgs),
	/// Show the address of a wallet using it's seed
	ShowAddress(ShowAddressArgs),
	/// Show the seed of a wallet
	ShowSeed(ShowSeedArgs),
	/// Show the viewing key of a shielded wallet using its seed
	ShowViewingKey(ShowViewingKeyArgs),
	/// Show the token type for a contract address + domain sep pair
	ShowTokenType(ShowTokenTypeArgs),
	/// Show the deserialized value of a serialized transaction
	ShowTransaction(ShowTransactionArgs),
	/// Show and save in a file the Contract Address included in a DeployContract tx
	ContractAddress(ContractAddressArgs),
	/// Show and save a Contract state
	ContractState(ContractStateArgs),
	/// Extract `Transaction` from `TransactionWithContext`
	GetTxFromContext(GetTxFromContextArgs),
	/// Generate a random `UserAddress` for a given `NetworkId`
	RandomAddress(RandomAddressArgs),
	/// Get the version information
	Version,
}

#[derive(Args)]
#[group(required = false, multiple = false)]
pub struct GenesisSource {
	/// RPC URL of node instance; Used to fetch existing transactions
	#[arg(long, short = 'u')]
	rpc_url: Option<String>,
	/// Filename of genesis tx. Used as initial state for generated txs.
	#[arg(long)]
	genesis_tx: Option<String>,
	/// Number of threads to use when fetching transactions from a live network
	#[arg(long, default_value = "20")]
	fetch_concurrency: usize,
}

#[derive(Debug)]
struct PanicError(String);

impl fmt::Display for PanicError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Panic occurred: {}", self.0)
	}
}

impl Error for PanicError {}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let result = panic::catch_unwind(AssertUnwindSafe(|| {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
			.block_on(async {
				// Initialize the logger.
				structured_logger::Builder::with_level("info")
					.with_default_writer(structured_logger::async_json::new_writer(
						tokio::io::sink(),
					))
					.with_target_writer(
						"midnight_node_toolkit*",
						structured_logger::async_json::new_writer(tokio::io::stdout()),
					)
					.init();

				// Initialize tracing (used by ledger to emit warnings)
				let subscriber =
					tracing_subscriber::fmt().with_max_level(tracing::Level::WARN).finish();
				tracing::subscriber::set_global_default(subscriber)?;

				let cli = Cli::parse();

				run_command(cli.command).await
			})
	}));

	// Pass through standard `Error`s or transform panics into `Error`
	result.unwrap_or_else(|panic_info| {
		let msg = match panic_info.downcast_ref::<&str>() {
			Some(s) => s.to_string(),
			None => match panic_info.downcast_ref::<String>() {
				Some(s) => s.clone(),
				None => "Unknown panic".to_string(),
			},
		};
		let err: Box<dyn std::error::Error + Send + Sync> = Box::new(PanicError(msg));
		Err(err)
	})
}

pub(crate) async fn run_command(
	cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	match cmd {
		Commands::GenerateTxs(args) => {
			generate_txs::execute(args).await?;
			Ok(())
		},
		Commands::GenerateIntent(args) => {
			generate_intent::execute(args).await?;
			Ok(())
		},
		Commands::GenerateSampleIntent(args) => {
			generate_sample_intent::execute(args).await;
			Ok(())
		},
		Commands::SendIntent(args) => {
			send_intent::execute(args).await?;
			Ok(())
		},
		Commands::GenerateGenesis(args) => {
			let generator = generate_genesis::execute(args).await?;
			println!("The tx: {:#?}", generator.txs);
			Ok(())
		},
		Commands::ShowWallet(args) => {
			let result = show_wallet::execute(args).await?;
			match result {
				ShowWalletResult::Debug(result) => {
					println!("{:#?}", result.wallet);
					println!("Unshielded UTXOs: {:#?}", result.utxos)
				},
				ShowWalletResult::Json(json) => {
					println!("{}", serde_json::to_string_pretty(&json)?);
				},
				ShowWalletResult::DryRun(()) => (),
			}

			Ok(())
		},
		Commands::ShowAddress(args) => {
			let address = show_address::execute(args);
			match address {
				ShowAddress::Addresses(addresses) => {
					println!("{}", serde_json::to_string_pretty(&addresses)?);
				},
				ShowAddress::SingleAddress(address) => println!("{address}"),
			};

			Ok(())
		},
		Commands::ShowSeed(args) => {
			let seed = show_seed::execute(args);
			println!("{}", seed);
			Ok(())
		},
		Commands::ShowViewingKey(args) => {
			let viewing_key = show_viewing_key::execute(args);
			println!("{viewing_key}");
			Ok(())
		},
		Commands::ShowTransaction(args) => {
			let transaction_information = show_transaction::execute(args)?;

			println!("{transaction_information}");
			Ok(())
		},
		Commands::ContractAddress(args) => {
			let address = contract_address::execute(args)?;
			println!("{address}");
			Ok(())
		},
		Commands::ContractState(args) => contract_state::execute(args).await,
		Commands::GetTxFromContext(args) => {
			let (serialized_tx, timestamp) = get_tx_from_context::execute(&args)?;
			std::fs::write(args.dest_file, serialized_tx)?;
			println!("{}", timestamp);
			Ok(())
		},
		Commands::RandomAddress(args) => {
			let address = random_address::execute(args);
			println!("{}", address);

			Ok(())
		},
		Commands::Version => {
			let node_version = utils::find_crate_version!("../../../node/Cargo.toml");
			let ledger_version =
				find_dependency_version("mn-ledger").expect("missing ledger version");
			let compactc_version = include_str!("../../toolkit-js/COMPACTC_VERSION").trim();

			println!(
				"Node: {}\nLedger: {}\nCompactc: {}",
				node_version, ledger_version, compactc_version
			);
			return Ok(());
		},
		Commands::ShowTokenType(args) => {
			let token_type = show_token_type::execute(args);
			match token_type {
				ShowTokenType::TokenTypes(token_types) => {
					println!("{}", serde_json::to_string_pretty(&token_types)?);
				},
				ShowTokenType::SingleTokenType(ttype) => println!("{ttype}"),
			};

			Ok(())
		},
		Commands::DustBalance(args) => {
			let result = dust_balance::execute(args).await?;
			match result {
				DustBalanceResult::Json(json) => {
					println!("{}", serde_json::to_string_pretty(&json)?);
				},
				DustBalanceResult::DryRun(()) => (),
			}

			Ok(())
		},
	}
}

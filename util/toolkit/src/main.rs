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
use midnight_node_ledger_helpers::*;
use std::{
	error::Error,
	fmt,
	panic::{self, AssertUnwindSafe},
	path::Path,
};

use mn_node_toolkit::{
	ProofType, SignatureType,
	cli_parsers::{self as cli},
	genesis_generator::{FundingArgsShielded, FundingArgsUnshielded, GenesisGenerator},
	tx_generator::{TxGenerator, builder::Builder, destination::Destination, source::Source},
};

const GENESIS_NONCE_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000037";

/// Node Toolkit for Midnight
#[derive(Parser)]
#[command(version, about, long_about, verbatim_doc_comment)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Generates the genesis transaction and state, outputting them to file in the current working
	/// directory. Genesis generation is seeded, so output is deterministic.
	GenerateGenesis {
		/// Seed for genesis block generation
		#[arg(
            short,
            long,
            value_parser = cli::hex_str_decode::<[u8; 32]>,
            default_value = GENESIS_NONCE_SEED,
		)]
		nonce_seed: [u8; 32],
		// Target Network
		#[arg(long, value_parser = cli::network_id_decode)]
		network: NetworkId,
		// Proof Server Host
		#[arg(long, short)]
		proof_server: Option<String>,
		// Output suffix (defaults to match network)
		#[arg(long)]
		suffix: Option<String>,
		/// Aguments for funding Shielded wallets
		#[command(flatten)]
		shielded: FundingArgsShielded,
		/// Aguments for funding Unshielded wallets
		#[command(flatten)]
		unshielded: FundingArgsUnshielded,
		/// Output directory
		#[arg(long, short = 'o', default_value = "out")]
		out_dir: String,
	},
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
	GenerateTxs {
		#[clap(subcommand)]
		builder: Builder,
		#[command(flatten)]
		source: Source,
		#[command(flatten)]
		destination: Destination,
		// Proof Server Host
		#[arg(long, short)]
		proof_server: Option<String>,
	},
	/// Show the state of a wallet using it's seed
	ShowWallet {
		#[command(flatten)]
		source: Source,
		/// Wallet seed to check the balance of
		#[arg(long, value_parser = cli::wallet_seed_decode)]
		seed: WalletSeed,
	},
	/// Show the address of a wallet using it's seed
	ShowAddress {
		/// Target network
		#[arg(long, value_parser = cli::network_id_decode)]
		network: NetworkId,
		/// Wallet seed
		#[arg(long, value_parser = cli::wallet_seed_decode)]
		seed: WalletSeed,
		// HD structure derivation path
		#[arg(long, short)]
		path: String,
	},
	/// Show the deserialized value of a serialized transaction
	ShowTransaction {
		/// Target network
		#[arg(long, value_parser = cli::network_id_decode)]
		network: NetworkId,
		/// Serialized Transaction
		#[arg(long, short)]
		src_file: String,
		/// Select if the transactions to show is saved as bytes
		#[arg(long, default_value = "false")]
		from_bytes: bool,
	},
	/// Show the deserialized value of a serialized transaction with context
	ShowTransactionWithContext {
		/// Target network
		#[arg(long, value_parser = cli::network_id_decode)]
		network: NetworkId,
		/// Serialized Transaction
		#[arg(long, short)]
		src_file: String,
		/// Select if the transactions to show is saved as bytes
		#[arg(long, default_value = "false")]
		from_bytes: bool,
	},
	/// Show and save in a file the Contract Address included in a DeployContract tx
	ContractAddress {
		/// Target network
		#[arg(long, value_parser = cli::network_id_decode)]
		network: NetworkId,
		/// Serialized Transaction
		#[arg(long, short)]
		src_file: String,
		/// Destination file to save the address
		#[arg(long, short)]
		dest_file: String,
	},
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
						"mn_node_toolkit*",
						structured_logger::async_json::new_writer(tokio::io::stdout()),
					)
					.init();

				let cli = Cli::parse();

				match cli.command {
					Commands::GenerateTxs { source, destination, builder, proof_server } => {
						let generator = TxGenerator::<SignatureType, ProofType>::new(
							source,
							destination,
							builder,
							proof_server,
						)
						.await?;
						let received_txs = generator.get_txs().await?;
						let generated_txs = generator.build_txs(&received_txs).await?;
						generator.send_txs(&generated_txs).await
					},
					Commands::GenerateGenesis {
						proof_server,
						nonce_seed,
						network,
						suffix,
						shielded,
						unshielded,
						out_dir,
					} => {
						let dir = Path::new(&out_dir);
						std::fs::create_dir_all(&dir)?;

						let network_string = match network {
							NetworkId::MainNet => "mainnet",
							NetworkId::DevNet => "devnet",
							NetworkId::TestNet => "testnet",
							NetworkId::Undeployed => "undeployed",
							_ => panic!("unknown network id: {network:?}"),
						};

						let suffix =
							if let Some(ref suffix) = suffix { suffix } else { network_string };

						println!("generating genesis for network {network_string} ({suffix})...");
						let genesis = GenesisGenerator::new(
							nonce_seed,
							network,
							proof_server,
							shielded,
							unshielded,
						)
						.await;

						let genesis_state_path = dir.join(format!("genesis_state_{suffix}.mn"));
						std::fs::write(&genesis_state_path, serialize(&genesis.state, network)?)?;
						println!("Written to {}", genesis_state_path.display());

						let genesis_tx_path = dir.join(format!("genesis_tx_{suffix}.mn"));
						std::fs::write(&genesis_tx_path, serialize(&genesis.tx, network)?)?;
						println!("Written to {}", genesis_tx_path.display());

						Ok(())
					},
					Commands::ShowWallet { source, seed } => {
						let (src, _) =
							TxGenerator::<SignatureType, ProofType>::source(source).await?;

						let context = LedgerContext::new_from_wallet_seeds(&[seed]);
						let txs = src.get_txs().await?.flat();

						context.update_from_txs(txs);

						context.with_ledger_state(|ledger_state| {
							context.with_wallet_from_seed(seed, |wallet| {
								println!("{:#?}", wallet);
								println!("{:#?}", wallet.unshielded_utxos(ledger_state))
							});
						});

						Ok(())
					},
					Commands::ShowAddress { seed, network, path } => {
						let derivation_path = DerivationPath::new(path.clone());

						let address = match derivation_path.role {
							Role::UnshieldedExternal => {
								UnshieldedWallet::from_path(seed, path.clone()).address(network)
							},
							Role::Zswap => {
								ShieldedWallet::<DefaultDB>::from_path(seed, path).address(network)
							},
							_ => unimplemented!(),
						};

						println!("{}", address.0);
						Ok(())
					},
					Commands::ShowTransaction { src_file, network, from_bytes } => {
						let (deserialized_tx, size) = if !from_bytes {
							// Read single tx from file
							let file_content = std::fs::read(&src_file)?;
							let tx_hex = String::from_utf8_lossy(&file_content);
							// Some IDEs auto-add an extra empty line at the end of the file
							let sanitized_hex_tx: String =
								tx_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();

							let tx = hex::decode(&sanitized_hex_tx)?;

							let bytes = tx.as_slice();
							let deserialized_tx: Transaction<
								SignatureType,
								ProofType,
								PedersenRandomness,
								DefaultDB,
							> = deserialize(bytes, network)?;

							(deserialized_tx, bytes.len())
						} else {
							let bytes = std::fs::read(&src_file)?;
							let deserialized_tx: Transaction<
								SignatureType,
								ProofType,
								PedersenRandomness,
								DefaultDB,
							> = mn_ledger_serialize::deserialize(bytes.as_slice(), network)?;

							(deserialized_tx, bytes.len())
						};

						println!("\nTx {:#?}\n", deserialized_tx);
						println!("Size {:?}", size);
						Ok(())
					},
					Commands::ShowTransactionWithContext { src_file, network, from_bytes } => {
						let (deserialized_tx, size) = if !from_bytes {
							// Read single tx from file
							let file_content = std::fs::read(&src_file)?;
							let tx_hex = String::from_utf8_lossy(&file_content);
							// Some IDEs auto-add an extra empty line at the end of the file
							let sanitized_hex_tx: String =
								tx_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();

							let tx = hex::decode(&sanitized_hex_tx)?;

							let bytes = tx.as_slice();
							let deserialized_tx: TransactionWithContext<
								SignatureType,
								ProofType,
								DefaultDB,
							> = deserialize(bytes, network)?;

							(deserialized_tx, bytes.len())
						} else {
							let bytes = std::fs::read(&src_file)?;
							let deserialized_tx: TransactionWithContext<
								SignatureType,
								ProofType,
								DefaultDB,
							> = mn_ledger_serialize::deserialize(bytes.as_slice(), network)?;

							(deserialized_tx, bytes.len())
						};

						println!("\nTx {:#?}\n", deserialized_tx);
						println!("Size {:?}", size);
						Ok(())
					},
					Commands::ContractAddress { src_file, dest_file, network } => {
						let bytes = std::fs::read(&src_file)?;
						let tx_with_context: TransactionWithContext<
							SignatureType,
							ProofType,
							DefaultDB,
						> = mn_ledger_serialize::deserialize(bytes.as_slice(), network)?;
						let deploy = tx_with_context
							.tx
							.deploys()
							.next()
							.expect("There is not any `ContractDeploy` in the tx");

						let deserialized_address = deploy.1.address();
						let address =
							hex::encode(serialize(&deserialized_address, network).unwrap());

						println!("\nDeserialized Address {:?}\n", deserialized_address.0);
						println!("\nSerialized Address {:?}\n", address);

						std::fs::write(&dest_file, address.as_bytes())?;
						Ok(())
					},
				}
			})
	}));

	// Pass through standard `Error`s or transform panics into `Error`
	match result {
		Ok(inner_result) => inner_result,
		Err(panic_info) => {
			let msg = match panic_info.downcast_ref::<&str>() {
				Some(s) => s.to_string(),
				None => match panic_info.downcast_ref::<String>() {
					Some(s) => s.clone(),
					None => "Unknown panic".to_string(),
				},
			};
			let err: Box<dyn std::error::Error> = Box::new(PanicError(msg));
			Err(err)
		},
	}
}

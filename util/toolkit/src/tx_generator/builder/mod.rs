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

use async_trait::async_trait;
use builders::{DoNothingBuilder, compute_batches_seeds};
use clap::{Args, Subcommand, ValueEnum};
pub use midnight_node_ledger_helpers::CoinSelectionStrategy;
use midnight_node_ledger_helpers::fork::{
	fork_aware_context::{
		ForkAwareLedgerContext, apply_block_7, apply_block_8, apply_block_9,
		block_context_from_raw_7, block_context_from_raw_8, block_context_from_raw_9,
		fork_context_7_to_8,
	},
	raw_block_data::{LedgerVersion, RawBlockData},
};
use midnight_node_ledger_helpers::*;
use serde::Deserialize;
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
	sync::Arc,
};

use crate::{
	cli_parsers as cli,
	fetcher::{
		fetch_storage::WalletStateCaching, wallet_state_cache,
		wallet_state_cache::CachedWalletState,
	},
	serde_def::SourceTransactions,
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use subxt::utils::H256;

pub mod builders;

pub const FUNDING_SEED: &str = "0000000000000000000000000000000000000000000000000000000000000001";

/// Toolkit-local mirror of the ledger's `ClaimKind`, used so the CLI can expose a
/// `--claim-kind` selector via clap's `ValueEnum` without depending on a specific
/// ledger version's type. Each version builder converts this into its own `ClaimKind`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[clap(rename_all = "kebab-case")]
pub enum ClaimKindArg {
	/// Claim block-production rewards (the historical default).
	#[default]
	Reward,
	/// Claim mNIGHT bridged from Cardano via the protocol bridge.
	CardanoBridge,
}

#[derive(Args, Clone, Debug)]
pub struct ClaimRewardsArgs {
	/// Fee-payer seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+), e.g. `--funding-seed ecdsa:<seed>`.
	#[arg(long, default_value = FUNDING_SEED, value_parser = cli::scheme_seed_decode)]
	pub funding_seed: cli::SchemeSeed,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Amount for the claim mint
	#[arg(long, short, default_value_t = 500_000)]
	pub amount: u128,
	/// Which kind of claim to issue: `reward` (block rewards) or
	/// `cardano-bridge` (mNIGHT bridged from Cardano via the c2m protocol bridge).
	#[arg(long, value_enum, default_value_t = ClaimKindArg::Reward)]
	pub claim_kind: ClaimKindArg,
}

#[derive(Args, Clone, Debug)]
pub struct ContractDeployArgs {
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	/// Seed for the contract committee. Accepts multiple
	#[arg(long = "authority-seed", value_parser = cli::wallet_seed_decode)]
	pub authority_seeds: Vec<WalletSeed>,
	/// Authority committee threshold. Default == authority_seeds.len()
	#[arg(long)]
	pub authority_threshold: Option<u32>,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Args, Clone, Debug)]
pub struct CustomContractArgs {
	/// Seed for the random number generator. Defaults to entropy source
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Seed for funding the transactions
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	/// The directory containing directories with key files for the Resolver. Accepts multiple
	#[arg(short, long = "compiled-contract-dir")]
	pub compiled_contract_dirs: Vec<String>,
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
	/// Seed for the current contract authority. Accepts multiple
	#[arg(long = "authority-seed", value_parser = cli::wallet_seed_decode)]
	pub authority_seeds: Vec<WalletSeed>,
	/// Seed for the new authority. Accepts multiple
	#[arg(long = "new-authority-seed", value_parser = cli::wallet_seed_decode)]
	pub new_authority_seeds: Vec<WalletSeed>,
	/// File to read the contract address from
	#[arg(long, value_parser = cli::contract_address_decode)]
	pub contract_address: ContractAddress,
	/// Threshold for Maintenance ReplaceAthority
	#[arg(long)]
	pub threshold: Option<u32>,
	/// Path to verifier key for Contract entrypoint to update/insert. Accepts multiple
	#[arg(long = "upsert-entrypoint")]
	pub upsert_entrypoints: Vec<PathBuf>,
	/// Name of Contract entrypoint to remove. Accepts multiple
	#[arg(long = "remove-entrypoint")]
	pub remove_entrypoints: Vec<String>,
	/// Counter for Maintenance ReplaceAthority
	#[arg(long, default_value = "0")]
	pub counter: u32,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Args, Clone, Debug)]
pub struct BatchesArgs {
	/// Fee-payer seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+), e.g. `--funding-seed ecdsa:<seed>`.
	#[arg(long, default_value = FUNDING_SEED, value_parser = cli::scheme_seed_decode)]
	pub funding_seed: cli::SchemeSeed,
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
	/// Strategy for ordering candidate coins/UTXOs during input selection.
	/// `largest-first` minimizes the number of inputs; `smallest-first` consolidates dust.
	#[arg(long, value_parser = cli::coin_selection_strategy, default_value = "largest-first")]
	pub coin_selection: CoinSelectionStrategy,
}

#[derive(Args, Clone, Debug)]
pub struct SingleTxArgs {
	/// Per-destination output spec. Repeatable. Bundles the address, amount,
	/// and (optional) token type for one destination in a single argument.
	///
	/// Format:
	///   `addr=<bech32_address>,amount=<u128>[,token=<32-byte-hex>]`
	///
	/// The address HRP picks the side (shielded vs unshielded). If `token`
	/// is omitted, it defaults to the all-zeros token type. Cannot be mixed
	/// with `--destination-address` / `--*-amount` / `--*-token-type` in the
	/// same invocation.
	#[arg(long = "output", value_parser = cli::output_arg_decode)]
	pub outputs: Vec<cli::OutputArg>,
	/// Amount(s) to send to shielded destinations.
	///
	/// Provide once to broadcast the same amount to every shielded destination,
	/// or repeat once per shielded destination (in the order they appear in
	/// `--destination-address`) for per-destination amounts.
	#[arg(long)]
	pub shielded_amount: Vec<u128>,
	/// Token type(s) for shielded destinations.
	///
	/// Same broadcast / per-destination semantics as `--shielded-amount`. If
	/// omitted, defaults to the all-zeros token type and broadcasts to every
	/// shielded destination.
	#[arg(
		long,
		value_parser = cli::token_decode::<ShieldedTokenType>,
	)]
	pub shielded_token_type: Vec<ShieldedTokenType>,
	/// Amount(s) to send to unshielded destinations. Same broadcast /
	/// per-destination semantics as `--shielded-amount`.
	#[arg(long)]
	pub unshielded_amount: Vec<u128>,
	/// Token type(s) for unshielded destinations. Same broadcast /
	/// per-destination semantics as `--shielded-token-type`.
	#[arg(
		long,
		value_parser = cli::token_decode::<UnshieldedTokenType>,
	)]
	pub unshielded_token_type: Vec<UnshieldedTokenType>,
	/// Source wallet seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+), e.g. `--source-seed ecdsa:<seed>`.
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	pub source_seed: cli::SchemeSeed,
	/// Funding seed for transaction. If not set, uses source_seed. Bare seed selects Schnorr;
	/// prefix with `ecdsa:` for an ECDSA identity (ledger 9+).
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	pub funding_seed: Option<cli::SchemeSeed>,
	/// Destination address, both shielded and unshielded. Used together with
	/// `--*-amount` / `--*-token-type` flags. Either this or `--output` must
	/// be provided, but not both.
	#[arg(long)]
	pub destination_address: Vec<WalletAddress>,
	/// Pin specific wallet UTXOs as inputs to the unshielded transfer. Format:
	/// <intent_hash_hex>#<output_no>, e.g. abc123…#0. Repeatable. When set, the
	/// toolkit skips its built-in coin selection and uses exactly these UTXOs;
	/// their summed value must be >= the total of `--unshielded-amount` across
	/// destinations of the same token type. Only valid when exactly one
	/// unshielded token type is used.
	#[arg(long = "input-utxo", value_parser = cli::utxo_id_decode)]
	pub input_utxos: Vec<UtxoId>,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
	/// Strategy for ordering candidate coins/UTXOs during input selection.
	/// `largest-first` minimizes the number of inputs; `smallest-first` consolidates dust.
	#[arg(long, value_parser = cli::coin_selection_strategy, default_value = "largest-first")]
	pub coin_selection: CoinSelectionStrategy,
}
#[derive(Args, Clone, Debug)]
pub struct RegisterDustAddressArgs {
	/// Wallet seed to register. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA
	/// identity (ledger 9+), e.g. `--wallet-seed ecdsa:<seed>`.
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	pub wallet_seed: cli::SchemeSeed,
	/// Seed for funding wallet. If not provided, uses retroactive DUST from NIGHT UTXOs. Bare
	/// seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity (ledger 9+).
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	pub funding_seed: Option<cli::SchemeSeed>,
	#[arg(
		long,
		value_parser = cli::wallet_address,
	)]
	pub destination_dust: Option<WalletAddress>,
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Args, Clone, Debug)]
pub struct DeregisterDustAddressArgs {
	/// Wallet seed to deregister. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA
	/// identity (ledger 9+), e.g. `--wallet-seed ecdsa:<seed>`.
	#[arg(long, value_parser = cli::scheme_seed_decode)]
	pub wallet_seed: cli::SchemeSeed,
	/// Fee-payer seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+), e.g. `--funding-seed ecdsa:<seed>`.
	#[arg(long, default_value = FUNDING_SEED, value_parser = cli::scheme_seed_decode)]
	pub funding_seed: cli::SchemeSeed,
	/// RNG seed for deterministic transaction generation (32 bytes hex)
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TransferSpec {
	/// Source wallet seed. Bare seed selects Schnorr; prefix with `ecdsa:` for an ECDSA identity
	/// (ledger 9+), e.g. `"ecdsa:<seed>"`.
	pub source_seed: cli::SchemeSeed,
	pub destination_address: String,
	pub unshielded_amount: Option<u128>,
	pub unshielded_token_type: Option<String>,
	pub shielded_amount: Option<u128>,
	pub shielded_token_type: Option<String>,
	/// Fee-payer seed. Absent means the source seed funds the tx. Bare seed selects Schnorr;
	/// prefix with `ecdsa:` for an ECDSA identity (ledger 9+).
	pub funding_seed: Option<cli::SchemeSeed>,
	pub rng_seed: Option<String>,
}

impl TransferSpec {
	/// The source NIGHT identity and its unshielded signature scheme.
	pub fn resolve_source(&self) -> (WalletSeed, UnshieldedSignatureScheme) {
		self.source_seed.resolve()
	}

	/// The optional fee-payer NIGHT identity: `None` means the source seed funds the tx.
	pub fn resolve_funding(&self) -> Option<(WalletSeed, UnshieldedSignatureScheme)> {
		self.funding_seed.as_ref().map(cli::SchemeSeed::resolve)
	}
}

#[derive(Args, Clone, Debug)]
#[group(required = true, multiple = false)]
pub struct TransferArgs {
	/// Path to JSON file with transfer specifications
	#[arg(long)]
	pub transfers_file: Option<String>,
	/// Transfer specifications, provided as in-line JSON
	#[arg(long, value_parser = cli::serde_json_decode::<Vec<TransferSpec>>)]
	pub transfers: Option<Vec<TransferSpec>>,
}

#[derive(Args, Clone, Debug)]
pub struct BatchSingleTxArgs {
	#[command(flatten)]
	pub transfers: TransferArgs,
	/// Number of concurrent tx generation tasks (default: available CPUs)
	#[arg(long)]
	pub concurrency: Option<usize>,
	/// Strategy for ordering candidate coins/UTXOs during input selection.
	/// `largest-first` minimizes the number of inputs; `smallest-first` consolidates dust.
	#[arg(long, value_parser = cli::coin_selection_strategy, default_value = "largest-first")]
	pub coin_selection: CoinSelectionStrategy,
}

impl BatchSingleTxArgs {
	pub fn get_transfer_specs(&self) -> Vec<TransferSpec> {
		if let Some(ref transfers_file) = self.transfers.transfers_file {
			let file_content = std::fs::read_to_string(&transfers_file).unwrap_or_else(|e| {
				panic!("failed to read transfers file '{}': {}", transfers_file, e)
			});
			serde_json::from_str(&file_content)
				.unwrap_or_else(|e| panic!("failed to parse transfers JSON: {}", e))
		} else {
			// unwrap() is safe here - must be Some(_) if transfers_file is None
			self.transfers.transfers.clone().unwrap()
		}
	}
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
	/// Claim block rewards or tokens made claimable by the protocol bridge
	ClaimRewards(ClaimRewardsArgs),
	/// Send a single transaction with one-or-many outputs across shielded
	/// and/or unshielded destinations, optionally mixing multiple token types
	/// in one tx.
	#[clap(long_about = "\
Send a single transaction with one-or-many outputs across shielded and/or \
unshielded destinations, optionally mixing multiple token types in one tx.

Two CLI shapes are supported. Pick one per invocation; mixing them is rejected:

  (A) --output (recommended): one flag per destination, bundling the triple
      (address, amount, token type) in a single argument.
        --output addr=<bech32>,amount=<u128>[,token=<32-byte-hex>]
      Each occurrence is one tx output. The address HRP picks the side
      (shielded vs unshielded). `token` is optional and defaults to the
      all-zeros token type (NIGHT).

  (B) --destination-address + per-side --*-amount / --*-token-type: each
      side accepts parallel lists. Provide a flag once on a side to broadcast
      it to every destination on that side, or once per destination on that
      side to align by command-line order. Omit --*-token-type to default to
      the all-zeros token type.

Examples:

  # (A) Mixed-token tx with one unshielded NIGHT output and one shielded output:
  midnight-node-toolkit generate-txs single-tx \\
    --source-seed <SEED> \\
    --output addr=mn_addr1...,amount=410000000,token=0000...0000 \\
    --output addr=mn_shield-addr1...,amount=41,token=0000...0001

  # (A) Token omitted -> defaults to all-zeros:
  midnight-node-toolkit generate-txs single-tx \\
    --source-seed <SEED> \\
    --output addr=mn_addr1...,amount=100

  # (B) Two unshielded destinations, same token type and amount (broadcast):
  midnight-node-toolkit generate-txs single-tx \\
    --source-seed <SEED> \\
    --unshielded-amount 100 \\
    --destination-address mn_addr1...A \\
    --destination-address mn_addr1...B

  # (B) Two unshielded destinations, different amounts and token types (per-destination):
  midnight-node-toolkit generate-txs single-tx \\
    --source-seed <SEED> \\
    --destination-address mn_addr1...A \\
    --unshielded-amount 100 \\
    --unshielded-token-type 0000...0000 \\
    --destination-address mn_addr1...B \\
    --unshielded-amount 250 \\
    --unshielded-token-type 0000...0001

Notes:
  * --input-utxo is only supported when exactly one unshielded token type is used.
  * In shape (B), mismatched flag counts (e.g. 3 destinations on a side but 2 amounts) are rejected with a clear error.
")]
	SingleTx(SingleTxArgs),
	/// Register a DUST address for the wallet
	RegisterDustAddress(RegisterDustAddressArgs),
	/// Deregister (unlink) a DUST address for the wallet
	DeregisterDustAddress(DeregisterDustAddressArgs),
	/// Build multiple single-output txs from a JSON transfer spec file (one process, shared context)
	BatchSingleTx(BatchSingleTxArgs),
	/// Send is a no-op here (source is sent directly to destination)
	Send,
}

/// Configuration for how proofs should be generated.
#[derive(Clone, Debug)]
pub enum ProverConfig {
	Local,
	Remote(String),
}

/// Error when constructing a versioned builder.
#[derive(Debug, thiserror::Error)]
pub enum BuilderConstructionError {
	#[error("remote prover is not supported for ledger 7")]
	RemoteProverNotSupportedForLedger7,
	#[error("{0} builder is not supported for ledger 7")]
	NotSupportedForLedger7(&'static str),
	#[error(
		"ECDSA unshielded (NIGHT) signatures are only supported from ledger 9; the source chain is \
		 on {0:?}. Use a bare or `schnorr:`-prefixed seed (--seed / --source-seed / --wallet-seed / \
		 --funding-seed) instead of an `ecdsa:`-prefixed one."
	)]
	EcdsaNotSupportedForLedger(LedgerVersion),
	#[error("chain has not reached any known ledger version")]
	NoContext,
	#[error("internal error: version mismatch in fork context")]
	VersionMismatch,
}

impl From<BuilderConstructionError> for DynamicError {
	fn from(e: BuilderConstructionError) -> Self {
		Self { error: Box::new(e) }
	}
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

impl From<ContextNotLedger8Error> for DynamicError {
	fn from(e: ContextNotLedger8Error) -> Self {
		Self { error: Box::new(e) }
	}
}

#[async_trait]
impl<T: BuildTxs + Send + Sync> BuildTxs for DynamicTransactionBuilder<T> {
	type Error = DynamicError;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		self.builder
			.build_txs_from(received_tx)
			.await
			.map_err(|e| DynamicError { error: Box::new(e) })
	}
}

impl Builder {
	/// Extract wallet seeds needed by this builder configuration, without constructing
	/// the full builder (which requires context/prover). Returns empty for pass-through builders.
	///
	/// Seeds are resolved from each command's `--…-seed` value (a bare/`schnorr:`/`ecdsa:`-prefixed
	/// [`cli::SchemeSeed`]); the scheme itself is dropped here — see [`Self::relevant_wallet_schemes`]
	/// for the companion scheme map, which must decode the *same* resolved seed values so the two
	/// line up by key).
	pub fn relevant_wallet_seeds(&self) -> Result<Vec<WalletSeed>, &'static str> {
		match self {
			Builder::Batches(args) => {
				let (funding, _) = args.funding_seed.resolve();
				compute_batches_seeds(&funding, args.num_txs_per_batch, args.num_batches)
			},
			Builder::ContractSimple(call) => {
				let seed_str = match call {
					ContractCall::Deploy(args) => &args.funding_seed,
					ContractCall::Call(args) => &args.funding_seed,
					ContractCall::Maintenance(args) => &args.funding_seed,
				};
				Ok(vec![Wallet::<DefaultDB>::wallet_seed_decode(seed_str)])
			},
			Builder::ContractCustom(args) => {
				Ok(vec![Wallet::<DefaultDB>::wallet_seed_decode(&args.funding_seed)])
			},
			Builder::ClaimRewards(args) => {
				let (funding, _) = args.funding_seed.resolve();
				Ok(vec![funding])
			},
			Builder::SingleTx(args) => {
				let (source, _) = args.source_seed.resolve();
				let mut seeds = vec![source];
				if let Some((funding, _)) = args.funding_seed.as_ref().map(cli::SchemeSeed::resolve)
				{
					seeds.push(funding);
				}
				Ok(seeds)
			},
			Builder::RegisterDustAddress(args) => {
				let (wallet_seed, _) = args.wallet_seed.resolve();
				if let Some((funding, _)) = args.funding_seed.as_ref().map(cli::SchemeSeed::resolve)
				{
					Ok(vec![wallet_seed, funding])
				} else {
					Ok(vec![wallet_seed])
				}
			},
			Builder::DeregisterDustAddress(args) => {
				let (wallet_seed, _) = args.wallet_seed.resolve();
				let (funding, _) = args.funding_seed.resolve();
				Ok(vec![wallet_seed, funding])
			},
			Builder::BatchSingleTx(args) => {
				let specs = args.get_transfer_specs();
				let mut seen = HashSet::new();
				let mut seeds = Vec::new();
				for spec in &specs {
					let (source, _) = spec.resolve_source();
					if seen.insert(source.clone()) {
						seeds.push(source);
					}
					if let Some((funding, _)) = spec.resolve_funding() {
						if seen.insert(funding.clone()) {
							seeds.push(funding);
						}
					}
				}
				Ok(seeds)
			},
			Builder::Send => Ok(vec![]),
		}
	}

	/// Companion to [`Self::relevant_wallet_seeds`]: map each *resolved* seed to its unshielded
	/// signature scheme. Only ECDSA seeds get an entry — seeds absent from the map default to
	/// Schnorr via [`scheme_of`], so a pure-Schnorr configuration returns an empty map (matching
	/// the pre-ECDSA behaviour). Keys are decoded identically to `relevant_wallet_seeds` so the two
	/// stay aligned.
	///
	/// Rejects a seed that is requested under both schemes within the same build (e.g.
	/// `--source-seed X --funding-seed-ecdsa X`, or two batch-transfer specs referring to `X` with
	/// different schemes): since the context/cache plumbing keys wallets by seed alone, silently
	/// collapsing such a seed to a single scheme would build/sign with the wrong identity.
	///
	/// Committee/contract seeds (`ContractSimple`, `ContractCustom`) and the batch *output* seeds
	/// stay Schnorr and are intentionally omitted here (out of scope for ECDSA).
	pub fn relevant_wallet_schemes(&self) -> Result<WalletSchemes, &'static str> {
		let mut schemes = WalletSchemes::new();
		let mut seen: HashMap<WalletSeed, UnshieldedSignatureScheme> = HashMap::new();
		let mut mark = |seed: WalletSeed,
		                scheme: UnshieldedSignatureScheme|
		 -> Result<(), &'static str> {
			if let Some(previous) = seen.insert(seed.clone(), scheme) {
				if previous != scheme {
					return Err(
						"the same seed was requested under both Schnorr and ECDSA schemes in one build; each seed must use a single scheme",
					);
				}
				return Ok(());
			}
			if scheme == UnshieldedSignatureScheme::Ecdsa {
				schemes.insert(seed, scheme);
			}
			Ok(())
		};
		match self {
			Builder::Batches(args) => {
				let (funding, scheme) = args.funding_seed.resolve();
				mark(funding, scheme)?;
			},
			Builder::ClaimRewards(args) => {
				let (funding, scheme) = args.funding_seed.resolve();
				mark(funding, scheme)?;
			},
			Builder::SingleTx(args) => {
				let (source, source_scheme) = args.source_seed.resolve();
				mark(source, source_scheme)?;
				if let Some((funding, funding_scheme)) =
					args.funding_seed.as_ref().map(cli::SchemeSeed::resolve)
				{
					mark(funding, funding_scheme)?;
				}
			},
			Builder::RegisterDustAddress(args) => {
				let (wallet_seed, wallet_scheme) = args.wallet_seed.resolve();
				mark(wallet_seed, wallet_scheme)?;
				if let Some((funding, funding_scheme)) =
					args.funding_seed.as_ref().map(cli::SchemeSeed::resolve)
				{
					mark(funding, funding_scheme)?;
				}
			},
			Builder::DeregisterDustAddress(args) => {
				let (wallet_seed, wallet_scheme) = args.wallet_seed.resolve();
				mark(wallet_seed, wallet_scheme)?;
				let (funding, funding_scheme) = args.funding_seed.resolve();
				mark(funding, funding_scheme)?;
			},
			Builder::BatchSingleTx(args) => {
				for spec in &args.get_transfer_specs() {
					let (source, source_scheme) = spec.resolve_source();
					mark(source, source_scheme)?;
					if let Some((funding, funding_scheme)) = spec.resolve_funding() {
						mark(funding, funding_scheme)?;
					}
				}
			},
			Builder::ContractSimple(_) | Builder::ContractCustom(_) | Builder::Send => {},
		}
		Ok(schemes)
	}

	/// Construct a versioned builder for the appropriate ledger version.
	///
	/// Dispatches on `fork_ctx.version()`:
	/// - Ledger8 → builds with ledger_8 types
	/// - Ledger7 → builds with ledger_7 types (errors if remote prover requested)
	/// - None (pass-through builders) → defaults to ledger_8
	pub fn to_versioned_builder(
		self,
		fork_ctx: Option<ForkAwareLedgerContext>,
		prover_config: &ProverConfig,
		_dry_run: bool,
	) -> Result<Box<dyn BuildTxs<Error = DynamicError>>, BuilderConstructionError> {
		match fork_ctx {
			Some(ctx) => {
				let self_clone = self.clone();
				ctx.dispatch(
					|context| {
						if matches!(prover_config, ProverConfig::Remote(_)) {
							return Err(
								BuilderConstructionError::RemoteProverNotSupportedForLedger7,
							);
						}
						let prover: Arc<
							dyn midnight_node_ledger_helpers::ledger_7::ProofProvider<
									midnight_node_ledger_helpers::ledger_7::DefaultDB,
								>,
						> = Arc::new(midnight_node_ledger_helpers::ledger_7::LocalProofServer::new());
						self_clone.clone().to_builder_v7(Arc::new(context), prover)
					},
					|context| {
						let prover = Self::make_prover_v8(prover_config);
						Ok(self_clone.clone().to_builder_v8(Arc::new(context), prover))
					},
					|context| {
						let prover = Self::make_prover(prover_config);
						Ok(self.to_builder_v9(Arc::new(context), prover))
					},
				)
			},
			None => {
				// Pass-through builder (Send) doesn't need context
				Ok(self.to_builder_passthrough())
			},
		}
	}

	fn make_prover_v8(
		config: &ProverConfig,
	) -> Arc<
		dyn midnight_node_ledger_helpers::ledger_8::ProofProvider<
				midnight_node_ledger_helpers::ledger_8::DefaultDB,
			>,
	> {
		match config {
			ProverConfig::Local => {
				Arc::new(midnight_node_ledger_helpers::ledger_8::LocalProofServer::new())
			},
			ProverConfig::Remote(url) => {
				Arc::new(crate::remote_prover::RemoteProofServer::new(url.clone()))
			},
		}
	}

	fn make_prover(config: &ProverConfig) -> Arc<dyn ProofProvider<DefaultDB>> {
		match config {
			ProverConfig::Local => Arc::new(LocalProofServer::new()),
			ProverConfig::Remote(url) => {
				Arc::new(crate::remote_prover::RemoteProofServer::new(url.clone()))
			},
		}
	}

	fn to_builder_v9(
		self,
		context: Arc<LedgerContext<DefaultDB>>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Box<dyn BuildTxs<Error = DynamicError>> {
		fn constr(
			builder: impl BuildTxs + Send + Sync + 'static,
		) -> Box<dyn BuildTxs<Error = DynamicError>> {
			Box::new(DynamicTransactionBuilder { builder })
		}

		use builders::ledger_9 as v9;

		match self {
			Builder::Batches(args) => constr(v9::BatchesBuilder::new(args, context, prover)),
			Builder::ContractSimple(call) => match call {
				ContractCall::Deploy(args) => {
					constr(v9::ContractDeployBuilder::new(args, context, prover))
				},
				ContractCall::Call(args) => {
					constr(v9::ContractCallBuilder::new(args, context, prover))
				},
				ContractCall::Maintenance(args) => {
					constr(v9::ContractMaintenanceBuilder::new(args, context, prover))
				},
			},
			Builder::ContractCustom(args) => {
				constr(v9::CustomContractBuilder::new(args, context, prover))
			},
			Builder::ClaimRewards(args) => {
				constr(v9::ClaimRewardsBuilder::new(args, context, prover))
			},
			Builder::SingleTx(args) => {
				constr(v9::single_tx::SingleTxBuilder::new(args, context, prover))
			},
			Builder::RegisterDustAddress(args) => {
				constr(v9::RegisterDustAddressBuilder::new(args, context, prover))
			},
			Builder::DeregisterDustAddress(args) => {
				constr(v9::DeregisterDustAddressBuilder::new(args, context, prover))
			},
			Builder::BatchSingleTx(args) => {
				constr(v9::batch_single_tx::BatchSingleTxBuilder::new(args, context, prover))
			},
			Builder::Send => constr(v9::DoNothingBuilder::new()),
		}
	}

	fn to_builder_v8(
		self,
		context: Arc<
			midnight_node_ledger_helpers::ledger_8::context::LedgerContext<
				midnight_node_ledger_helpers::ledger_8::DefaultDB,
			>,
		>,
		prover: Arc<
			dyn midnight_node_ledger_helpers::ledger_8::ProofProvider<
					midnight_node_ledger_helpers::ledger_8::DefaultDB,
				>,
		>,
	) -> Box<dyn BuildTxs<Error = DynamicError>> {
		fn constr(
			builder: impl BuildTxs + Send + Sync + 'static,
		) -> Box<dyn BuildTxs<Error = DynamicError>> {
			Box::new(DynamicTransactionBuilder { builder })
		}

		use builders::ledger_8 as v8;

		match self {
			Builder::Batches(args) => constr(v8::BatchesBuilder::new(args, context, prover)),
			Builder::ContractSimple(call) => match call {
				ContractCall::Deploy(args) => {
					constr(v8::ContractDeployBuilder::new(args, context, prover))
				},
				ContractCall::Call(args) => {
					constr(v8::ContractCallBuilder::new(args, context, prover))
				},
				ContractCall::Maintenance(args) => {
					constr(v8::ContractMaintenanceBuilder::new(args, context, prover))
				},
			},
			Builder::ContractCustom(args) => {
				constr(v8::CustomContractBuilder::new(args, context, prover))
			},
			Builder::ClaimRewards(args) => {
				constr(v8::ClaimRewardsBuilder::new(args, context, prover))
			},
			Builder::SingleTx(args) => {
				constr(v8::single_tx::SingleTxBuilder::new(args, context, prover))
			},
			Builder::RegisterDustAddress(args) => {
				constr(v8::RegisterDustAddressBuilder::new(args, context, prover))
			},
			Builder::DeregisterDustAddress(args) => {
				constr(v8::DeregisterDustAddressBuilder::new(args, context, prover))
			},
			Builder::BatchSingleTx(args) => {
				constr(v8::batch_single_tx::BatchSingleTxBuilder::new(args, context, prover))
			},
			Builder::Send => constr(v8::DoNothingBuilder::new()),
		}
	}

	fn to_builder_v7(
		self,
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
	) -> Result<Box<dyn BuildTxs<Error = DynamicError>>, BuilderConstructionError> {
		fn constr(
			builder: impl BuildTxs + Send + Sync + 'static,
		) -> Box<dyn BuildTxs<Error = DynamicError>> {
			Box::new(DynamicTransactionBuilder { builder })
		}

		use builders::ledger_7 as v7;

		Ok(match self {
			Builder::Batches(args) => constr(v7::BatchesBuilder::new(args, context, prover)),
			Builder::ContractSimple(call) => match call {
				ContractCall::Deploy(args) => {
					constr(v7::ContractDeployBuilder::new(args, context, prover))
				},
				ContractCall::Call(args) => {
					constr(v7::ContractCallBuilder::new(args, context, prover))
				},
				ContractCall::Maintenance(args) => {
					constr(v7::ContractMaintenanceBuilder::new(args, context, prover))
				},
			},
			Builder::ContractCustom(args) => {
				constr(v7::CustomContractBuilder::new(args, context, prover))
			},
			Builder::BatchSingleTx(_) => {
				return Err(BuilderConstructionError::NotSupportedForLedger7("batch-single-tx"));
			},
			Builder::ClaimRewards(args) => {
				constr(v7::ClaimRewardsBuilder::new(args, context, prover))
			},
			Builder::SingleTx(args) => {
				constr(v7::single_tx::SingleTxBuilder::new(args, context, prover))
			},
			Builder::RegisterDustAddress(args) => {
				constr(v7::RegisterDustAddressBuilder::new(args, context, prover))
			},
			Builder::DeregisterDustAddress(args) => {
				constr(v7::DeregisterDustAddressBuilder::new(args, context, prover))
			},
			Builder::Send => constr(DoNothingBuilder::new()),
		})
	}

	fn to_builder_passthrough(self) -> Box<dyn BuildTxs<Error = DynamicError>> {
		fn constr(
			builder: impl BuildTxs + Send + Sync + 'static,
		) -> Box<dyn BuildTxs<Error = DynamicError>> {
			Box::new(DynamicTransactionBuilder { builder })
		}

		match self {
			Builder::Send => constr(DoNothingBuilder::new()),
			other => panic!("builder {:?} requires context but none was provided", other),
		}
	}
}

#[async_trait]
pub trait BuildTxs {
	type Error: std::error::Error + Send + Sync + 'static;

	/// Build transactions from source data.
	/// Context and prover are stored in the builder itself.
	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error>;
}

/// One-liner replacement for the repeated `Instant::now()` / `elapsed()` pattern.
macro_rules! timed {
	($label:expr, $expr:expr) => {{
		let __t = std::time::Instant::now();
		let __result = $expr;
		log::debug!("[perf] {} took {:?}", $label, __t.elapsed());
		__result
	}};
}

/// Per-seed unshielded signature scheme for context/cache building. Seeds absent from the map
/// resolve to Schnorr (the default), so the empty map reproduces the pre-ECDSA behaviour.
pub type WalletSchemes = HashMap<WalletSeed, UnshieldedSignatureScheme>;

/// Resolve the scheme for `seed`, defaulting to Schnorr.
fn scheme_of(schemes: &WalletSchemes, seed: &WalletSeed) -> UnshieldedSignatureScheme {
	schemes.get(seed).copied().unwrap_or_default()
}

/// Reject ECDSA seeds on a pre-ledger-9 source with a clear CLI error, rather than letting the
/// loud panic fire deep in [`ForkAwareLedgerContext::new_from_wallet_seeds_with_schemes`]. Returns
/// `Ok(())` when no ECDSA seed is present, or when the source has already reached ledger 9.
///
/// Callers must pass the source's *initial* ledger version (`SourceTransactions::ledger_version()`)
/// — the same version the cold-path context is built at, which is where the ledger-level guard
/// asserts.
pub fn ensure_ecdsa_supported(
	ledger_version: LedgerVersion,
	schemes: &WalletSchemes,
) -> Result<(), BuilderConstructionError> {
	if ledger_version != LedgerVersion::Ledger9
		&& schemes.values().any(|scheme| *scheme == UnshieldedSignatureScheme::Ecdsa)
	{
		return Err(BuilderConstructionError::EcdsaNotSupportedForLedger(ledger_version));
	}
	Ok(())
}

/// Load per-wallet cache entries and partition into uncached seeds and cached (seed, state) pairs.
/// Cached pairs are sorted by block height for two-pointer replay.
async fn load_and_partition_cache(
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	storage: &dyn WalletStateCaching,
	schemes: &WalletSchemes,
) -> (Vec<WalletSeed>, Vec<(WalletSeed, CachedWalletState)>) {
	let seed_hashes: Vec<H256> = wallet_seeds
		.iter()
		.map(|seed| wallet_state_cache::wallet_cache_key(seed, scheme_of(schemes, seed)))
		.collect();
	let raw_cached = timed!(
		"storage.get_wallet_states",
		storage.get_wallet_states(chain_id, &seed_hashes).await
	);

	let mut uncached_seeds: Vec<WalletSeed> = Vec::new();
	let mut cached: Vec<(WalletSeed, CachedWalletState)> = Vec::new();
	for (seed, cached_state) in wallet_seeds.iter().zip(raw_cached) {
		match cached_state {
			Some(state) => cached.push((seed.clone(), state)),
			None => uncached_seeds.push(seed.clone()),
		}
	}
	cached.sort_by_key(|(_, ws)| ws.block_height);

	(uncached_seeds, cached)
}

/// Inject a batch of cached wallets into a ledger context. Panics on failure (corrupted cache).
fn inject_cached_wallets(
	ctx: &LedgerContext<DefaultDB>,
	wallets: &[(WalletSeed, CachedWalletState)],
	ledger_state: &LedgerState<DefaultDB>,
	at_height: u64,
	schemes: &WalletSchemes,
) {
	for (seed, state) in wallets {
		let scheme = scheme_of(schemes, seed);
		wallet_state_cache::inject_wallet_from_cache(ctx, state, seed, scheme, ledger_state)
			.unwrap_or_else(|e| {
				panic!(
					"failed to inject wallet at height {}: {} — clear caches and retry",
					at_height, e
				)
			});
	}
}

/// Create the initial fork-aware context, either cold (genesis) or warm (snapshot restore).
async fn initialize_context(
	received_tx: &SourceTransactions,
	uncached_seeds: &[WalletSeed],
	start_height: u64,
	storage: &dyn WalletStateCaching,
	chain_id: H256,
	schemes: &WalletSchemes,
) -> ForkAwareLedgerContext {
	if start_height == 0 {
		let seeds_with_schemes: Vec<(WalletSeed, UnshieldedSignatureScheme)> = uncached_seeds
			.iter()
			.map(|seed| (seed.clone(), scheme_of(schemes, seed)))
			.collect();
		timed!(
			"new_from_wallet_seeds (cold)",
			ForkAwareLedgerContext::new_from_wallet_seeds_with_schemes(
				received_tx.ledger_version(),
				&received_tx.network_id,
				&seeds_with_schemes,
			)
		)
	} else {
		let snapshot = timed!(
			"storage.get_ledger_snapshot",
			storage.get_ledger_snapshot(chain_id, start_height).await
		)
		.unwrap_or_else(|| {
			panic!("ledger snapshot missing at height {} — clear caches and retry", start_height)
		});

		let (ctx, _, _) = timed!(
			"restore_context_from_ledger_snapshot",
			wallet_state_cache::restore_context_from_ledger_snapshot(&snapshot)
		)
		.unwrap_or_else(|e| {
			panic!(
				"failed to restore ledger snapshot at height {}: {} — clear caches and retry",
				start_height, e
			)
		});

		ForkAwareLedgerContext::Ledger9(ctx)
	}
}

type Db7 = midnight_node_ledger_helpers::ledger_7::DefaultDB;
type Db8 = midnight_node_ledger_helpers::ledger_8::DefaultDB;
type Db9 = midnight_node_ledger_helpers::ledger_9::DefaultDB;

const DUST_BATCH_SIZE: usize = 1000;

/// Interval between info-level "replay progress: …" log lines emitted from
/// `replay_blocks_{7,8}`. Fine-grained per-batch progress remains at
/// `log::debug!`; this throttle is what users see by default during a
/// multi-hour replay so it doesn't look like the process has hung.
const REPLAY_INFO_HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(30);

fn replay_blocks_7(
	ctx: &midnight_node_ledger_helpers::ledger_7::context::LedgerContext<Db7>,
	blocks_sorted_by_height: &[RawBlockData],
) {
	let mut events: Vec<midnight_node_ledger_helpers::ledger_7::Event<Db7>> = Vec::new();
	let total = blocks_sorted_by_height.len();
	let mut last_info_at = std::time::Instant::now();

	for (i, block) in blocks_sorted_by_height.iter().enumerate() {
		events.extend(apply_block_7(ctx, block));

		let is_last = i + 1 == total;
		if events.len() >= DUST_BATCH_SIZE || is_last {
			ctx.update_dust_from_events(events.as_slice());
			events.clear();
			log::debug!("[perf] replay_blocks_7 progress: {}/{} blocks", i + 1, total);
		}

		// Heartbeat lives outside the flush branch so a long stretch of
		// blocks with no dust events still gets a "still alive" signal.
		// Inside the flush branch this would only fire on `DUST_BATCH_SIZE`
		// or `is_last`, which on sparse chains can be far apart.
		if last_info_at.elapsed() >= REPLAY_INFO_HEARTBEAT {
			log::info!(
				"replay progress: {}/{} blocks ({:.1}%)",
				i + 1,
				total,
				(i + 1) as f64 / total as f64 * 100.0,
			);
			last_info_at = std::time::Instant::now();
		}
	}

	if let Some(block) = blocks_sorted_by_height.last() {
		ctx.update_dust_from_block(&block_context_from_raw_7(block));
	}
}

fn replay_blocks_8(
	ctx: &midnight_node_ledger_helpers::ledger_8::context::LedgerContext<Db8>,
	blocks_sorted_by_height: &[RawBlockData],
) {
	let mut events: Vec<midnight_node_ledger_helpers::ledger_8::Event<Db8>> = Vec::new();

	let total = blocks_sorted_by_height.len();

	for (i, block) in blocks_sorted_by_height.iter().enumerate() {
		events.extend(apply_block_8(ctx, block));

		let is_last = i + 1 == total;
		if events.len() >= DUST_BATCH_SIZE || is_last {
			ctx.update_dust_from_events(events.as_slice());
			events.clear();
			log::debug!("[perf] replay_blocks_8 progress: {}/{} blocks", i + 1, total);
		}
	}

	if let Some(block) = blocks_sorted_by_height.last() {
		ctx.update_dust_from_block(&block_context_from_raw_8(block));
	}
}

fn replay_blocks_9(
	ctx: &midnight_node_ledger_helpers::ledger_9::context::LedgerContext<Db9>,
	blocks_sorted_by_height: &[RawBlockData],
	wallets_sorted_by_height: &[(WalletSeed, CachedWalletState)],
	schemes: &WalletSchemes,
) {
	let mut events: Vec<midnight_node_ledger_helpers::ledger_9::Event<Db9>> = Vec::new();
	let mut remaining = wallets_sorted_by_height;
	let total = blocks_sorted_by_height.len();
	let mut last_info_at = std::time::Instant::now();

	for (i, block) in blocks_sorted_by_height.iter().enumerate() {
		let n = remaining.partition_point(|(_, ws)| ws.block_height < block.number);
		if n > 0 {
			let (to_inject, rest) = remaining.split_at(n);
			if !events.is_empty() {
				ctx.update_dust_from_events(events.as_slice());
				events.clear();
			}
			let ls = ctx.ledger_state.lock().expect("ledger_state lock poisoned").clone();
			inject_cached_wallets(ctx, to_inject, &ls, block.number, schemes);
			remaining = rest;
		}

		events.extend(apply_block_9(ctx, block));

		let is_last = i + 1 == total;
		if events.len() >= DUST_BATCH_SIZE || is_last {
			ctx.update_dust_from_events(events.as_slice());
			events.clear();
			log::debug!("[perf] replay_blocks_8 progress: {}/{} blocks", i + 1, total);
		}

		// See note in `replay_blocks_7`: heartbeat must be evaluated every
		// iteration, not gated on the event-flush condition, so sparse
		// chains still get a "still alive" signal at the 30 s cadence.
		if last_info_at.elapsed() >= REPLAY_INFO_HEARTBEAT {
			log::info!(
				"replay progress: {}/{} blocks ({:.1}%)",
				i + 1,
				total,
				(i + 1) as f64 / total as f64 * 100.0,
			);
			last_info_at = std::time::Instant::now();
		}
	}

	// Inject remaining wallets at the last replayed block height.
	// This handles the case where some wallets are cached at the tip with no new blocks.
	if !remaining.is_empty() {
		let ls = ctx.ledger_state.lock().expect("ledger_state lock poisoned").clone();
		let height = blocks_sorted_by_height.last().map(|b| b.number).unwrap_or(0);
		inject_cached_wallets(ctx, remaining, &ls, height, schemes);
	}

	if let Some(block) = blocks_sorted_by_height.last() {
		ctx.update_dust_from_block(&block_context_from_raw_9(block));
	}
}

/// Replays blocks across a potential Ledger7→Ledger8->Ledger9 fork boundaries,
/// injecting cached wallets at their saved height.
pub(crate) fn replay_blocks(
	fork_ctx: ForkAwareLedgerContext,
	blocks: &[RawBlockData],
	cached: &[(WalletSeed, CachedWalletState)],
	schemes: &WalletSchemes,
) -> ForkAwareLedgerContext {
	if !blocks.is_empty() && !cached.is_empty() {
		log::info!(
			"Replaying {} blocks after cache checkpoint ({}..)",
			blocks.len(),
			blocks.first().map(|b| b.number).unwrap_or(0)
		);
	}

	let t_replay = std::time::Instant::now();

	let fork_7_to_8_idx = blocks.partition_point(|b| b.ledger_version() == LedgerVersion::Ledger7);
	let (l7_blocks, l8_and_l9_blocks) = blocks.split_at(fork_7_to_8_idx);
	let fork_8_to_9_idx =
		l8_and_l9_blocks.partition_point(|b| b.ledger_version() == LedgerVersion::Ledger8);
	let (l8_blocks, l9_blocks) = l8_and_l9_blocks.split_at(fork_8_to_9_idx);

	assert!(
		l9_blocks.is_empty() || (l7_blocks.is_empty() && l8_blocks.is_empty()),
		"chain has Ledger9 blocks and eariler version blocks. This is not supported yet!"
	);

	let result = match fork_ctx {
		ForkAwareLedgerContext::Ledger7(ctx7) => {
			replay_blocks_7(&ctx7, l7_blocks);
			if l8_blocks.is_empty() {
				assert!(cached.is_empty(), "cached wallets with no Ledger8 blocks");
				ForkAwareLedgerContext::Ledger7(ctx7)
			} else {
				let ctx8 = fork_context_7_to_8(ctx7).expect("fork 7 to 8 failed");
				replay_blocks_8(&ctx8, l8_blocks);
				ForkAwareLedgerContext::Ledger8(ctx8)
			}
		},
		ForkAwareLedgerContext::Ledger8(ctx8) => {
			assert!(l7_blocks.is_empty(), "Ledger7 blocks with Ledger8 context");
			replay_blocks_8(&ctx8, l8_blocks);
			ForkAwareLedgerContext::Ledger8(ctx8)
		},
		ForkAwareLedgerContext::Ledger9(ctx9) => {
			assert!(l7_blocks.is_empty(), "Ledger7 blocks with Ledger9 context");
			assert!(l8_blocks.is_empty(), "Ledger8 blocks with Ledger9 context");
			replay_blocks_9(&ctx9, l9_blocks, cached, schemes);
			ForkAwareLedgerContext::Ledger9(ctx9)
		},
	};

	log::debug!("[perf] block replay: {} blocks in {:?}", blocks.len(), t_replay.elapsed());
	result
}

/// Build a fork-aware context with per-wallet state caching.
///
/// Uses deduplicated ledger snapshots (one per block height) and per-wallet cache
/// entries (one per seed). Wallets at different cached heights are caught up via
/// single-pass replay with mid-replay injection (two-pointer merge).
///
/// Caching is skipped when no deterministic chain ID can be derived (e.g. file-loaded
/// datasets with no block #1), to avoid cross-dataset cache collisions.
pub async fn build_fork_aware_context_cached(
	wallet_seeds: &[WalletSeed],
	received_tx: &SourceTransactions,
	cache_storage: Option<&dyn WalletStateCaching>,
) -> ForkAwareLedgerContext {
	build_fork_aware_context_cached_with_schemes(
		wallet_seeds,
		received_tx,
		cache_storage,
		&WalletSchemes::new(),
	)
	.await
}

/// Scheme-aware variant of [`build_fork_aware_context_cached`]. `schemes` maps each seed to its
/// unshielded signature scheme (absent → Schnorr); this determines both the cache key and how
/// wallets are (re)built, so ECDSA identities cache and restore correctly and never collide with
/// their Schnorr counterparts for the same seed.
pub async fn build_fork_aware_context_cached_with_schemes(
	wallet_seeds: &[WalletSeed],
	received_tx: &SourceTransactions,
	cache_storage: Option<&dyn WalletStateCaching>,
	schemes: &WalletSchemes,
) -> ForkAwareLedgerContext {
	if wallet_seeds.is_empty() {
		return build_fork_aware_context_raw_with_schemes(received_tx, wallet_seeds, schemes);
	}
	let Some(chain_id) = received_tx.chain_id() else {
		return build_fork_aware_context_raw_with_schemes(received_tx, wallet_seeds, schemes);
	};
	let Some(storage) = cache_storage else {
		return build_fork_aware_context_raw_with_schemes(received_tx, wallet_seeds, schemes);
	};

	// 1. Load cache and partition wallets.
	let (uncached_seeds, cached) =
		load_and_partition_cache(wallet_seeds, chain_id, storage, schemes).await;

	// 2. Compute start height.
	let start_height = if !uncached_seeds.is_empty() {
		0
	} else {
		cached.first().map(|c| c.1.block_height).unwrap_or(0)
	};

	// 3. Initialize context (cold genesis or warm snapshot restore).
	let fork_ctx =
		initialize_context(received_tx, &uncached_seeds, start_height, storage, chain_id, schemes)
			.await;

	// 4. Determine blocks to replay.
	//
	// Exclude any dust-warp synthetic block from the replay set so the
	// persisted snapshot (step 6) captures the real-head `BlockContext`
	// rather than wall-clock-now. `from_blocks(_, dust_warp = true, _)`
	// appends a synthetic timestamp-only block via
	// `RawBlockData::new_from_timestamp(...)` which hard-codes
	// `number = 0`. If that block is replayed before save, the snapshot's
	// `latest_block_context.tblock` becomes the warp timestamp but the
	// snapshot is keyed at the real chain height; a later run on the
	// same `ledger_state_db` with `dust_warp = false` would then restore
	// the warped context and downstream callers (`register_dust_address`,
	// batch builders) would read warp time even though warping is off.
	//
	// The synthetic is always pushed last by `from_blocks`, so we
	// detect it as last-block-number=0 alongside at least one block
	// with number>0 (guards against legitimate fixture-loaded sources
	// where every block has number=0 — those won't pass the chain_id
	// check anyway, but we double-guard for clarity). We apply it
	// explicitly *after* save as step 7 so the in-memory context for
	// this run reflects the warp.
	let synthetic_dust_warp = received_tx
		.blocks
		.last()
		.filter(|last| last.number == 0 && received_tx.blocks.iter().any(|b| b.number > 0));
	let real_blocks: &[RawBlockData] = if synthetic_dust_warp.is_some() {
		&received_tx.blocks[..received_tx.blocks.len() - 1]
	} else {
		&received_tx.blocks[..]
	};
	// Warm path uses `partition_point` (O(log n) binary search) rather
	// than a linear `.filter()` — `real_blocks` is sorted by `b.number`
	// ascending (the rest of `replay_blocks_*` already relies on this).
	// Cold path takes the whole slice.
	let blocks: &[RawBlockData] = if start_height == 0 {
		real_blocks
	} else {
		let i = real_blocks.partition_point(|b| b.number <= start_height);
		&real_blocks[i..]
	};

	// 5. Replay with mid-replay wallet injection.
	let fork_ctx = replay_blocks(fork_ctx, blocks, &cached, schemes);

	// 6. Save updated cache. `blocks.last()` is sound here because
	// step 4 already excluded the dust-warp synthetic (`number = 0`)
	// from `blocks`; the last entry is the real chain head, and
	// pointer lookup beats an O(n) `max_by_key` on long replays.
	if let Some(final_block) = blocks.last() {
		try_save_cache_v2(&fork_ctx, wallet_seeds, chain_id, final_block.number, storage, schemes)
			.await;
	}

	// 7. Apply the dust-warp synthetic block (in-memory only, post-save).
	//
	// Intentionally runs *after* `try_save_cache_v2`: applying the
	// synthetic overwrites `latest_block_context` with wall-clock-now,
	// and persisting that under the real-head height would surface as a
	// silent warp-leak on later `dust_warp = false` runs against the
	// same `ledger_state_db`. Doing it here keeps the warp in-memory
	// only — the saved snapshot stays clean. Downstream callers in
	// this run (`register_dust_address`, batch builders) read the
	// warped tblock as expected.
	//
	// Mirrors `replay_blocks_{7,8}`'s contract: `apply_block_*` only
	// updates the ledger context (and `latest_block_context`); the
	// per-wallet dust TTL advance lives in `update_dust_from_block`,
	// which `replay_blocks_{7,8}` always calls for the last replayed
	// block (see their final stanzas). Without this second call the
	// warp would advance the *ledger's* clock but leave wallets' dust
	// nullifier windows pinned at the real-head block's tblock, so
	// transaction builders would read a warped `latest_block_context`
	// while wallet dust availability still reflects real-head time.
	// The synthetic has no transactions, so we don't need a matching
	// `update_dust_from_events` — `apply_block_*` returns an empty
	// event vec on a tx-less block.
	//
	// Handle both Ledger7 and Ledger8 variants: a pre-fork chain
	// produces a `Ledger7` context out of step 5, and the raw/no-cache
	// path replays the synthetic block inline in that case, so the
	// cached path must do the same to preserve dust-warp semantics on
	// pre-Ledger8 sources.
	if let Some(synthetic) = synthetic_dust_warp {
		match &fork_ctx {
			ForkAwareLedgerContext::Ledger9(ctx9) => {
				let _events = apply_block_9(ctx9, synthetic);
				ctx9.update_dust_from_block(&block_context_from_raw_9(synthetic));
			},
			ForkAwareLedgerContext::Ledger8(ctx8) => {
				let _events = apply_block_8(ctx8, synthetic);
				ctx8.update_dust_from_block(&block_context_from_raw_8(synthetic));
			},
			ForkAwareLedgerContext::Ledger7(ctx7) => {
				let _events = apply_block_7(ctx7, synthetic);
				ctx7.update_dust_from_block(&block_context_from_raw_7(synthetic));
			},
		}
	}

	fork_ctx
}

/// Save per-wallet cache from a `ForkAwareLedgerContext` if it holds a ledger 9 context.
async fn try_save_cache_v2(
	fork_ctx: &ForkAwareLedgerContext,
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	block_height: u64,
	storage: &dyn WalletStateCaching,
	schemes: &WalletSchemes,
) {
	let ctx = match fork_ctx {
		ForkAwareLedgerContext::Ledger9(ctx) => ctx,
		ForkAwareLedgerContext::Ledger8(_) => {
			log::debug!("Skipping cache save: context is still on ledger 8");
			return;
		},
		ForkAwareLedgerContext::Ledger7(_) => {
			log::debug!("Skipping cache save: context is still on ledger 7");
			return;
		},
	};

	// Save ledger snapshot
	let t = std::time::Instant::now();
	let snapshot = match wallet_state_cache::create_ledger_snapshot(ctx, block_height) {
		Ok(s) => s,
		Err(e) => {
			log::warn!("Failed to create ledger snapshot: {}", e);
			return;
		},
	};
	log::debug!("[perf] create_ledger_snapshot took {:?}", t.elapsed());

	let t = std::time::Instant::now();
	storage.set_ledger_snapshot(chain_id, snapshot).await;
	log::debug!("[perf] storage.set_ledger_snapshot took {:?}", t.elapsed());

	// Save individual wallet snapshots
	let t = std::time::Instant::now();
	let wallet_snapshots: Vec<_> = wallet_seeds
		.iter()
		.filter_map(|seed| {
			match wallet_state_cache::create_wallet_snapshot(
				ctx,
				seed,
				scheme_of(schemes, seed),
				block_height,
			) {
				Ok(ws) => Some(ws),
				Err(e) => {
					log::warn!("Failed to create wallet snapshot: {}", e);
					None
				},
			}
		})
		.collect();
	log::debug!(
		"[perf] create wallet snapshots: {} wallets in {:?}",
		wallet_snapshots.len(),
		t.elapsed()
	);

	if !wallet_snapshots.is_empty() {
		let t = std::time::Instant::now();
		storage.set_wallet_states(chain_id, &wallet_snapshots).await;
		log::debug!("[perf] storage.set_wallet_states took {:?}", t.elapsed());
	}

	// GC: keep heights referenced by all cached wallets (cross-process safe)
	let t = std::time::Instant::now();
	let mut keep_heights = storage.get_all_cached_wallet_heights(chain_id).await;
	log::debug!("[perf] storage.get_all_cached_wallet_heights took {:?}", t.elapsed());
	if !keep_heights.contains(&block_height) {
		keep_heights.push(block_height);
	}
	let t = std::time::Instant::now();
	storage.gc_ledger_snapshots(chain_id, &keep_heights).await;
	log::debug!("[perf] storage.gc_ledger_snapshots took {:?}", t.elapsed());

	log::info!(
		"Saved per-wallet cache at block {} ({} wallets, 1 ledger snapshot)",
		block_height,
		wallet_snapshots.len()
	);
}

#[derive(Debug, thiserror::Error)]
#[error("chain has not reached ledger 8 (final version: {0:?})")]
pub struct ContextNotLedger8Error(pub LedgerVersion);

#[derive(Debug, thiserror::Error)]
#[error("chain has not reached ledger 9 (final version: {0:?})")]
pub struct ContextNotLedger9Error(pub LedgerVersion);

/// Build a fork-aware context from source transactions, returning the raw
/// `ForkAwareLedgerContext` without extracting a specific version.
pub fn build_fork_aware_context_raw(
	received_tx: &SourceTransactions,
	wallet_seeds: &[WalletSeed],
) -> ForkAwareLedgerContext {
	build_fork_aware_context_raw_with_schemes(received_tx, wallet_seeds, &WalletSchemes::new())
}

/// Scheme-aware variant of [`build_fork_aware_context_raw`] (see
/// [`build_fork_aware_context_cached_with_schemes`]).
pub fn build_fork_aware_context_raw_with_schemes(
	received_tx: &SourceTransactions,
	wallet_seeds: &[WalletSeed],
	schemes: &WalletSchemes,
) -> ForkAwareLedgerContext {
	let network_id = &received_tx.network_id;
	let initial_version = received_tx
		.blocks
		.first()
		.map(|b| b.ledger_version())
		.unwrap_or(LedgerVersion::Ledger9);

	let seeds_with_schemes: Vec<(WalletSeed, UnshieldedSignatureScheme)> = wallet_seeds
		.iter()
		.map(|seed| (seed.clone(), scheme_of(schemes, seed)))
		.collect();

	let t = std::time::Instant::now();
	let ctx = ForkAwareLedgerContext::new_from_wallet_seeds_with_schemes(
		initial_version,
		network_id,
		&seeds_with_schemes,
	);
	log::debug!("[perf] new_from_wallet_seeds (raw) took {:?}", t.elapsed());

	replay_blocks(ctx, &received_tx.blocks, &[], schemes)
}

/// Build a fork-aware context from source transactions, returning a ledger 9 context.
///
/// This handles chains that may have forked to ledger 9 by using
/// `ForkAwareLedgerContext` to process blocks across version boundaries.
pub fn build_fork_aware_context(
	received_tx: &SourceTransactions,
	wallet_seeds: &[WalletSeed],
) -> Result<LedgerContext<DefaultDB>, ContextNotLedger9Error> {
	let ctx = build_fork_aware_context_raw(received_tx, wallet_seeds);
	let final_version = ctx.version();
	ctx.into_ledger9().ok_or(ContextNotLedger9Error(final_version))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ecdsa_schemes() -> WalletSchemes {
		WalletSchemes::from([(WalletSeed::Short([7u8; 16]), UnshieldedSignatureScheme::Ecdsa)])
	}

	#[test]
	fn ecdsa_guard_rejects_pre_ledger9_sources() {
		for version in [LedgerVersion::Ledger7, LedgerVersion::Ledger8] {
			let err = ensure_ecdsa_supported(version, &ecdsa_schemes())
				.expect_err("ECDSA on a pre-ledger-9 source must be rejected");
			assert!(
				matches!(err, BuilderConstructionError::EcdsaNotSupportedForLedger(v) if v == version),
				"expected EcdsaNotSupportedForLedger({version:?}), got {err:?}",
			);
		}
	}

	#[test]
	fn ecdsa_guard_allows_ledger9() {
		assert!(ensure_ecdsa_supported(LedgerVersion::Ledger9, &ecdsa_schemes()).is_ok());
	}

	#[test]
	fn relevant_wallet_schemes_rejects_same_seed_under_both_schemes() {
		let seed = "0000000000000000000000000000000000000000000000000000000000000042";
		let builder = Builder::DeregisterDustAddress(DeregisterDustAddressArgs {
			wallet_seed: format!("schnorr:{seed}").parse().unwrap(),
			funding_seed: format!("ecdsa:{seed}").parse().unwrap(),
			rng_seed: None,
		});

		builder
			.relevant_wallet_schemes()
			.expect_err("same seed requested under two different schemes must be rejected");
	}

	#[test]
	fn relevant_wallet_schemes_allows_same_seed_under_one_scheme() {
		let seed = "0000000000000000000000000000000000000000000000000000000000000042";
		let builder = Builder::DeregisterDustAddress(DeregisterDustAddressArgs {
			wallet_seed: format!("ecdsa:{seed}").parse().unwrap(),
			funding_seed: format!("ecdsa:{seed}").parse().unwrap(),
			rng_seed: None,
		});

		let schemes = builder.relevant_wallet_schemes().expect("repeated same-scheme seed is fine");
		assert_eq!(schemes.len(), 1);
	}

	#[test]
	fn schnorr_only_is_allowed_on_every_version() {
		// The empty map (all-Schnorr) and an explicit Schnorr entry must both pass on any version.
		let schnorr = WalletSchemes::from([(
			WalletSeed::Short([7u8; 16]),
			UnshieldedSignatureScheme::Schnorr,
		)]);
		for version in [LedgerVersion::Ledger7, LedgerVersion::Ledger8, LedgerVersion::Ledger9] {
			assert!(ensure_ecdsa_supported(version, &WalletSchemes::new()).is_ok());
			assert!(ensure_ecdsa_supported(version, &schnorr).is_ok());
		}
	}
}

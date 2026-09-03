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
		ForkAwareLedgerContext, apply_block_8, apply_block_9, block_context_from_raw_8,
		block_context_from_raw_9, fork_context_8_to_9,
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
	/// Seed for the contract committee. Accepts multiple. Each accepts an optional
	/// `schnorr:`/`ecdsa:` scheme prefix (bare = Schnorr; ECDSA requires ledger 9).
	#[arg(long = "authority-seed", value_parser = cli::scheme_seed_decode)]
	pub authority_seeds: Vec<cli::SchemeSeed>,
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
	/// Seed for the current contract authority. Accepts multiple. Each accepts an optional
	/// `schnorr:`/`ecdsa:` scheme prefix (bare = Schnorr; ECDSA requires ledger 9).
	#[arg(long = "authority-seed", value_parser = cli::scheme_seed_decode)]
	pub authority_seeds: Vec<cli::SchemeSeed>,
	/// Seed for the new authority. Accepts multiple. Each accepts an optional
	/// `schnorr:`/`ecdsa:` scheme prefix (bare = Schnorr; ECDSA requires ledger 9).
	#[arg(long = "new-authority-seed", value_parser = cli::scheme_seed_decode)]
	pub new_authority_seeds: Vec<cli::SchemeSeed>,
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
				let funding = match call {
					ContractCall::Deploy(args) => &args.funding_seed,
					ContractCall::Call(args) => &args.funding_seed,
					ContractCall::Maintenance(args) => &args.funding_seed,
				};
				let mut seeds = vec![Wallet::<DefaultDB>::wallet_seed_decode(funding)];
				// Committee members are also built into the context so their (possibly ECDSA)
				// scheme is resolved consistently with `relevant_wallet_schemes`.
				match call {
					ContractCall::Deploy(args) => {
						seeds.extend(args.authority_seeds.iter().map(|s| s.resolve().0));
					},
					ContractCall::Maintenance(args) => {
						seeds.extend(args.authority_seeds.iter().map(|s| s.resolve().0));
						seeds.extend(args.new_authority_seeds.iter().map(|s| s.resolve().0));
					},
					ContractCall::Call(_) => {},
				}
				Ok(seeds)
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
	/// Contract-committee seeds (`ContractSimple` deploy/maintenance authority members) carry a
	/// per-seed scheme and are included here so ECDSA committees are guarded on pre-ledger-9 chains.
	/// The contract *funding* seed, `ContractCustom` seeds and the batch *output* seeds stay Schnorr.
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
			Builder::ContractSimple(call) => return contract_call_wallet_schemes(call),
			Builder::ContractCustom(_) | Builder::Send => {},
		}
		Ok(schemes)
	}

	/// Construct a versioned builder for the appropriate ledger version.
	///
	/// Dispatches on `fork_ctx.version()`:
	/// - Ledger8 → builds with ledger_8 types
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

/// Scheme map for a `contract-simple` call's wallets (funding + committee members). Shared by
/// [`Builder::relevant_wallet_schemes`] and `generate-sample-intent`, which builds contract intents
/// outside the `Builder` flow but needs the same pre-ledger-9 [`ensure_ecdsa_supported`] guard. The
/// funding seed is marked Schnorr so reusing it as an `ecdsa:` committee member is rejected, not
/// silently rebuilt as ECDSA (the context/cache keys wallets by seed alone).
pub fn contract_call_wallet_schemes(call: &ContractCall) -> Result<WalletSchemes, &'static str> {
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

	let funding = match call {
		ContractCall::Deploy(args) => &args.funding_seed,
		ContractCall::Call(args) => &args.funding_seed,
		ContractCall::Maintenance(args) => &args.funding_seed,
	};
	mark(Wallet::<DefaultDB>::wallet_seed_decode(funding), UnshieldedSignatureScheme::Schnorr)?;
	match call {
		ContractCall::Deploy(args) => {
			for s in &args.authority_seeds {
				let (seed, scheme) = s.resolve();
				mark(seed, scheme)?;
			}
		},
		ContractCall::Maintenance(args) => {
			for s in args.authority_seeds.iter().chain(&args.new_authority_seeds) {
				let (seed, scheme) = s.resolve();
				mark(seed, scheme)?;
			}
		},
		ContractCall::Call(_) => {},
	}
	Ok(schemes)
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
/// A ledger-8 restore injects and drains `cached` here (no mid-replay injection on ledger 8).
async fn initialize_context(
	received_tx: &SourceTransactions,
	uncached_seeds: &[WalletSeed],
	restore_height: Option<u64>,
	storage: &dyn WalletStateCaching,
	chain_id: H256,
	schemes: &WalletSchemes,
	cached: &mut Vec<(WalletSeed, CachedWalletState)>,
) -> ForkAwareLedgerContext {
	let Some(start_height) = restore_height else {
		let seeds_with_schemes: Vec<(WalletSeed, UnshieldedSignatureScheme)> = uncached_seeds
			.iter()
			.map(|seed| (seed.clone(), scheme_of(schemes, seed)))
			.collect();
		return timed!(
			"new_from_wallet_seeds (cold)",
			ForkAwareLedgerContext::new_from_wallet_seeds_with_schemes(
				received_tx.ledger_version(),
				&received_tx.network_id,
				&seeds_with_schemes,
			)
		);
	};

	let snapshot = timed!(
		"storage.get_ledger_snapshot",
		storage.get_ledger_snapshot(chain_id, start_height).await
	)
	.unwrap_or_else(|| {
		panic!("ledger snapshot missing at height {} — clear caches and retry", start_height)
	});
	log::info!(
		"restoring {:?} ledger snapshot at block {start_height} (skipping replay of everything before it)",
		snapshot.ledger_version
	);

	match snapshot.ledger_version {
		LedgerVersion::Ledger8 => {
			let (ctx, ledger_state, _) = timed!(
				"restore_context_from_ledger_snapshot_8",
				wallet_state_cache::restore_context_from_ledger_snapshot_8(&snapshot)
			)
			.unwrap_or_else(|e| {
				panic!(
					"failed to restore ledger snapshot at height {}: {} — clear caches and retry",
					start_height, e
				)
			});
			for (seed, state) in cached.drain(..) {
				wallet_state_cache::inject_wallet_from_cache_8(
					&ctx,
					&state,
					&seed,
					scheme_of(schemes, &seed),
					&ledger_state,
				)
				.unwrap_or_else(|e| {
					panic!(
						"failed to inject wallet at height {}: {} — clear caches and retry",
						start_height, e
					)
				});
			}
			ForkAwareLedgerContext::Ledger8(ctx)
		},
		LedgerVersion::Ledger9 => {
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
		},
	}
}

/// The one place deciding which cache entries a replay can consume; the rest are
/// dropped and their seeds replayed from genesis. An entry's generation is that of
/// the block at its height. Ledger-9 entries inject mid-replay at any height;
/// ledger-8 entries only at a ledger-8 snapshot restore, which needs every seed
/// cached at one height on a chain still on ledger 8 - anything else would splice
/// the entry into a later ledger-9 state and silently skip the blocks in between.
fn discard_unusable_cache(
	uncached_seeds: &mut Vec<WalletSeed>,
	cached: &mut Vec<(WalletSeed, CachedWalletState)>,
	blocks: &[RawBlockData],
) {
	let version_at = |height: u64| -> Option<LedgerVersion> {
		let i = blocks.partition_point(|b| b.number <= height);
		i.checked_sub(1).map(|i| blocks[i].ledger_version())
	};
	let is_ledger9 =
		|ws: &CachedWalletState| version_at(ws.block_height) == Some(LedgerVersion::Ledger9);

	let ledger8_entries = cached.iter().filter(|(_, ws)| !is_ledger9(ws)).count();
	if ledger8_entries == 0 {
		return;
	}

	let tip_version = blocks.last().map(|b| b.ledger_version());
	let restore_height = cached.first().map(|(_, ws)| ws.block_height);
	let reason = if tip_version != Some(LedgerVersion::Ledger8) {
		"the chain has moved on to ledger 9"
	} else if !uncached_seeds.is_empty() {
		"some requested seeds have no cache entry"
	} else if ledger8_entries == cached.len()
		&& cached.iter().all(|(_, ws)| Some(ws.block_height) == restore_height)
	{
		return;
	} else {
		"the cached heights differ"
	};

	log::warn!(
		"wallet cache: {ledger8_entries} of {} cached seed(s) were saved under ledger 8 and cannot be resumed ({reason}); replaying from genesis",
		cached.len(),
	);
	let (keep, dropped): (Vec<_>, Vec<_>) =
		std::mem::take(cached).into_iter().partition(|(_, ws)| is_ledger9(ws));
	*cached = keep;
	uncached_seeds.extend(dropped.into_iter().map(|(seed, _)| seed));
}

type Db8 = midnight_node_ledger_helpers::ledger_8::DefaultDB;
type Db9 = midnight_node_ledger_helpers::ledger_9::DefaultDB;

const DUST_BATCH_SIZE: usize = 1000;

/// Interval between info-level "replay progress: …" log lines emitted from
/// `replay_blocks_8`. Fine-grained per-batch progress remains at
/// `log::debug!`; this throttle is what users see by default during a
/// multi-hour replay so it doesn't look like the process has hung.
const REPLAY_INFO_HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(30);

fn replay_tx_failures() -> (u64, u64) {
	use std::sync::atomic::Ordering::Relaxed;
	(
		midnight_node_ledger_helpers::replay_stats::PARTIALLY_FAILED_TXS.load(Relaxed),
		midnight_node_ledger_helpers::replay_stats::FAILED_TXS.load(Relaxed),
	)
}

fn log_replay_progress(done: usize, total: usize) {
	let (partial, failed) = replay_tx_failures();
	log::info!(
		"replay progress: {done}/{total} blocks ({:.1}%); historical txs partially failed: {partial}, failed: {failed}",
		done as f64 / total as f64 * 100.0,
	);
}

fn replay_blocks_8(
	ctx: &midnight_node_ledger_helpers::ledger_8::context::LedgerContext<Db8>,
	blocks_sorted_by_height: &[RawBlockData],
) {
	let mut events: Vec<midnight_node_ledger_helpers::ledger_8::Event<Db8>> = Vec::new();

	let total = blocks_sorted_by_height.len();
	let mut last_info_at = std::time::Instant::now();

	for (i, block) in blocks_sorted_by_height.iter().enumerate() {
		events.extend(apply_block_8(ctx, block));

		let is_last = i + 1 == total;
		if events.len() >= DUST_BATCH_SIZE || is_last {
			ctx.update_dust_from_events(events.as_slice());
			events.clear();
			log::debug!("[perf] replay_blocks_8 progress: {}/{} blocks", i + 1, total);
		}

		// Heartbeat lives outside the flush branch so a long stretch of blocks
		// with no dust events still gets a "still alive" signal.
		if last_info_at.elapsed() >= REPLAY_INFO_HEARTBEAT {
			log_replay_progress(i + 1, total);
			last_info_at = std::time::Instant::now();
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
			log::debug!("[perf] replay_blocks_9 progress: {}/{} blocks", i + 1, total);
		}

		// See note in `replay_blocks_8`: heartbeat must be evaluated every
		// iteration, not gated on the event-flush condition, so sparse
		// chains still get a "still alive" signal at the 30 s cadence.
		if last_info_at.elapsed() >= REPLAY_INFO_HEARTBEAT {
			log_replay_progress(i + 1, total);
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

/// Fork a ledger-8 context to ledger 9 (real state translation) and replay the
/// ledger-9 blocks, if any. Returns the ledger-8 context unchanged when there are
/// no ledger-9 blocks.
fn fork_8_to_9_if_needed(
	ctx8: midnight_node_ledger_helpers::ledger_8::context::LedgerContext<Db8>,
	l9_blocks: &[RawBlockData],
	cached: &[(WalletSeed, CachedWalletState)],
	schemes: &WalletSchemes,
) -> ForkAwareLedgerContext {
	if l9_blocks.is_empty() {
		ForkAwareLedgerContext::Ledger8(ctx8)
	} else {
		let ctx9 =
			timed!("fork_context_8_to_9", fork_context_8_to_9(ctx8)).expect("fork 8 to 9 failed");
		replay_blocks_9(&ctx9, l9_blocks, cached, schemes);
		ForkAwareLedgerContext::Ledger9(ctx9)
	}
}

/// Replays blocks across a potential Ledger8->Ledger9 fork boundary,
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

	let fork_8_to_9_idx = blocks.partition_point(|b| b.ledger_version() == LedgerVersion::Ledger8);
	let (l8_blocks, l9_blocks) = blocks.split_at(fork_8_to_9_idx);

	// Replay each version's blocks in order, forking the context across the
	// 8->9 boundary as needed. The fork performs a real state
	// translation (see `fork_context_8_to_9`) so post-hardfork transactions are
	// built at ledger 9, matching the upgraded chain.
	let result = match fork_ctx {
		ForkAwareLedgerContext::Ledger8(ctx8) => {
			replay_blocks_8(&ctx8, l8_blocks);
			fork_8_to_9_if_needed(ctx8, l9_blocks, cached, schemes)
		},
		ForkAwareLedgerContext::Ledger9(ctx9) => {
			assert!(l8_blocks.is_empty(), "Ledger8 blocks with Ledger9 context");
			replay_blocks_9(&ctx9, l9_blocks, cached, schemes);
			ForkAwareLedgerContext::Ledger9(ctx9)
		},
	};

	log::debug!("[perf] block replay: {} blocks in {:?}", blocks.len(), t_replay.elapsed());
	let (partial, failed) = replay_tx_failures();
	if partial + failed > 0 {
		log::info!(
			"replayed {} blocks; historical txs partially failed: {partial}, failed: {failed} (normal on-chain history, details at debug level)",
			blocks.len()
		);
	}
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
	replay_checkpoint_interval: u64,
) -> ForkAwareLedgerContext {
	build_fork_aware_context_cached_with_schemes(
		wallet_seeds,
		received_tx,
		cache_storage,
		&WalletSchemes::new(),
		replay_checkpoint_interval,
	)
	.await
}

/// Scheme-aware variant of [`build_fork_aware_context_cached`]. `schemes` maps each seed to its
/// unshielded signature scheme (absent → Schnorr); this determines both the cache key and how
/// wallets are (re)built, so ECDSA identities cache and restore correctly and never collide with
/// their Schnorr counterparts for the same seed.
///
/// `replay_checkpoint_interval` > 0 saves a wallet-cache checkpoint every that
/// many replayed blocks, so an interrupted long replay resumes from the last
/// checkpoint instead of starting over. 0 disables checkpointing.
pub async fn build_fork_aware_context_cached_with_schemes(
	wallet_seeds: &[WalletSeed],
	received_tx: &SourceTransactions,
	cache_storage: Option<&dyn WalletStateCaching>,
	schemes: &WalletSchemes,
	replay_checkpoint_interval: u64,
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

	// 1. Load cache and partition wallets.
	let (mut uncached_seeds, mut cached) =
		load_and_partition_cache(wallet_seeds, chain_id, storage, schemes).await;
	discard_unusable_cache(&mut uncached_seeds, &mut cached, real_blocks);

	// 2. Warm start only when every seed is cached: an uncached wallet needs its
	//    full history scanned, which forces the replay back to genesis.
	let restore_height = if uncached_seeds.is_empty() {
		cached.first().map(|c| c.1.block_height)
	} else {
		log::warn!(
			"{} of {} wallet seeds have no cache entry ({} cached) — full replay from genesis forced. \
			 Warm the cache once with the complete seed set to avoid this.",
			uncached_seeds.len(),
			wallet_seeds.len(),
			cached.len(),
		);
		None
	};

	// 3. Initialize context (cold genesis or warm snapshot restore).
	let fork_ctx = initialize_context(
		received_tx,
		&uncached_seeds,
		restore_height,
		storage,
		chain_id,
		schemes,
		&mut cached,
	)
	.await;

	// 4. Determine blocks to replay.
	//
	// Warm path uses `partition_point` (O(log n) binary search) rather
	// than a linear `.filter()` — `real_blocks` is sorted by `b.number`
	// ascending (the rest of `replay_blocks_*` already relies on this).
	// Cold path takes the whole slice.
	let blocks: &[RawBlockData] = match restore_height {
		None => real_blocks,
		Some(height) => &real_blocks[real_blocks.partition_point(|b| b.number <= height)..],
	};

	// 5. Replay with mid-replay wallet injection, optionally saving
	// checkpoints so an interrupted long replay resumes from the last
	// checkpoint instead of starting over.
	let interval = replay_checkpoint_interval as usize;
	let fork_ctx = if interval > 0 && blocks.len() > interval {
		let mut ctx = fork_ctx;
		let mut cached_cursor = 0usize;
		let mut start = 0usize;
		while start < blocks.len() {
			let end = usize::min(start + interval, blocks.len());
			let chunk = &blocks[start..end];
			let chunk_last = chunk[chunk.len() - 1].number;
			// Wallets cached beyond this chunk's last block must be withheld:
			// `replay_blocks` injects any leftovers of the slice it is given
			// at the end of its block range, which would splice a
			// future-height wallet state into an older ledger state. Wallets
			// cached exactly at `chunk_last` are injected against the same
			// post-block ledger state either way, so `<=` matches the
			// monolithic behavior.
			let cached_end = cached.partition_point(|(_, ws)| ws.block_height <= chunk_last);
			ctx = replay_blocks(ctx, chunk, &cached[cached_cursor..cached_end], schemes);
			cached_cursor = cached_end;
			// The final chunk's save is step 6 below.
			if end < blocks.len() {
				log::info!(
					"replay checkpoint: saving cache at block {} ({}/{} blocks replayed)",
					chunk_last,
					end,
					blocks.len(),
				);
				try_save_cache_v2(&ctx, wallet_seeds, chain_id, chunk_last, storage, schemes).await;
			}
			start = end;
		}
		ctx
	} else {
		replay_blocks(fork_ctx, blocks, &cached, schemes)
	};

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
	// Mirrors `replay_blocks_8`'s contract: `apply_block_*` only
	// updates the ledger context (and `latest_block_context`); the
	// per-wallet dust TTL advance lives in `update_dust_from_block`,
	// which `replay_blocks_8` always calls for the last replayed
	// block (see its final stanza). Without this second call the
	// warp would advance the *ledger's* clock but leave wallets' dust
	// nullifier windows pinned at the real-head block's tblock, so
	// transaction builders would read a warped `latest_block_context`
	// while wallet dust availability still reflects real-head time.
	// The synthetic has no transactions, so we don't need a matching
	// `update_dust_from_events` — `apply_block_*` returns an empty
	// event vec on a tx-less block.
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
		}
	}

	fork_ctx
}

/// Save the ledger snapshot + per-wallet cache at `block_height`.
async fn try_save_cache_v2(
	fork_ctx: &ForkAwareLedgerContext,
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	block_height: u64,
	storage: &dyn WalletStateCaching,
	schemes: &WalletSchemes,
) {
	match fork_ctx {
		ForkAwareLedgerContext::Ledger9(ctx) => {
			save_cache_ledger9(ctx, wallet_seeds, chain_id, block_height, storage, schemes).await
		},
		ForkAwareLedgerContext::Ledger8(ctx) => {
			save_cache_ledger8(ctx, wallet_seeds, chain_id, block_height, storage, schemes).await
		},
	}
}

/// Ledger-8 twin of [`save_cache_ledger9`].
async fn save_cache_ledger8(
	ctx: &midnight_node_ledger_helpers::ledger_8::context::LedgerContext<Db8>,
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	block_height: u64,
	storage: &dyn WalletStateCaching,
	schemes: &WalletSchemes,
) {
	let t = std::time::Instant::now();
	let snapshot = match wallet_state_cache::create_ledger_snapshot_8(ctx, block_height) {
		Ok(s) => s,
		Err(e) => {
			log::warn!("Failed to create ledger snapshot: {}", e);
			return;
		},
	};
	log::debug!("[perf] create_ledger_snapshot_8 took {:?}", t.elapsed());

	storage.set_ledger_snapshot(chain_id, snapshot).await;

	let wallet_snapshots: Vec<_> = wallet_seeds
		.iter()
		.filter_map(|seed| {
			match wallet_state_cache::create_wallet_snapshot_8(
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

	if !wallet_snapshots.is_empty() {
		storage.set_wallet_states(chain_id, &wallet_snapshots).await;
	}

	// GC: keep heights referenced by all cached wallets (cross-process safe)
	let mut keep_heights = storage.get_all_cached_wallet_heights(chain_id).await;
	if !keep_heights.contains(&block_height) {
		keep_heights.push(block_height);
	}
	storage.gc_ledger_snapshots(chain_id, &keep_heights).await;

	log::info!(
		"Saved per-wallet cache at block {} ({} wallets, 1 ledger snapshot)",
		block_height,
		wallet_snapshots.len()
	);
}

/// Persist the ledger snapshot + per-wallet states at `block_height`.
async fn save_cache_ledger9(
	ctx: &LedgerContext<DefaultDB>,
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	block_height: u64,
	storage: &dyn WalletStateCaching,
	schemes: &WalletSchemes,
) {
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
		let version = LedgerVersion::Ledger8;
		let err = ensure_ecdsa_supported(version, &ecdsa_schemes())
			.expect_err("ECDSA on a pre-ledger-9 source must be rejected");
		assert!(
			matches!(err, BuilderConstructionError::EcdsaNotSupportedForLedger(v) if v == version),
			"expected EcdsaNotSupportedForLedger({version:?}), got {err:?}",
		);
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
	fn relevant_wallet_schemes_rejects_funding_seed_reused_as_ecdsa_authority() {
		// The contract funding seed is always the Schnorr identity, but it is folded into the same
		// context set as the committee seeds (see `relevant_wallet_seeds`). Reusing it as an
		// `ecdsa:` committee member must be rejected by the cross-scheme guard, not silently
		// rebuilt as an ECDSA wallet (which would fund from the wrong on-chain identity).
		let seed = "0000000000000000000000000000000000000000000000000000000000000042";
		let builder = Builder::ContractSimple(ContractCall::Deploy(ContractDeployArgs {
			funding_seed: seed.to_string(),
			authority_seeds: vec![format!("ecdsa:{seed}").parse().unwrap()],
			authority_threshold: None,
			rng_seed: None,
		}));

		builder
			.relevant_wallet_schemes()
			.expect_err("funding seed reused as an ECDSA committee member must be rejected");
	}

	#[test]
	fn relevant_wallet_schemes_allows_distinct_contract_committee() {
		// A distinct ECDSA committee member alongside the Schnorr funding seed is legitimate: only
		// the ECDSA authority is recorded, and the funding seed stays (implicitly) Schnorr.
		let funding = "0000000000000000000000000000000000000000000000000000000000000042";
		let authority = "0000000000000000000000000000000000000000000000000000000000000043";
		let builder = Builder::ContractSimple(ContractCall::Deploy(ContractDeployArgs {
			funding_seed: funding.to_string(),
			authority_seeds: vec![format!("ecdsa:{authority}").parse().unwrap()],
			authority_threshold: None,
			rng_seed: None,
		}));

		let schemes = builder
			.relevant_wallet_schemes()
			.expect("distinct funding + ECDSA committee is fine");
		assert_eq!(schemes.len(), 1, "only the ECDSA authority should be recorded");
	}

	#[test]
	fn contract_call_wallet_schemes_guards_generate_sample_intent_path() {
		// `generate-sample-intent` builds contract intents outside the `Builder` flow and calls
		// `contract_call_wallet_schemes` directly. It must surface an `ecdsa:` committee member so the
		// pre-ledger-9 guard fires there too (rather than panicking in the ECDSA stubs).
		let funding = "0000000000000000000000000000000000000000000000000000000000000042";
		let authority = "0000000000000000000000000000000000000000000000000000000000000043";
		let call = ContractCall::Deploy(ContractDeployArgs {
			funding_seed: funding.to_string(),
			authority_seeds: vec![format!("ecdsa:{authority}").parse().unwrap()],
			authority_threshold: None,
			rng_seed: None,
		});

		let schemes = contract_call_wallet_schemes(&call)
			.expect("distinct funding + ECDSA committee is fine");
		assert_eq!(schemes.len(), 1, "only the ECDSA authority should be recorded");
		ensure_ecdsa_supported(LedgerVersion::Ledger8, &schemes)
			.expect_err("an ECDSA committee on a pre-ledger-9 source must be rejected");
		assert!(ensure_ecdsa_supported(LedgerVersion::Ledger9, &schemes).is_ok());
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
		for version in [LedgerVersion::Ledger8, LedgerVersion::Ledger9] {
			assert!(ensure_ecdsa_supported(version, &WalletSchemes::new()).is_ok());
			assert!(ensure_ecdsa_supported(version, &schnorr).is_ok());
		}
	}

	fn block(number: u64, version: LedgerVersion) -> RawBlockData {
		RawBlockData {
			hash: [0; 32],
			parent_hash: [0; 32],
			number,
			ledger_version: version,
			transactions: vec![],
			tblock_secs: 0,
			tblock_err: 30,
			parent_block_hash: [0; 32],
			last_block_time_secs: 0,
			state_root: None,
			state: None,
		}
	}

	/// `l8` ledger-8 blocks from genesis, then `l9` ledger-9 blocks.
	fn chain(l8: u64, l9: u64) -> Vec<RawBlockData> {
		(0..l8)
			.map(|n| block(n, LedgerVersion::Ledger8))
			.chain((l8..l8 + l9).map(|n| block(n, LedgerVersion::Ledger9)))
			.collect()
	}

	fn seed(byte: u8) -> WalletSeed {
		WalletSeed::try_from_hex_str(&format!("{:0>64}", format!("{byte:02x}"))).unwrap()
	}

	fn entry(byte: u8, height: u64) -> (WalletSeed, CachedWalletState) {
		(
			seed(byte),
			CachedWalletState {
				seed_hash: H256::zero(),
				block_height: height,
				shielded_state_bytes: vec![],
				dust_local_state_bytes: None,
			},
		)
	}

	fn run(
		uncached: &[u8],
		cached: Vec<(WalletSeed, CachedWalletState)>,
		blocks: &[RawBlockData],
	) -> (Vec<WalletSeed>, Vec<(WalletSeed, CachedWalletState)>) {
		let mut uncached: Vec<WalletSeed> = uncached.iter().map(|b| seed(*b)).collect();
		let mut cached = cached;
		discard_unusable_cache(&mut uncached, &mut cached, blocks);
		(uncached, cached)
	}

	#[test]
	fn cache_predicate_keeps_ledger9_entries_at_any_height() {
		let (uncached, cached) = run(&[3], vec![entry(1, 6), entry(2, 8)], &chain(5, 5));
		assert_eq!(uncached, vec![seed(3)]);
		assert_eq!(cached.len(), 2);
	}

	#[test]
	fn cache_predicate_drops_ledger8_entry_once_chain_crossed() {
		let (uncached, cached) = run(&[2], vec![entry(1, 3)], &chain(5, 5));
		assert!(cached.is_empty());
		assert_eq!(uncached, vec![seed(2), seed(1)]);
	}

	#[test]
	fn cache_predicate_drops_mixed_ledger8_heights_once_chain_crossed() {
		let (uncached, cached) = run(&[], vec![entry(1, 2), entry(2, 3)], &chain(5, 5));
		assert!(cached.is_empty());
		assert_eq!(uncached, vec![seed(1), seed(2)]);
	}

	#[test]
	fn cache_predicate_keeps_ledger9_and_drops_ledger8_entries_once_chain_crossed() {
		let (uncached, cached) = run(&[], vec![entry(1, 3), entry(2, 7)], &chain(5, 5));
		assert_eq!(uncached, vec![seed(1)]);
		assert_eq!(cached.len(), 1);
		assert_eq!(cached[0].0, seed(2));
	}

	#[test]
	fn cache_predicate_drops_mixed_ledger8_heights_on_ledger8_chain() {
		let (uncached, cached) = run(&[], vec![entry(1, 2), entry(2, 3)], &chain(5, 0));
		assert!(cached.is_empty());
		assert_eq!(uncached.len(), 2);
	}

	#[test]
	fn cache_predicate_drops_ledger8_entries_beside_uncached_seed_on_ledger8_chain() {
		let (uncached, cached) = run(&[2], vec![entry(1, 3)], &chain(5, 0));
		assert!(cached.is_empty());
		assert_eq!(uncached, vec![seed(2), seed(1)]);
	}

	#[test]
	fn cache_predicate_keeps_uniform_ledger8_entries_on_ledger8_chain() {
		let (uncached, cached) = run(&[], vec![entry(1, 3), entry(2, 3)], &chain(5, 0));
		assert!(uncached.is_empty());
		assert_eq!(cached.len(), 2);
	}

	/// A cache ahead of the source (the queried node lags).
	#[test]
	fn cache_predicate_treats_heights_beyond_tip_as_tip_generation() {
		let (uncached, cached) = run(&[], vec![entry(1, 99), entry(2, 4)], &chain(0, 5));
		assert!(uncached.is_empty());
		assert_eq!(cached.len(), 2);

		let (uncached, cached) = run(&[], vec![entry(1, 99), entry(2, 99)], &chain(5, 0));
		assert!(uncached.is_empty());
		assert_eq!(cached.len(), 2);
	}
}

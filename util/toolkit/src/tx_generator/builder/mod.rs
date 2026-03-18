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
use clap::{Args, Subcommand};
use midnight_node_ledger_helpers::fork::{
	fork_aware_context::{
		ForkAwareLedgerContext, apply_block_7, apply_block_8, block_context_from_raw_7,
		block_context_from_raw_8, fork_context_7_to_8,
	},
	raw_block_data::{LedgerVersion, RawBlockData},
};
use midnight_node_ledger_helpers::*;
use serde::Deserialize;
use std::{collections::HashSet, path::PathBuf, sync::Arc};

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
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	pub source_seed: WalletSeed,
	/// Funding seed for transaction. If not set, uses source_seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	pub funding_seed: Option<WalletSeed>,
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
	/// Seed for funding wallet. If not provided, uses retroactive DUST from NIGHT UTXOs.
	#[arg(long)]
	pub funding_seed: Option<String>,
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
	/// Seed for the wallet to deregister
	#[arg(long)]
	pub wallet_seed: String,
	/// Seed for funding wallet
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
	/// RNG seed for deterministic transaction generation (32 bytes hex)
	#[arg(
        long,
        value_parser = cli::hex_str_decode::<[u8; 32]>,
    )]
	pub rng_seed: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TransferSpec {
	pub source_seed: String,
	pub destination_address: String,
	pub unshielded_amount: Option<u128>,
	pub unshielded_token_type: Option<String>,
	pub shielded_amount: Option<u128>,
	pub shielded_token_type: Option<String>,
	pub funding_seed: Option<String>,
	pub rng_seed: Option<String>,
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
	/// Claim rewards
	ClaimRewards(ClaimRewardsArgs),
	/// Send single transaction with one-or-many outputs
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
	pub fn relevant_wallet_seeds(&self) -> Vec<WalletSeed> {
		match self {
			Builder::Batches(args) => {
				compute_batches_seeds(&args.funding_seed, args.num_txs_per_batch, args.num_batches)
			},
			Builder::ContractSimple(call) => {
				let seed_str = match call {
					ContractCall::Deploy(args) => &args.funding_seed,
					ContractCall::Call(args) => &args.funding_seed,
					ContractCall::Maintenance(args) => &args.funding_seed,
				};
				vec![Wallet::<DefaultDB>::wallet_seed_decode(seed_str)]
			},
			Builder::ContractCustom(args) => {
				vec![Wallet::<DefaultDB>::wallet_seed_decode(&args.funding_seed)]
			},
			Builder::ClaimRewards(args) => {
				vec![Wallet::<DefaultDB>::wallet_seed_decode(&args.funding_seed)]
			},
			Builder::SingleTx(args) => {
				let mut seeds = vec![args.source_seed];
				seeds.extend(args.funding_seed.iter());
				seeds
			},
			Builder::RegisterDustAddress(args) => {
				let seed = Wallet::<DefaultDB>::wallet_seed_decode(&args.wallet_seed);
				if let Some(ref funding_seed) = args.funding_seed {
					vec![seed, Wallet::<DefaultDB>::wallet_seed_decode(funding_seed)]
				} else {
					vec![seed]
				}
			},
			Builder::DeregisterDustAddress(args) => {
				let seed = Wallet::<DefaultDB>::wallet_seed_decode(&args.wallet_seed);
				let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&args.funding_seed);
				vec![seed, funding_seed]
			},
			Builder::BatchSingleTx(args) => {
				let specs = args.get_transfer_specs();
				let mut seen = HashSet::new();
				let mut seeds = Vec::new();
				for spec in &specs {
					if seen.insert(spec.source_seed.clone()) {
						seeds.push(Wallet::<DefaultDB>::wallet_seed_decode(&spec.source_seed));
					}
					if let Some(ref fs) = spec.funding_seed {
						if seen.insert(fs.clone()) {
							seeds.push(Wallet::<DefaultDB>::wallet_seed_decode(fs));
						}
					}
				}
				seeds
			},
			Builder::Send => vec![],
		}
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
						self_clone.to_builder_v7(Arc::new(context), prover)
					},
					|context| {
						let prover = Self::make_prover(prover_config);
						Ok(self.to_builder_v8(Arc::new(context), prover))
					},
				)
			},
			None => {
				// Pass-through builder (Send) doesn't need context
				Ok(self.to_builder_passthrough())
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

	fn to_builder_v8(
		self,
		context: Arc<LedgerContext<DefaultDB>>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
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

/// Load per-wallet cache entries and partition into uncached seeds and cached (seed, state) pairs.
/// Cached pairs are sorted by block height for two-pointer replay.
async fn load_and_partition_cache(
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	storage: &dyn WalletStateCaching,
) -> (Vec<WalletSeed>, Vec<(WalletSeed, CachedWalletState)>) {
	let seed_hashes: Vec<H256> = wallet_seeds.iter().map(wallet_state_cache::hash_seed).collect();
	let raw_cached = timed!(
		"storage.get_wallet_states",
		storage.get_wallet_states(chain_id, &seed_hashes).await
	);

	let mut uncached_seeds: Vec<WalletSeed> = Vec::new();
	let mut cached: Vec<(WalletSeed, CachedWalletState)> = Vec::new();
	for (seed, cached_state) in wallet_seeds.iter().zip(raw_cached) {
		match cached_state {
			Some(state) => cached.push((*seed, state)),
			None => uncached_seeds.push(*seed),
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
) {
	for (seed, state) in wallets {
		wallet_state_cache::inject_wallet_from_cache(ctx, state, seed, ledger_state)
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
) -> ForkAwareLedgerContext {
	if start_height == 0 {
		timed!(
			"new_from_wallet_seeds (cold)",
			ForkAwareLedgerContext::new_from_wallet_seeds(
				received_tx.ledger_version(),
				&received_tx.network_id,
				uncached_seeds,
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

		ForkAwareLedgerContext::Ledger8(ctx)
	}
}

type Db7 = midnight_node_ledger_helpers::ledger_7::DefaultDB;
type Db8 = midnight_node_ledger_helpers::ledger_8::DefaultDB;

const DUST_BATCH_SIZE: usize = 1000;

fn replay_blocks_7(
	ctx: &midnight_node_ledger_helpers::ledger_7::context::LedgerContext<Db7>,
	blocks_sorted_by_height: &[&RawBlockData],
) {
	let mut events: Vec<midnight_node_ledger_helpers::ledger_7::Event<Db7>> = Vec::new();
	let total = blocks_sorted_by_height.len();

	for (i, block) in blocks_sorted_by_height.iter().enumerate() {
		events.extend(apply_block_7(ctx, block));

		let is_last = i + 1 == total;
		if events.len() >= DUST_BATCH_SIZE || is_last {
			ctx.update_dust_from_events(events.as_slice());
			events.clear();
			log::debug!("[perf] replay_blocks_7 progress: {}/{} blocks", i + 1, total);
		}
	}

	if let Some(block) = blocks_sorted_by_height.last() {
		ctx.update_dust_from_block(&block_context_from_raw_7(block));
	}
}

fn replay_blocks_8(
	ctx: &midnight_node_ledger_helpers::ledger_8::context::LedgerContext<Db8>,
	blocks_sorted_by_height: &[&RawBlockData],
	wallets_sorted_by_height: &[(WalletSeed, CachedWalletState)],
) {
	let mut events: Vec<midnight_node_ledger_helpers::ledger_8::Event<Db8>> = Vec::new();
	let mut remaining = wallets_sorted_by_height;
	let total = blocks_sorted_by_height.len();

	for (i, block) in blocks_sorted_by_height.iter().enumerate() {
		let n = remaining.partition_point(|(_, ws)| ws.block_height < block.number);
		if n > 0 {
			let (to_inject, rest) = remaining.split_at(n);
			if !events.is_empty() {
				ctx.update_dust_from_events(events.as_slice());
				events.clear();
			}
			let ls = ctx.ledger_state.lock().expect("ledger_state lock poisoned").clone();
			inject_cached_wallets(ctx, to_inject, &ls, block.number);
			remaining = rest;
		}

		events.extend(apply_block_8(ctx, block));

		let is_last = i + 1 == total;
		if events.len() >= DUST_BATCH_SIZE || is_last {
			ctx.update_dust_from_events(events.as_slice());
			events.clear();
			log::debug!("[perf] replay_blocks_8 progress: {}/{} blocks", i + 1, total);
		}
	}

	// Inject remaining wallets at the last replayed block height.
	// This handles the case where some wallets are cached at the tip with no new blocks.
	if !remaining.is_empty() {
		let ls = ctx.ledger_state.lock().expect("ledger_state lock poisoned").clone();
		let height = blocks_sorted_by_height.last().map(|b| b.number).unwrap_or(0);
		inject_cached_wallets(ctx, remaining, &ls, height);
	}

	if let Some(block) = blocks_sorted_by_height.last() {
		ctx.update_dust_from_block(&block_context_from_raw_8(block));
	}
}

/// Replays blocks across a potential Ledger7→Ledger8 fork boundary,
/// injecting cached wallets at their saved height.
pub(crate) fn replay_blocks(
	fork_ctx: ForkAwareLedgerContext,
	blocks: &[&RawBlockData],
	cached: &[(WalletSeed, CachedWalletState)],
) -> ForkAwareLedgerContext {
	if !blocks.is_empty() && !cached.is_empty() {
		log::info!(
			"Replaying {} blocks after cache checkpoint ({}..)",
			blocks.len(),
			blocks.first().map(|b| b.number).unwrap_or(0)
		);
	}

	let t_replay = std::time::Instant::now();

	let fork_idx = blocks.partition_point(|b| b.ledger_version() == LedgerVersion::Ledger7);
	let (l7_blocks, l8_blocks) = blocks.split_at(fork_idx);

	let result = match fork_ctx {
		ForkAwareLedgerContext::Ledger7(ctx7) => {
			replay_blocks_7(&ctx7, l7_blocks);
			if l8_blocks.is_empty() {
				assert!(cached.is_empty(), "cached wallets with no Ledger8 blocks");
				ForkAwareLedgerContext::Ledger7(ctx7)
			} else {
				let ctx8 = fork_context_7_to_8(ctx7).expect("fork failed");
				replay_blocks_8(&ctx8, l8_blocks, cached);
				ForkAwareLedgerContext::Ledger8(ctx8)
			}
		},
		ForkAwareLedgerContext::Ledger8(ctx8) => {
			assert!(l7_blocks.is_empty(), "Ledger7 blocks with Ledger8 context");
			replay_blocks_8(&ctx8, l8_blocks, cached);
			ForkAwareLedgerContext::Ledger8(ctx8)
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
	if wallet_seeds.is_empty() {
		return build_fork_aware_context_raw(received_tx, wallet_seeds);
	}
	let Some(chain_id) = received_tx.chain_id() else {
		return build_fork_aware_context_raw(received_tx, wallet_seeds);
	};
	let Some(storage) = cache_storage else {
		return build_fork_aware_context_raw(received_tx, wallet_seeds);
	};

	// 1. Load cache and partition wallets.
	let (uncached_seeds, cached) = load_and_partition_cache(wallet_seeds, chain_id, storage).await;

	// 2. Compute start height.
	let start_height = if !uncached_seeds.is_empty() {
		0
	} else {
		cached.first().map(|c| c.1.block_height).unwrap_or(0)
	};

	// 3. Initialize context (cold genesis or warm snapshot restore).
	let fork_ctx =
		initialize_context(received_tx, &uncached_seeds, start_height, storage, chain_id).await;

	// 4. Determine blocks to replay.
	let blocks: Vec<_> = if start_height == 0 {
		received_tx.blocks.iter().collect()
	} else {
		received_tx.blocks.iter().filter(|b| b.number > start_height).collect()
	};

	// 5. Replay with mid-replay wallet injection.
	let fork_ctx = replay_blocks(fork_ctx, &blocks, &cached);

	// 6. Save updated cache.
	if let Some(final_block) = blocks.last() {
		try_save_cache_v2(&fork_ctx, wallet_seeds, chain_id, final_block.number, storage).await;
	}

	fork_ctx
}

/// Save per-wallet cache from a `ForkAwareLedgerContext` if it holds a ledger 8 context.
async fn try_save_cache_v2(
	fork_ctx: &ForkAwareLedgerContext,
	wallet_seeds: &[WalletSeed],
	chain_id: H256,
	block_height: u64,
	storage: &dyn WalletStateCaching,
) {
	let ctx = match fork_ctx {
		ForkAwareLedgerContext::Ledger8(ctx) => ctx,
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
			match wallet_state_cache::create_wallet_snapshot(ctx, seed, block_height) {
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

/// Build a fork-aware context from source transactions, returning the raw
/// `ForkAwareLedgerContext` without extracting a specific version.
pub fn build_fork_aware_context_raw(
	received_tx: &SourceTransactions,
	wallet_seeds: &[WalletSeed],
) -> ForkAwareLedgerContext {
	let network_id = &received_tx.network_id;
	let initial_version = received_tx
		.blocks
		.first()
		.map(|b| b.ledger_version())
		.unwrap_or(LedgerVersion::Ledger8);

	let t = std::time::Instant::now();
	let ctx =
		ForkAwareLedgerContext::new_from_wallet_seeds(initial_version, network_id, wallet_seeds);
	log::debug!("[perf] new_from_wallet_seeds (raw) took {:?}", t.elapsed());

	let blocks: Vec<_> = received_tx.blocks.iter().collect();
	replay_blocks(ctx, &blocks, &[])
}

/// Build a fork-aware context from source transactions, returning a ledger 8 context.
///
/// This handles chains that may have forked from ledger 7 to ledger 8 by using
/// `ForkAwareLedgerContext` to process blocks across version boundaries.
pub fn build_fork_aware_context(
	received_tx: &SourceTransactions,
	wallet_seeds: &[WalletSeed],
) -> Result<LedgerContext<DefaultDB>, ContextNotLedger8Error> {
	let ctx = build_fork_aware_context_raw(received_tx, wallet_seeds);
	let final_version = ctx.version();
	ctx.into_ledger8().ok_or(ContextNotLedger8Error(final_version))
}

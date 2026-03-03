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
	fork_aware_context::ForkAwareLedgerContext, raw_block_data::LedgerVersion,
};
use midnight_node_ledger_helpers::*;
use std::{path::PathBuf, sync::Arc};

use crate::{
	cli_parsers as cli,
	fetcher::{fetch_storage::WalletStateCaching, wallet_state_cache},
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
	/// Seed for funding wallet
	#[arg(
		long,
		default_value = FUNDING_SEED
	)]
	pub funding_seed: String,
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
				let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&args.funding_seed);
				vec![seed, funding_seed]
			},
			Builder::DeregisterDustAddress(args) => {
				let seed = Wallet::<DefaultDB>::wallet_seed_decode(&args.wallet_seed);
				let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&args.funding_seed);
				vec![seed, funding_seed]
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
			Builder::ContractCustom(_) => {
				return Err(BuilderConstructionError::NotSupportedForLedger7("contract-custom"));
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

/// Build context with optional wallet state caching.
///
/// This function wraps the standard context building with cache support:
/// 1. If cache exists and is valid, restore from cache
/// 2. Only replay blocks since the cache checkpoint
/// 3. Save updated cache after processing
///
/// # Arguments
///
/// * `wallet_seeds` - The wallet seeds to initialize/restore
/// * `received_tx` - The source transactions (blocks) from the network
/// * `prover_arc` - The proof provider
/// * `rng_seed` - Optional RNG seed
/// * `chain_id` - The chain identity (block 1 hash)
/// * `cache_storage` - The wallet state caching backend
///
/// # Returns
///
/// A tuple of (context_arc, tx_info, blocks_cached) where blocks_cached indicates
/// how many blocks were skipped due to cache (0 if no cache hit).
pub async fn build_context_with_cache<C: WalletStateCaching>(
	wallet_seeds: Vec<WalletSeed>,
	received_tx: SourceTransactions,
	prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	rng_seed: Option<[u8; 32]>,
	chain_id: H256,
	cache_storage: Option<&C>,
) -> Result<
	(Arc<LedgerContext<DefaultDB>>, StandardTrasactionInfo<DefaultDB>, u64),
	ContextNotLedger8Error,
> {
	let total_blocks = received_tx.blocks.len() as u64;
	let network_id = &received_tx.network_id;

	// Compute wallet ID for cache lookup
	let wallet_id = compute_wallet_id_for_seeds(&wallet_seeds, network_id);

	// Try to restore from cache if storage is provided
	let (mut fork_ctx, start_block) = if let Some(storage) = cache_storage {
		if let Some(cache) = storage.get_wallet_state(chain_id, wallet_id).await {
			match wallet_state_cache::restore_context_from_cache(&cache, &wallet_seeds, chain_id) {
				Ok((ctx, height)) => {
					log::info!("Restored wallet state from cache at block {}", height);
					(ForkAwareLedgerContext::Ledger8(ctx), height + 1)
				},
				Err(e) => {
					log::warn!("Failed to restore from cache: {}, starting fresh", e);
					let initial_version = received_tx
						.blocks
						.first()
						.map(|b| b.ledger_version())
						.unwrap_or(LedgerVersion::Ledger8);
					(
						ForkAwareLedgerContext::new_from_wallet_seeds(
							initial_version,
							network_id,
							&wallet_seeds,
						),
						0u64,
					)
				},
			}
		} else {
			let initial_version = received_tx
				.blocks
				.first()
				.map(|b| b.ledger_version())
				.unwrap_or(LedgerVersion::Ledger8);
			(
				ForkAwareLedgerContext::new_from_wallet_seeds(
					initial_version,
					network_id,
					&wallet_seeds,
				),
				0u64,
			)
		}
	} else {
		let initial_version = received_tx
			.blocks
			.first()
			.map(|b| b.ledger_version())
			.unwrap_or(LedgerVersion::Ledger8);
		(
			ForkAwareLedgerContext::new_from_wallet_seeds(
				initial_version,
				network_id,
				&wallet_seeds,
			),
			0u64,
		)
	};

	// Replay only blocks since start_block
	let blocks_to_replay: Vec<_> =
		received_tx.blocks.iter().filter(|b| b.number >= start_block).collect();

	let blocks_replayed = blocks_to_replay.len() as u64;

	if blocks_replayed > 0 {
		log::info!(
			"Replaying {} blocks (from {} to {})",
			blocks_replayed,
			start_block,
			start_block + blocks_replayed - 1
		);
	}

	for block in blocks_to_replay {
		fork_ctx = fork_ctx.update_from_block(block);
	}

	let final_version = fork_ctx.version();
	let context = fork_ctx.into_ledger8().ok_or(ContextNotLedger8Error(final_version))?;

	// Save updated cache if storage is provided and blocks were replayed
	if let Some(storage) = cache_storage {
		if blocks_replayed > 0 || start_block == 0 {
			let final_height = start_block + blocks_replayed.saturating_sub(1);
			save_context_to_cache(&context, chain_id, wallet_id, final_height, storage).await;
		}
	}

	let context_arc = Arc::new(context);
	let tx_info =
		StandardTrasactionInfo::new_from_context(context_arc.clone(), prover_arc.clone(), rng_seed);

	let blocks_cached = total_blocks.saturating_sub(blocks_replayed);
	Ok((context_arc, tx_info, blocks_cached))
}

/// Compute a wallet identity from seeds.
fn compute_wallet_id_for_seeds(seeds: &[WalletSeed], network_id: &str) -> H256 {
	use sha2::{Digest, Sha256};

	let mut hasher = Sha256::new();
	hasher.update(network_id.as_bytes());
	for seed in seeds {
		hasher.update(seed.as_bytes());
	}
	H256::from_slice(&hasher.finalize())
}

/// Save context state to cache.
async fn save_context_to_cache<C: WalletStateCaching>(
	context: &LedgerContext<DefaultDB>,
	chain_id: H256,
	wallet_id: H256,
	block_height: u64,
	storage: &C,
) {
	let cache = match wallet_state_cache::create_cache_from_context(
		context,
		chain_id,
		wallet_id,
		block_height,
	) {
		Ok(c) => c,
		Err(e) => {
			log::warn!("Failed to create cache: {}", e);
			return;
		},
	};

	storage.set_wallet_state(chain_id, wallet_id, cache).await;
	log::info!("Saved wallet state cache at block {}", block_height);
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

	let mut ctx =
		ForkAwareLedgerContext::new_from_wallet_seeds(initial_version, network_id, wallet_seeds);
	for block in &received_tx.blocks {
		ctx = ctx.update_from_block(block);
	}

	ctx
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

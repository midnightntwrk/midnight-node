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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned, INVALID_PARAMS_CODE},
};

use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use pallet_midnight::{LedgerApiError, MidnightRuntimeApi};
use sc_client_api::{BlockBackend, BlockchainEvents};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::hashing::blake2_256;
use sp_runtime::traits::Block as BlockT;
use std::num::NonZeroU32;
use std::sync::Arc;

pub const API_VERSIONS: [u32; 1] = [2];

/// Midnight core RPC API.
///
/// Provides methods for querying contract state, ledger state roots, and version
/// information from the Midnight privacy ledger.
#[rpc(client, server)]
pub trait MidnightApi<BlockHash> {
	/// Returns the state of a deployed contract.
	///
	/// The contract is identified by its hex-encoded address. The returned state is
	/// also hex-encoded. Queries run against the best block unless `at` specifies
	/// a historical block hash.
	#[method(name = "midnight_contractState")]
	fn get_state(
		&self,
		contract_address: String,
		at: Option<BlockHash>,
	) -> Result<String, StateRpcError>;

	/// Returns the Merkle root of the zswap (shielded transaction) state tree.
	///
	/// The root is returned as raw bytes. If `at` is `None`, the best block is used.
	#[method(name = "midnight_zswapStateRoot")]
	fn get_zswap_state_root(&self, at: Option<BlockHash>) -> Result<Vec<u8>, StateRpcError>;

	/// Returns the Merkle root of the overall ledger state.
	///
	/// The root is returned as raw bytes. If `at` is `None`, the best block is used.
	#[method(name = "midnight_ledgerStateRoot")]
	fn get_ledger_state_root(&self, at: Option<BlockHash>) -> Result<Vec<u8>, StateRpcError>;

	/// Returns the RPC API version(s) supported by this node.
	///
	/// The returned array currently contains a single element (`[2]`).
	/// This is the RPC protocol version, distinct from the runtime API version.
	#[method(name = "midnight_apiVersions")]
	fn get_supported_api_versions(&self) -> RpcResult<Vec<u32>>;

	/// Returns the ledger implementation version string.
	///
	/// If `at` is `None`, the best block is used.
	#[method(name = "midnight_ledgerVersion")]
	fn get_ledger_version(&self, at: Option<BlockHash>) -> Result<String, BlockRpcError>;

	#[method(name = "midnight_validateTransaction")]
	fn validate_transaction(&self, tx_hex: String, at: Option<BlockHash>) -> RpcResult<String>;
}

#[derive(Debug)]
pub enum StateRpcError {
	BadContractAddress(String),
	BadAccountAddress(String),
	ContractNotPresent,
	UnableToGetContractState,
	UnableToGetZSwapChainState,
	UnableToGetZSwapStateRoot,
	UnableToGetLedgerStateRoot,
}

#[derive(Debug)]
pub enum BlockRpcError {
	UnableToGetBlock(String),
	BlockNotFound,
	UnableToGetLedgerState,
	UnableToDecodeTransactions(String),
	UnableToSerializeBlock(String),
	UnableToGetChainVersion,
}

#[derive(Debug, Serialize)]
pub enum EventsError {
	HexDecode { event: String, error: String },
	Decode { event: String, error: String },
	UnableToSerializeEvent { event: String, error: String },
}

impl Display for BlockRpcError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			BlockRpcError::UnableToGetBlock(reason) => {
				write!(f, "Error while getting block: {}", reason)
			},
			BlockRpcError::BlockNotFound => {
				write!(f, "Unable to get block by hash")
			},
			BlockRpcError::UnableToDecodeTransactions(reason) => {
				write!(f, "Unable to decode transactions for block: {}", reason)
			},
			BlockRpcError::UnableToSerializeBlock(reason) => {
				write!(f, "Unable to serialize block to JSON: {}", reason)
			},
			BlockRpcError::UnableToGetChainVersion => {
				write!(f, "Unable to read chain name")
			},
			BlockRpcError::UnableToGetLedgerState => {
				write!(f, "Unable to get ledger state")
			},
		}
	}
}

impl Display for StateRpcError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			StateRpcError::BadContractAddress(malformed_address) => {
				write!(f, "Unable to decode contract address: {}", malformed_address)
			},
			StateRpcError::BadAccountAddress(malformed_address) => {
				write!(f, "Unable to decode account address: {}", malformed_address)
			},
			StateRpcError::ContractNotPresent => {
				write!(f, "Contract not present at the requested address")
			},
			StateRpcError::UnableToGetContractState => {
				write!(f, "Unable to get requested contract state")
			},
			StateRpcError::UnableToGetZSwapChainState => {
				write!(f, "Unable to get requested zswap chain state")
			},
			StateRpcError::UnableToGetZSwapStateRoot => {
				write!(f, "Unable to get requested zswap state root")
			},
			StateRpcError::UnableToGetLedgerStateRoot => {
				write!(f, "Unable to get requested ledger state root")
			},
		}
	}
}

impl Display for EventsError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			EventsError::HexDecode { event: malformed_event, error } => {
				write!(f, "Unable to hex decode event: {} , because of {}", malformed_event, error)
			},

			EventsError::Decode { event: malformed_event, error } => {
				write!(f, "Unable to decode event: {} , because of {}", malformed_event, error)
			},

			EventsError::UnableToSerializeEvent { event: malformed_event, error } => {
				write!(
					f,
					"Unable to serialize event to json: {} , because of {}",
					malformed_event, error
				)
			},
		}
	}
}

impl std::error::Error for BlockRpcError {}
impl std::error::Error for StateRpcError {}
impl std::error::Error for EventsError {}

impl From<EventsError> for ErrorObjectOwned {
	fn from(value: EventsError) -> Self {
		ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
	}
}

impl From<BlockRpcError> for ErrorObjectOwned {
	fn from(value: BlockRpcError) -> Self {
		ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
	}
}

impl From<StateRpcError> for ErrorObjectOwned {
	fn from(value: StateRpcError) -> Self {
		ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
	}
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum Operation {
	Call { address: String, entry_point: String },
	Deploy { address: String },
	FallibleCoins,
	GuaranteedCoins,
	Maintain { address: String },
	ClaimRewards { value: u128 },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MidnightRpcTransaction {
	pub tx_hash: String,
	pub operations: Vec<Operation>,
	pub identifiers: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum RpcTransaction {
	MidnightTransaction {
		#[serde(skip)]
		tx_raw: String,
		tx: MidnightRpcTransaction,
	},
	MalformedMidnightTransaction,
	Timestamp(u64),
	RuntimeUpgrade,
	UnknownTransaction,
}

/// JSON Schema for this type is provided manually in the OpenRPC document
/// because the generic `Header` type parameter does not implement `JsonSchema`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RpcBlock<Header> {
	pub header: Header,
	pub body: Vec<RpcTransaction>,
	pub transactions_index: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ValidateRateLimitConfig {
	pub global_rate_limit: u32,
	pub per_tx_cooldown_secs: u64,
}

type KeyedRateLimiter = RateLimiter<[u8; 32], DefaultKeyedStateStore<[u8; 32]>, DefaultClock>;

struct ValidationRateLimiter {
	global: governor::RateLimiter<
		governor::state::NotKeyed,
		governor::state::InMemoryState,
		DefaultClock,
	>,
	per_tx: KeyedRateLimiter,
	/// Eviction bookkeeping for the keyed `per_tx` limiter.
	///
	/// The keyed store is an unbounded `DashMap`: an attacker sending uniquely-hashing
	/// transactions grows it without bound (governor only GCs lazily). We bound it by
	/// periodically calling [`RateLimiter::retain_recent`], which drops keys whose cooldown
	/// has fully elapsed (i.e. keys that would pass the check anyway), followed by
	/// `shrink_to_fit` to release the backing capacity.
	///
	/// GC runs at most once per `gc_interval`, triggered opportunistically from the
	/// validation handler rather than from a background timer — `Midnight` is constructed
	/// per RPC connection (see `node/src/rpc.rs`), so a per-connection interval task would
	/// be wasteful.
	last_gc: std::sync::Mutex<std::time::Instant>,
	gc_interval: std::time::Duration,
}

impl ValidationRateLimiter {
	fn new(config: &ValidateRateLimitConfig) -> Self {
		let cooldown_secs = config.per_tx_cooldown_secs.max(1);
		let global_quota =
			Quota::per_second(NonZeroU32::new(config.global_rate_limit.max(1)).unwrap());
		let per_tx_quota = Quota::with_period(std::time::Duration::from_secs(cooldown_secs))
			.expect("per_tx_cooldown_secs > 0");

		Self {
			global: governor::RateLimiter::direct(global_quota),
			per_tx: governor::RateLimiter::keyed(per_tx_quota),
			last_gc: std::sync::Mutex::new(std::time::Instant::now()),
			// Sweep at the cooldown cadence: by the time a key is `gc_interval` old its
			// token has refilled, so eviction never drops a key that is still rate-limiting.
			gc_interval: std::time::Duration::from_secs(cooldown_secs),
		}
	}

	/// Evict stale keys from the `per_tx` store if at least `gc_interval` has elapsed since
	/// the last sweep. Non-blocking: if another thread is already sweeping (or validating),
	/// we skip this round rather than contend on the lock.
	fn maybe_gc(&self) {
		let Ok(mut last_gc) = self.last_gc.try_lock() else {
			return;
		};
		if last_gc.elapsed() >= self.gc_interval {
			self.per_tx.retain_recent();
			self.per_tx.shrink_to_fit();
			*last_gc = std::time::Instant::now();
		}
	}
}

pub struct Midnight<C, Block> {
	client: Arc<C>,
	validate_rate_limiter: Arc<ValidationRateLimiter>,
	_marker: std::marker::PhantomData<Block>,
}

impl<C, Block> Midnight<C, Block> {
	pub fn new(client: Arc<C>, validate_rate_limit_config: ValidateRateLimitConfig) -> Self {
		Self {
			client,
			validate_rate_limiter: Arc::new(ValidationRateLimiter::new(
				&validate_rate_limit_config,
			)),
			_marker: Default::default(),
		}
	}
}

fn get_api_version<C, Block>(
	runtime_api: &sp_api::ApiRef<'_, <C as ProvideRuntimeApi<Block>>::Api>,
	block_hash: Block::Hash,
) -> Result<u32, sp_api::ApiError>
where
	Block: BlockT,
	C: Send + Sync + 'static,
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block>,
	C: BlockBackend<Block>,
	C: BlockchainEvents<Block>,
	C::Api: MidnightRuntimeApi<Block>,
{
	runtime_api
		.api_version::<dyn MidnightRuntimeApi<Block>>(block_hash)?
		.ok_or(sp_api::ApiError::UsingSameInstanceForDifferentBlocks)
}

impl<C, Block> MidnightApiServer<<Block as BlockT>::Hash> for Midnight<C, Block>
where
	Block: BlockT,
	C: Send + Sync + 'static,
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block>,
	C: BlockBackend<Block>,
	C: BlockchainEvents<Block>,
	C::Api: MidnightRuntimeApi<Block>,
{
	fn get_state(
		&self,
		contract_address: String,
		at: Option<<Block as BlockT>::Hash>,
	) -> Result<String, StateRpcError> {
		let dehexed = hex::decode(&contract_address)
			.map_err(|_e| StateRpcError::BadContractAddress(contract_address))?;

		let api = self.client.runtime_api();

		let at = at.unwrap_or_else(||
		// If the block hash is not supplied assume the best block.
		self.client.info().best_hash);

		let api_version = get_api_version::<C, Block>(&api, at)
			.map_err(|_| StateRpcError::UnableToGetContractState)?;

		let result = if api_version < 2 {
			// Legacy path: v1 of the RPC contract predates ContractNotPresent,
			// so callers on api_version < 2 must continue to see the generic
			// UnableToGetContractState. Do not surface ContractNotPresent here.
			#[allow(deprecated)]
			api.get_contract_state_before_version_2(at, dehexed)
				.map_err(|_e| StateRpcError::UnableToGetContractState)?
		} else {
			api.get_contract_state(at, dehexed)
				.map_err(|_e| StateRpcError::UnableToGetContractState)
				.and_then(|inner_res| {
					inner_res.map_err(|e| match e {
						LedgerApiError::ContractNotPresent => StateRpcError::ContractNotPresent,
						_ => StateRpcError::UnableToGetContractState,
					})
				})?
		};

		Ok(hex::encode(result))
	}

	fn get_zswap_state_root(
		&self,
		at: Option<<Block as BlockT>::Hash>,
	) -> Result<Vec<u8>, StateRpcError> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		let root = self
			.client
			.runtime_api()
			.get_zswap_state_root(at)
			.map_err(|_e| StateRpcError::UnableToGetZSwapStateRoot)
			.and_then(|inner_res| {
				inner_res.map_err(|_| StateRpcError::UnableToGetZSwapStateRoot)
			})?;

		Ok(root)
	}

	fn get_ledger_state_root(
		&self,
		at: Option<<Block as BlockT>::Hash>,
	) -> Result<Vec<u8>, StateRpcError> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		let root = self
			.client
			.runtime_api()
			.get_ledger_state_root(at)
			.map_err(|_e| StateRpcError::UnableToGetLedgerStateRoot)
			.and_then(|inner_res| {
				inner_res.map_err(|_| StateRpcError::UnableToGetLedgerStateRoot)
			})?;

		Ok(root)
	}

	fn get_supported_api_versions(&self) -> RpcResult<Vec<u32>> {
		Ok(API_VERSIONS.to_vec())
	}

	fn get_ledger_version(
		&self,
		at: Option<<Block as BlockT>::Hash>,
	) -> Result<String, BlockRpcError> {
		let hash = at.unwrap_or_else(|| self.client.info().best_hash);

		let ledger_version = self
			.client
			.runtime_api()
			.get_ledger_version(hash)
			.map_err(|_e| BlockRpcError::BlockNotFound)?;

		Ok(String::from_utf8_lossy(&ledger_version).to_string())
	}

	fn validate_transaction(
		&self,
		tx_hex: String,
		at: Option<<Block as BlockT>::Hash>,
	) -> RpcResult<String> {
		let tx_bytes = hex::decode(&tx_hex).map_err(|e| {
			ErrorObject::owned(
				INVALID_PARAMS_CODE,
				format!("Invalid hex encoding: {e}"),
				None::<()>,
			)
		})?;

		// Global rate limit first: when the node is saturated we reject here without ever
		// touching the keyed per-tx store, which bounds how fast that store can grow under
		// unique-hash spam.
		if self.validate_rate_limiter.global.check().is_err() {
			return Err(ErrorObject::owned(-32005, "Rate limit exceeded", None::<()>));
		}

		// Opportunistically evict stale keys before we (potentially) insert a new one.
		self.validate_rate_limiter.maybe_gc();

		// Per-tx rate limit (keyed by blake2_256 of tx bytes). This guards against
		// trivial replay of the *same* transaction to stop clients accidentally DOS'ing
		let tx_key = blake2_256(&tx_bytes);
		if self.validate_rate_limiter.per_tx.check_key(&tx_key).is_err() {
			return Err(ErrorObject::owned(
				-32005,
				"Rate limit exceeded: per-transaction cooldown",
				None::<()>,
			));
		}

		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		let api = self.client.runtime_api();

		// The validation context (state key, block context, spec version, max block weight) is
		// served by `get_validation_context`, added to MidnightRuntimeApi in v6. Reject politely
		// on older runtimes rather than dispatching a method they don't implement.
		let api_version = get_api_version::<C, Block>(&api, at).map_err(|e| {
			ErrorObject::owned(-32603, format!("Runtime API error: {e}"), None::<()>)
		})?;
		if api_version < 6 {
			return Err(ErrorObject::owned(
				-32601,
				"midnight_validateTransaction requires MidnightRuntimeApi version >= 6",
				None::<()>,
			));
		}

		let ctx = api.get_validation_context(at).map_err(|e| {
			ErrorObject::owned(-32603, format!("Failed to get validation context: {e}"), None::<()>)
		})?;

		// Ledger version selects the correct native Bridge for validation.
		let runtime_ledger_version = api.get_ledger_version(at).map_err(|e| {
			ErrorObject::owned(-32603, format!("Failed to get ledger version: {e}"), None::<()>)
		})?;

		// Expensive native validation — dispatches to correct ledger version
		match midnight_node_ledger::native_api::validate_transaction_verbose(
			&runtime_ledger_version,
			&ctx.state_key,
			&tx_bytes,
			ctx.block_context,
			ctx.spec_version,
			ctx.max_block_weight,
		) {
			Ok(tx_hash) => Ok(format!("0x{}", hex::encode(tx_hash))),
			Err(validation_err) => {
				#[derive(Serialize)]
				struct ValidationErrorData {
					error_code: u8,
					reason: String,
					details: String,
				}

				Err(ErrorObject::owned(
					-32001,
					"Transaction validation failed",
					Some(ValidationErrorData {
						error_code: validation_err.error_code,
						reason: validation_err.reason,
						details: validation_err.details,
					}),
				))
			},
		}
	}
}

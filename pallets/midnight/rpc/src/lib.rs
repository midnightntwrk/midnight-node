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

/// Sweep the keyed `per_tx` store once it holds more than this many keys. This is a memory cap
/// on the otherwise-unbounded `DashMap` (~32B key + governor state + map overhead per entry),
/// set well above any legitimate working set so normal traffic never triggers a sweep.
const PER_TX_GC_THRESHOLD: usize = 50_000;

struct ValidationRateLimiter {
	global: governor::RateLimiter<
		governor::state::NotKeyed,
		governor::state::InMemoryState,
		DefaultClock,
	>,
	per_tx: KeyedRateLimiter,
	/// Eviction bookkeeping for the keyed `per_tx` limiter.
	///
	/// The keyed store is an unbounded `DashMap` and governor never evicts on its own
	/// (`retain_recent`/`shrink_to_fit` exist precisely because it doesn't): every distinct
	/// tx hash inserts a key that lives until we reclaim it. Because the per-tx check runs
	/// *before* the global limit — so a replayed tx can't burn the shared budget — insertion
	/// is not throttled by the global quota, and a flood of uniquely-hashing txs grows the
	/// store at the incoming request rate. We cap it by sweeping once it exceeds `gc_threshold`:
	/// [`RateLimiter::retain_recent`] drops keys whose cooldown has fully elapsed (keys that
	/// would pass the check anyway), then `shrink_to_fit` releases the backing capacity.
	///
	/// Note `retain_recent` can only reclaim *stale* keys, so under a sustained flood of fresh
	/// unique hashes the store settles around `request_rate * cooldown` keys — the threshold
	/// bounds the steady state but can't drop keys that are still actively rate-limiting.
	///
	/// The sweep is triggered opportunistically from the validation handler rather than a
	/// background timer — `Midnight` is constructed per RPC connection (see `node/src/rpc.rs`),
	/// so a per-connection timer would be wasteful. `gc_lock` serializes concurrent sweeps so
	/// callers don't all scan the map at once; a caller that can't grab it skips this round.
	gc_threshold: usize,
	gc_lock: std::sync::Mutex<()>,
}

/// Why a `validate_transaction` request was rejected by the rate limiter.
#[derive(Debug, PartialEq, Eq)]
enum RateLimitRejection {
	/// The same transaction was resubmitted within its per-tx cooldown.
	PerTx,
	/// The node-wide validation throughput limit is saturated.
	Global,
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
			gc_threshold: PER_TX_GC_THRESHOLD,
			gc_lock: std::sync::Mutex::new(()),
		}
	}

	/// Evict stale keys from the `per_tx` store once it grows past `gc_threshold`. Non-blocking:
	/// if another thread is already sweeping we skip this round rather than contend on the lock.
	fn maybe_gc(&self) {
		if self.per_tx.len() < self.gc_threshold {
			return;
		}
		let Ok(_guard) = self.gc_lock.try_lock() else {
			return;
		};
		// Re-check under the lock: a concurrent sweep may have just drained the store.
		if self.per_tx.len() >= self.gc_threshold {
			self.per_tx.retain_recent();
			self.per_tx.shrink_to_fit();
		}
	}

	/// Decide whether a `validate_transaction` request may proceed.
	///
	/// The per-tx cooldown is checked *before* the global limit, so a replayed tx is rejected
	/// without consuming a global token and cannot starve the shared budget for other callers.
	/// Stale keys are swept before the keyed insert. See [`ValidationRateLimiter`] for the memory
	/// trade-off this ordering implies.
	fn check(&self, tx_key: &[u8; 32]) -> Result<(), RateLimitRejection> {
		self.maybe_gc();
		if self.per_tx.check_key(tx_key).is_err() {
			return Err(RateLimitRejection::PerTx);
		}
		if self.global.check().is_err() {
			return Err(RateLimitRejection::Global);
		}
		Ok(())
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

		// Rate limiting: the per-tx cooldown (keyed by blake2_256 of tx bytes) is checked before
		// the global limit, so a client replaying the *same* transaction — typically a buggy
		// client retrying in a tight loop — is rejected without consuming a global token and so
		// can't drain the shared budget for every other caller. The trade-off: insertion is no
		// longer throttled by the global limit, so the keyed store is bounded by size instead
		// (see `ValidationRateLimiter`).
		match self.validate_rate_limiter.check(&blake2_256(&tx_bytes)) {
			Ok(()) => {},
			Err(RateLimitRejection::PerTx) => {
				return Err(ErrorObject::owned(
					-32005,
					"Rate limit exceeded: per-transaction cooldown",
					None::<()>,
				));
			},
			Err(RateLimitRejection::Global) => {
				return Err(ErrorObject::owned(-32005, "Rate limit exceeded", None::<()>));
			},
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

#[cfg(test)]
mod tests {
	use super::{RateLimitRejection, ValidateRateLimitConfig, ValidationRateLimiter};

	fn limiter(global_rate_limit: u32, per_tx_cooldown_secs: u64) -> ValidationRateLimiter {
		ValidationRateLimiter::new(&ValidateRateLimitConfig {
			global_rate_limit,
			per_tx_cooldown_secs,
		})
	}

	/// A distinct 32-byte tx key per `n`.
	fn key(n: u8) -> [u8; 32] {
		let mut k = [0u8; 32];
		k[0] = n;
		k
	}

	/// Replaying the *same* transaction is rejected by the per-tx cooldown without spending a
	/// global token — i.e. the per-tx check runs before the global one. We prove the global
	/// budget was untouched by the replays by showing it still admits exactly
	/// `global_rate_limit - 1` further *distinct* transactions afterwards.
	///
	/// Under the old global-first ordering this test fails: each replay would consume a global
	/// token before the per-tx check, so the budget would be exhausted by the replays and the
	/// later distinct txs would be rejected (and an over-budget replay would surface as
	/// `Global`, not `PerTx`). Run synchronously so quota replenishment (1 token / 100ms at
	/// rate 10) stays negligible across the whole test.
	#[test]
	fn per_tx_cooldown_does_not_consume_global_budget() {
		// Cooldown long enough that every replay falls inside the per-tx window.
		let rl = limiter(10, 3600);
		let a = key(1);

		// First sighting of A is admitted, consuming 1 of 10 global tokens.
		assert_eq!(rl.check(&a), Ok(()));

		// 100 immediate replays of A are all rejected by the per-tx cooldown...
		for _ in 0..100 {
			assert_eq!(rl.check(&a), Err(RateLimitRejection::PerTx));
		}

		// ...and did not touch the global budget: 9 more distinct txs still pass, using up
		// the remaining tokens (1 + 9 == 10).
		for n in 2..=10u8 {
			assert_eq!(
				rl.check(&key(n)),
				Ok(()),
				"distinct tx {n} must be admitted; replays must not have spent global tokens",
			);
		}

		// The 11th distinct tx exhausts the global budget and is rejected there — not by the
		// per-tx cooldown, since it is a fresh key.
		assert_eq!(rl.check(&key(11)), Err(RateLimitRejection::Global));
	}
}

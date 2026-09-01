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

use midnight_node_ledger::rpc::query_contract_state;
use pallet_midnight::{LedgerApiError, MidnightRuntimeApi};
use sc_client_api::{BlockBackend, BlockchainEvents};

// Re-exported so downstream callers (typed clients, e2e tests) can construct
// `PathKey(AlignedValue::from(...))` without a direct `base-crypto` dependency.
pub use base_crypto::fab::AlignedValue;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

pub const API_VERSIONS: [u32; 1] = [2];
/// TODO: Consider making this a CLI argument so RPC providers can customize it.
pub const MAX_STATE_QUERIES: usize = 100;

/// Maximum byte length of a single serialized path key on the wire.
/// Enforced inside `PathKey::deserialize` before the untagged deserialize
/// step, so a single accepted request can't allocate megabytes per key
/// before the deserializer rejects it.
///
/// Sized against the VM's `eq_valid_input` cap (64 untagged bytes per
/// AlignedValue), with headroom for compound map keys (e.g. a struct of a
/// few primitive fields). Worst-case per-request input bytes:
/// `MAX_STATE_QUERIES * MAX_PATH_DEPTH * MAX_KEY_BYTES`.
const MAX_KEY_BYTES: usize = 512;

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

	/// Queries specific fields from a deployed contract's state tree.
	///
	/// Each query is a path of hex-encoded serialized `AlignedValue` keys that
	/// navigates the state tree. Each key is interpreted based on the current
	/// node type: array index, map key, or merkle tree position.
	///
	/// If `at` is `None`, the best block is used.
	#[method(name = "midnight_queryContractState")]
	fn query_contract_state(
		&self,
		contract_address: String,
		queries: Vec<RpcStateQuery>,
		at: Option<BlockHash>,
	) -> Result<Vec<RpcStateQueryResult>, StateRpcError>;
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
	TooManyQueries { max: usize, got: usize },
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
			StateRpcError::TooManyQueries { max, got } => {
				write!(f, "Too many queries: got {got}, maximum is {max}")
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

/// A query into a contract's state tree.
///
/// Each element in `path` is a [`PathKey`] (a typed `AlignedValue`
/// transported as a 0x-prefixed hex string of its tagged binary form).
/// Interpreted as array index, map key, or merkle tree position depending
/// on the `StateValue` variant at each level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcStateQuery {
	pub path: Vec<PathKey>,
}

/// A typed `AlignedValue` for use in [`RpcStateQuery`]'s `path`.
///
/// Wire format: 0x-prefixed hex of the tagged-serialized `AlignedValue`
/// (the tag carries the type/version so a future ledger upgrade that
/// changes the untagged byte layout fails cleanly instead of silently
/// mis-decoding). The Rust type holds the deserialized `AlignedValue`
/// directly, so call sites don't repeat the bytes ↔ value conversion.
#[derive(Debug, Clone)]
pub struct PathKey(pub AlignedValue);

impl Serialize for PathKey {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let mut buf =
			Vec::with_capacity(midnight_serialize::Serializable::serialized_size(&self.0));
		midnight_serialize::Serializable::serialize(&self.0, &mut buf)
			.map_err(serde::ser::Error::custom)?;
		format!("0x{}", hex::encode(&buf)).serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for PathKey {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		let hex_str = s
			.strip_prefix("0x")
			.ok_or_else(|| serde::de::Error::custom("path key must be 0x-prefixed hex"))?;
		if hex_str.len() > MAX_KEY_BYTES * 2 {
			return Err(serde::de::Error::custom(format!(
				"path key too large: {} hex chars exceeds the maximum of {}",
				hex_str.len(),
				MAX_KEY_BYTES * 2
			)));
		}
		let bytes = hex::decode(hex_str).map_err(serde::de::Error::custom)?;
		// Untagged deserialize does not enforce full consumption, so we
		// check the reader is empty after the AlignedValue to keep the
		// wire format canonical (no trailing junk).
		let mut reader: &[u8] = &bytes;
		let value: AlignedValue = midnight_serialize::Deserializable::deserialize(&mut reader, 0)
			.map_err(serde::de::Error::custom)?;
		if !reader.is_empty() {
			return Err(serde::de::Error::custom(format!(
				"path key has {} trailing byte(s) after a valid AlignedValue",
				reader.len()
			)));
		}
		Ok(PathKey(value))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcStateQueryResult {
	pub query: RpcStateQuery,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
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

pub struct Midnight<C, Block> {
	/// Shared reference to the client.
	client: Arc<C>,
	//todo do I need this one?
	_marker: std::marker::PhantomData<Block>,
}

impl<C, Block> Midnight<C, Block> {
	pub fn new(client: Arc<C>) -> Self {
		Self { client, _marker: Default::default() }
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

	fn query_contract_state(
		&self,
		contract_address: String,
		queries: Vec<RpcStateQuery>,
		at: Option<<Block as BlockT>::Hash>,
	) -> Result<Vec<RpcStateQueryResult>, StateRpcError> {
		if queries.len() > MAX_STATE_QUERIES {
			return Err(StateRpcError::TooManyQueries {
				max: MAX_STATE_QUERIES,
				got: queries.len(),
			});
		}
		let dehexed_address = hex::decode(&contract_address)
			.map_err(|_| StateRpcError::BadContractAddress(contract_address))?;

		let api = self.client.runtime_api();
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		let api_version = get_api_version::<C, Block>(&api, at)
			.map_err(|_| StateRpcError::UnableToGetContractState)?;
		if api_version < 6 {
			return Err(StateRpcError::UnableToGetContractState);
		}

		// Read the state key via the runtime API, then call the bridge directly.
		// This avoids going through WASM for each query — the bridge navigates
		// the contract state lazily in ParityDB (O(log n) per query).
		let state_key =
			api.get_state_key(at).map_err(|_| StateRpcError::UnableToGetContractState)?;

		// The bridge enforces per-path depth and surfaces any per-query
		// navigation failures (out-of-bounds, etc.) inline in its
		// `Result<Vec<u8>, String>` slot, so the RPC layer can do a straight
		// 1:1 zip back to `RpcStateQueryResult`. Key-size and wire-format
		// validation already happened in `PathKey::deserialize`.
		let paths: Vec<Vec<AlignedValue>> = queries
			.iter()
			.map(|q| q.path.iter().map(|key| key.0.clone()).collect())
			.collect();
		let bridge_results =
			query_contract_state(&state_key, &dehexed_address, &paths).map_err(|e| match e {
				LedgerApiError::ContractNotPresent => StateRpcError::ContractNotPresent,
				_ => StateRpcError::UnableToGetContractState,
			})?;

		Ok(queries
			.into_iter()
			.zip(bridge_results.into_iter())
			.map(|(query, result)| match result {
				Ok(value) => {
					RpcStateQueryResult { query, value: Some(hex::encode(value)), error: None }
				},
				Err(msg) => RpcStateQueryResult { query, value: None, error: Some(msg) },
			})
			.collect())
	}
}

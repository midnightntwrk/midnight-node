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

use midnight_node_ledger::{
	host_api::ledger_8::ledger_8_bridge, is_ledger_8_state_key,
	ledger_8::types::LedgerApiError as Ledger8ApiError,
};
use midnight_primitives_ledger::{LedgerStorage, LedgerStorageExt};
use pallet_midnight::{LedgerApiError, MidnightRuntimeApi};
use parity_scale_codec::Decode;
use sc_client_api::{Backend, BlockBackend, BlockchainEvents, StorageKey, StorageProvider};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::Block as BlockT;
use sp_state_machine::BasicExternalities;
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

pub struct Midnight<C, Block, B> {
	/// Shared reference to the client.
	client: Arc<C>,
	/// The node's ledger storage configuration. Needed to build an externalities
	/// scope for native ledger host calls, see [`Midnight::with_ledger_storage`].
	ledger_storage: LedgerStorage,
	//todo do I need this one?
	_marker: std::marker::PhantomData<(Block, B)>,
}

impl<C, Block, B> Midnight<C, Block, B> {
	pub fn new(client: Arc<C>, ledger_storage: LedgerStorage) -> Self {
		Self { client, ledger_storage, _marker: Default::default() }
	}
}

impl<C, Block, B> Midnight<C, Block, B>
where
	Block: BlockT,
	B: Backend<Block>,
	C: StorageProvider<Block, B>,
{
	/// The ledger-8 arena root at `at`, if this block predates the v8->v9 ledger
	/// state translation.
	///
	/// `None` — i.e. "use the runtime API as usual" — when the state is
	/// unavailable, `StateKey` is unset, or the key is already ledger-9.
	///
	/// `StateKey` is read straight from the state backend rather than through the
	/// runtime API on purpose: at the `set_code` block of the hardfork the runtime
	/// API is precisely what cannot read it (the block's committed state pairs
	/// ledger-9 `:code` with a ledger-8 `StateKey`).
	fn maybe_ledger_8_state_key(&self, at: Block::Hash) -> Option<Vec<u8>> {
		// Same derivation as `util/toolkit/src/client.rs`: the storage key is
		// twox_128("Midnight") ++ twox_128("StateKey").
		let key = StorageKey([twox_128(b"Midnight"), twox_128(b"StateKey")].concat());
		let raw = self.client.storage(at, &key).ok().flatten()?;
		let state_key = Vec::<u8>::decode(&mut &raw.0[..]).ok()?;
		is_ledger_8_state_key(&state_key).then_some(state_key)
	}

	/// Run a ledger host function natively, inside a minimal externalities scope
	/// carrying the node's `LedgerStorageExt`.
	///
	/// `#[runtime_interface]` host functions take `&mut self` and so require an
	/// externalities environment. The ledger ones use it only to read
	/// `LedgerStorageExt` and pick the storage mode; without the extension they
	/// silently assume `Separate`, which would panic on a unified-storage node.
	fn with_ledger_storage<R>(&self, f: impl FnOnce() -> R) -> R {
		let mut ext = BasicExternalities::new_empty();
		ext.register_extension(LedgerStorageExt::new(self.ledger_storage.clone()));
		ext.execute_with(f)
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

impl<C, Block, B> MidnightApiServer<<Block as BlockT>::Hash> for Midnight<C, Block, B>
where
	Block: BlockT,
	B: Backend<Block> + 'static,
	C: Send + Sync + 'static,
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block>,
	C: BlockBackend<Block>,
	C: BlockchainEvents<Block>,
	C: StorageProvider<Block, B>,
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

		if let Some(state_key) = self.maybe_ledger_8_state_key(at) {
			let result = self
				.with_ledger_storage(|| ledger_8_bridge::get_contract_state(&state_key, &dehexed))
				.map_err(|e| match e {
					// Same legacy contract as below: api_version < 2 predates
					// ContractNotPresent and must keep seeing the generic error.
					Ledger8ApiError::ContractNotPresent if api_version >= 2 => {
						StateRpcError::ContractNotPresent
					},
					_ => StateRpcError::UnableToGetContractState,
				})?;

			return Ok(hex::encode(result));
		}

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

		if let Some(state_key) = self.maybe_ledger_8_state_key(at) {
			return self
				.with_ledger_storage(|| ledger_8_bridge::get_zswap_state_root(&state_key))
				.map_err(|_| StateRpcError::UnableToGetZSwapStateRoot);
		}

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

		if let Some(state_key) = self.maybe_ledger_8_state_key(at) {
			return self
				.with_ledger_storage(|| ledger_8_bridge::get_ledger_state_root(&state_key))
				.map_err(|_| StateRpcError::UnableToGetLedgerStateRoot);
		}

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
}

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

use pallet_midnight::{LedgerApiError, MidnightRuntimeApi};
use sc_client_api::{BlockBackend, BlockchainEvents};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

// Re-export the RPC trait, error types, and constants from midnight-rpc-api
// so existing consumers of pallet-midnight-rpc don't break.
pub use midnight_rpc_api::{
    BlockRpcError, EventsError, MidnightApiClient, MidnightApiServer, RpcResult, StateRpcError,
    API_VERSIONS,
};

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
}

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

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned, INVALID_PARAMS_CODE},
};

use midnight_node_ledger::types::Op;
use pallet_midnight::MidnightRuntimeApi;
use pallet_midnight::{TransactionType, TransactionTypeV2};
use sc_client_api::{BlockBackend, BlockchainEvents};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::SignedBlock, traits::Block as BlockT};
use std::sync::Arc;

pub const API_VERSIONS: [u32; 1] = [2];

#[rpc(client, server)]
pub trait MidnightApi<BlockHash> {
	#[method(name = "midnight_jsonContractState")]
	fn get_json_state(
		&self,
		contract_address: String,
		at: Option<BlockHash>,
	) -> Result<String, StateRpcError>;

	#[method(name = "midnight_contractState")]
	fn get_state(
		&self,
		contract_address: String,
		at: Option<BlockHash>,
	) -> Result<String, StateRpcError>;

	#[method(name = "midnight_jsonBlock")]
	fn get_block(&self, at: Option<BlockHash>) -> Result<String, BlockRpcError>;

	#[method(name = "midnight_zswapStateRoot")]
	fn get_zswap_state_root(&self, at: Option<BlockHash>) -> Result<Vec<u8>, StateRpcError>;

	#[method(name = "midnight_apiVersions")]
	fn get_supported_api_versions(&self) -> RpcResult<Vec<u32>>;

	#[method(name = "midnight_ledgerVersion")]
	fn get_ledger_version(&self, at: Option<BlockHash>) -> Result<String, BlockRpcError>;
}

#[derive(Debug)]
pub enum StateRpcError {
	BadContractAddress(String),
	BadAccountAddress(String),
	UnableToGetContractState,
	UnableToGetZSwapChainState,
	UnableToGetZSwapStateRoot,
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
			StateRpcError::UnableToGetContractState => {
				write!(f, "Unable to get requested contract state")
			},
			StateRpcError::UnableToGetZSwapChainState => {
				write!(f, "Unable to get requested zswap chain state")
			},
			StateRpcError::UnableToGetZSwapStateRoot => {
				write!(f, "Unable to get requested zswap state root")
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

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Operation {
	Call { address: String, entry_point: String },
	Deploy { address: String },
	FallibleCoins,
	GuaranteedCoins,
	Maintain { address: String },
	ClaimMint { value: u128, coin_type: String },
}
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MidnightRpcTransaction {
	pub tx_hash: String,
	pub operations: Vec<Operation>,
	pub identifiers: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
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

fn decode_transactions(transactions: &[TransactionType]) -> Vec<RpcTransaction> {
	transactions
		.iter()
		.map(|tx| match tx {
			TransactionType::MidnightTx(transaction_bytes, midnight_transaction) => {
				let Some(tx) = midnight_transaction else {
					return RpcTransaction::MalformedMidnightTransaction;
				};
				let tx_hash = hex::encode(tx.hash);
				let identifiers = tx.identifiers.iter().map(hex::encode).collect();
				let mut operations: Vec<Operation> = tx
					.operations
					.iter()
					.map(|op| match op {
						Op::Call { address, entry_point } => Operation::Call {
							address: hex::encode(address),
							entry_point: hex::encode(entry_point),
						},
						Op::Deploy { address } => {
							Operation::Deploy { address: hex::encode(address) }
						},
						Op::Maintain { address } => {
							Operation::Maintain { address: hex::encode(address) }
						},
						Op::ClaimMint { coin_type, value } => Operation::ClaimMint {
							coin_type: hex::encode(coin_type),
							value: *value,
						},
					})
					.collect();

				if tx.has_guaranteed_coins {
					operations.push(Operation::GuaranteedCoins);
				}
				if tx.has_fallible_coins {
					operations.push(Operation::FallibleCoins);
				}

				if let Ok(tx_raw) = String::from_utf8(transaction_bytes.to_vec()) {
					RpcTransaction::MidnightTransaction {
						tx_raw,
						tx: MidnightRpcTransaction { tx_hash, operations, identifiers },
					}
				} else {
					// We can't interpret it
					log::error!("Could not interpret Midnight transaction: {:?}", tx_hash);
					RpcTransaction::MalformedMidnightTransaction
				}
			},
			TransactionType::TimestampTx(time) => RpcTransaction::Timestamp(*time),
			TransactionType::UnknownTx => RpcTransaction::UnknownTransaction,
		})
		.collect()
}

fn decode_transactions_v2(transactions: &[TransactionTypeV2]) -> Vec<RpcTransaction> {
	transactions
		.iter()
		.map(|tx| match tx {
			TransactionTypeV2::MidnightTx(transaction_bytes, midnight_transaction) => {
				let Ok(tx) = midnight_transaction else {
					return RpcTransaction::MalformedMidnightTransaction;
				};
				let tx_hash = hex::encode(tx.hash);
				let identifiers = tx.identifiers.iter().map(hex::encode).collect();
				let mut operations: Vec<Operation> = tx
					.operations
					.iter()
					.map(|op| match op {
						Op::Call { address, entry_point } => Operation::Call {
							address: hex::encode(address),
							entry_point: hex::encode(entry_point),
						},
						Op::Deploy { address } => {
							Operation::Deploy { address: hex::encode(address) }
						},
						Op::Maintain { address } => {
							Operation::Maintain { address: hex::encode(address) }
						},
						Op::ClaimMint { coin_type, value } => Operation::ClaimMint {
							coin_type: hex::encode(coin_type),
							value: *value,
						},
					})
					.collect();

				if tx.has_guaranteed_coins {
					operations.push(Operation::GuaranteedCoins);
				}
				if tx.has_fallible_coins {
					operations.push(Operation::FallibleCoins);
				}

				if let Ok(tx_raw) = String::from_utf8(transaction_bytes.to_vec()) {
					RpcTransaction::MidnightTransaction {
						tx_raw,
						tx: MidnightRpcTransaction { tx_hash, operations, identifiers },
					}
				} else {
					// We can't interpret it
					log::error!("Could not interpret Midnight transaction: {:?}", tx_hash);
					RpcTransaction::MalformedMidnightTransaction
				}
			},
			TransactionTypeV2::TimestampTx(time) => RpcTransaction::Timestamp(*time),
			TransactionTypeV2::UnknownTx => RpcTransaction::UnknownTransaction,
		})
		.collect()
}

fn build_index(decoded_transactions: &[RpcTransaction]) -> Vec<(String, String)> {
	decoded_transactions
		.iter()
		.filter_map(|tx| match tx {
			RpcTransaction::MidnightTransaction { tx_raw, tx } => {
				Some((format!("0x{}", tx.tx_hash), format!("0x{}", tx_raw)))
			},
			_ => None,
		})
		.collect()
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
	fn get_json_state(
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
			#[allow(deprecated)]
			api.get_contract_state_json_before_version_2(at, dehexed)
				.map_err(|_e| StateRpcError::UnableToGetContractState)?
		} else {
			api.get_contract_state_json(at, dehexed)
				.map_err(|_e| StateRpcError::UnableToGetContractState)
				.and_then(|inner_res| {
					inner_res.map_err(|_| StateRpcError::UnableToGetContractState)
				})?
		};

		String::from_utf8(result).map_err(|_| StateRpcError::UnableToGetContractState)
	}

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
			#[allow(deprecated)]
			api.get_contract_state_before_version_2(at, dehexed)
				.map_err(|_e| StateRpcError::UnableToGetContractState)?
		} else {
			api.get_contract_state(at, dehexed)
				.map_err(|_e| StateRpcError::UnableToGetContractState)
				.and_then(|inner_res| {
					inner_res.map_err(|_| StateRpcError::UnableToGetContractState)
				})?
		};

		Ok(hex::encode(result))
	}

	fn get_block(&self, at: Option<<Block as BlockT>::Hash>) -> Result<String, BlockRpcError> {
		let hash = at.unwrap_or_else(|| self.client.info().best_hash);
		let block: SignedBlock<Block> = self
			.client
			.block(hash)
			.map_err(|e| BlockRpcError::UnableToGetBlock(e.to_string()))?
			.ok_or(BlockRpcError::BlockNotFound)?;

		let header = block.block.header();

		let body = block.block.extrinsics();

		let api = self.client.runtime_api();
		let api_version = get_api_version::<C, Block>(&api, hash)
			.map_err(|e| BlockRpcError::UnableToGetBlock(e.to_string()))?;

		let (decoded_transactions, index) = if api_version < 2 {
			#[allow(deprecated)]
			let transactions = api
				.get_decoded_transactions_before_version_2(hash, body.to_vec())
				.map_err(|e| BlockRpcError::UnableToDecodeTransactions(e.to_string()))?;

			let decoded_transactions: Vec<RpcTransaction> =
				decode_transactions(transactions.as_slice());
			let index: Vec<_> = build_index(&decoded_transactions);

			(decoded_transactions, index)
		} else {
			let transactions = api
				.get_decoded_transactions(hash, body.to_vec())
				.map_err(|e| BlockRpcError::UnableToDecodeTransactions(e.to_string()))?;

			let decoded_transactions: Vec<RpcTransaction> =
				decode_transactions_v2(transactions.as_slice());
			let index: Vec<_> = build_index(&decoded_transactions);

			(decoded_transactions, index)
		};

		let block = RpcBlock { header, body: decoded_transactions, transactions_index: index };

		let json_value = serde_json::to_value(block)
			.map_err(|e| BlockRpcError::UnableToSerializeBlock(e.to_string()))?;
		let transformed_json_value = midnight_node_ledger::json::transform(json_value);
		serde_json::to_string(&transformed_json_value)
			.map_err(|e| BlockRpcError::UnableToSerializeBlock(e.to_string()))
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

#[cfg(test)]
mod tests {
	use super::{
		RpcTransaction, TransactionType, TransactionTypeV2, decode_transactions,
		decode_transactions_v2,
	};
	use assert_matches::assert_matches;
	use midnight_node_ledger::types::Tx;
	use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};

	#[test]
	fn test_transaction_decoding() {
		let genesis_transaction = hex::encode(UndeployedNetwork.genesis_tx()).as_bytes().to_vec();
		let genesis_transaction = TransactionType::MidnightTx(
			genesis_transaction,
			Some(Tx {
				hash: [0u8; 32],
				operations: vec![],
				identifiers: vec![],
				has_fallible_coins: false,
				has_guaranteed_coins: false,
			}),
		);
		let decoded_transaction = decode_transactions(&[genesis_transaction]);
		let tx_raw_expected = hex::encode(UndeployedNetwork.genesis_tx());
		let tx_raw = assert_matches!(&decoded_transaction[0], RpcTransaction::MidnightTransaction { tx_raw, tx: _ } => tx_raw);
		assert_eq!(&tx_raw_expected, tx_raw);
	}

	#[test]
	fn test_transaction_decoding_v2() {
		let genesis_transaction = hex::encode(UndeployedNetwork.genesis_tx()).as_bytes().to_vec();
		let genesis_transaction = TransactionTypeV2::MidnightTx(
			genesis_transaction,
			Ok(Tx {
				hash: [0u8; 32],
				operations: vec![],
				identifiers: vec![],
				has_fallible_coins: false,
				has_guaranteed_coins: false,
			}),
		);
		let decoded_transaction = decode_transactions_v2(&[genesis_transaction]);
		let tx_raw_expected = hex::encode(UndeployedNetwork.genesis_tx());
		let tx_raw = assert_matches!(&decoded_transaction[0], RpcTransaction::MidnightTransaction { tx_raw, tx: _ } => tx_raw);
		assert_eq!(&tx_raw_expected, tx_raw);
	}
}

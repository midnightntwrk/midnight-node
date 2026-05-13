// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0

//! Client-side RPC API definition and types for the Midnight pallet.
//!
//! This crate contains only the `#[rpc(client)]` trait and the associated
//! request/response types. It has no dependency on the Substrate runtime
//! or executor, making it suitable for lightweight RPC clients that don't
//! need the full node stack.
//!
//! The server-side implementation lives in `pallet-midnight-rpc`.

use serde::Serialize;

use jsonrpsee::{
    proc_macros::rpc,
    types::error::{ErrorObject, ErrorObjectOwned, INVALID_PARAMS_CODE},
};
// Re-export for downstream consumers
pub use jsonrpsee::core::RpcResult;

pub const API_VERSIONS: [u32; 1] = [2];

/// Midnight core RPC API.
///
/// The `client` feature enables the generated `MidnightApiClient` trait.
/// The `server` feature enables the generated `MidnightApiServer` trait.
#[rpc(client, server)]
pub trait MidnightApi<BlockHash> {
    /// Returns the hex-encoded state of a deployed contract.
    #[method(name = "midnight_contractState")]
    fn get_state(
        &self,
        contract_address: String,
        at: Option<BlockHash>,
    ) -> Result<String, StateRpcError>;

    /// Returns the Merkle root of the zswap state tree.
    #[method(name = "midnight_zswapStateRoot")]
    fn get_zswap_state_root(&self, at: Option<BlockHash>) -> Result<Vec<u8>, StateRpcError>;

    /// Returns the Merkle root of the overall ledger state.
    #[method(name = "midnight_ledgerStateRoot")]
    fn get_ledger_state_root(&self, at: Option<BlockHash>) -> Result<Vec<u8>, StateRpcError>;

    /// Returns the RPC API version(s) supported by this node.
    #[method(name = "midnight_apiVersions")]
    fn get_supported_api_versions(&self) -> RpcResult<Vec<u32>>;

    /// Returns the ledger implementation version string.
    #[method(name = "midnight_ledgerVersion")]
    fn get_ledger_version(&self, at: Option<BlockHash>) -> Result<String, BlockRpcError>;
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum StateRpcError {
    #[error("Unable to decode contract address: {0}")]
    BadContractAddress(String),
    #[error("Unable to decode account address: {0}")]
    BadAccountAddress(String),
    #[error("Contract not present")]
    ContractNotPresent,
    #[error("Unable to get requested contract state")]
    UnableToGetContractState,
    #[error("Unable to get requested zswap chain state")]
    UnableToGetZSwapChainState,
    #[error("Unable to get requested zswap state root")]
    UnableToGetZSwapStateRoot,
    #[error("Unable to get requested ledger state root")]
    UnableToGetLedgerStateRoot,
}

#[derive(Debug, thiserror::Error)]
pub enum BlockRpcError {
    #[error("Error while getting block: {0}")]
    UnableToGetBlock(String),
    #[error("Unable to get block by hash")]
    BlockNotFound,
    #[error("Unable to get ledger state")]
    UnableToGetLedgerState,
    #[error("Unable to decode transactions for block: {0}")]
    UnableToDecodeTransactions(String),
    #[error("Unable to serialize block to JSON: {0}")]
    UnableToSerializeBlock(String),
    #[error("Unable to read chain name")]
    UnableToGetChainVersion,
}

#[derive(Debug, Serialize, thiserror::Error)]
pub enum EventsError {
    #[error("Unable to hex decode event: {event}, because of {error}")]
    HexDecode { event: String, error: String },
    #[error("Unable to decode event: {event}, because of {error}")]
    Decode { event: String, error: String },
    #[error("Unable to serialize event to json: {event}, because of {error}")]
    UnableToSerializeEvent { event: String, error: String },
}

impl From<StateRpcError> for ErrorObjectOwned {
    fn from(value: StateRpcError) -> Self {
        ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
    }
}

impl From<BlockRpcError> for ErrorObjectOwned {
    fn from(value: BlockRpcError) -> Self {
        ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
    }
}

impl From<EventsError> for ErrorObjectOwned {
    fn from(value: EventsError) -> Self {
        ErrorObject::owned(INVALID_PARAMS_CODE, value.to_string(), None::<()>)
    }
}

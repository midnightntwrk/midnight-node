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
use std::fmt::{Display, Formatter};

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

impl Display for StateRpcError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StateRpcError::BadContractAddress(addr) => {
                write!(f, "Unable to decode contract address: {addr}")
            }
            StateRpcError::BadAccountAddress(addr) => {
                write!(f, "Unable to decode account address: {addr}")
            }
            StateRpcError::ContractNotPresent => {
                write!(f, "Contract not present")
            }
            StateRpcError::UnableToGetContractState => {
                write!(f, "Unable to get requested contract state")
            }
            StateRpcError::UnableToGetZSwapChainState => {
                write!(f, "Unable to get requested zswap chain state")
            }
            StateRpcError::UnableToGetZSwapStateRoot => {
                write!(f, "Unable to get requested zswap state root")
            }
            StateRpcError::UnableToGetLedgerStateRoot => {
                write!(f, "Unable to get requested ledger state root")
            }
        }
    }
}

impl Display for BlockRpcError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockRpcError::UnableToGetBlock(reason) => {
                write!(f, "Error while getting block: {reason}")
            }
            BlockRpcError::BlockNotFound => write!(f, "Unable to get block by hash"),
            BlockRpcError::UnableToGetLedgerState => write!(f, "Unable to get ledger state"),
            BlockRpcError::UnableToDecodeTransactions(reason) => {
                write!(f, "Unable to decode transactions for block: {reason}")
            }
            BlockRpcError::UnableToSerializeBlock(reason) => {
                write!(f, "Unable to serialize block to JSON: {reason}")
            }
            BlockRpcError::UnableToGetChainVersion => write!(f, "Unable to read chain name"),
        }
    }
}

impl Display for EventsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EventsError::HexDecode { event, error } => {
                write!(f, "Unable to hex decode event: {event}, because of {error}")
            }
            EventsError::Decode { event, error } => {
                write!(f, "Unable to decode event: {event}, because of {error}")
            }
            EventsError::UnableToSerializeEvent { event, error } => {
                write!(
                    f,
                    "Unable to serialize event to json: {event}, because of {error}"
                )
            }
        }
    }
}

impl std::error::Error for StateRpcError {}
impl std::error::Error for BlockRpcError {}
impl std::error::Error for EventsError {}

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

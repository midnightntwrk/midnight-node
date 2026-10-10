// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0

//! RPC API definition and types for the Midnight pallet.
//!
//! This crate contains the `#[rpc(client, server)]` trait and the associated
//! request/response types. It has no dependency on the Substrate runtime
//! or executor, making it suitable for lightweight RPC clients that don't
//! need the full node stack.
//!
//! The server-side implementation lives in `pallet-midnight-rpc`.

mod error;

pub use error::{BlockRpcError, EventsError, StateRpcError};
pub use jsonrpsee::core::RpcResult;

use jsonrpsee::proc_macros::rpc;

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

// This file is part of midnight-indexer.
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

mod v0_22_0;
mod v1_0_0;
mod v2_0_0;
mod v2_1_0;

// To see how this is generated, look in build.rs
include!(concat!(env!("OUT_DIR"), "/generated_runtime.rs"));

use crate::{
    domain::{DParameter, DustRegistrationEvent, TermsAndConditions},
    infra::subxt_node::{ContentSource, OnlineClientAtBlock, SubxtNodeError},
};
use indexer_common::domain::{
    ByteVec, NodeVersion, SerializedContractAddress, SerializedContractState,
};

/// Runtime specific block details.
pub struct BlockDetails {
    pub timestamp: Option<u64>,
    /// True when this block emitted a `NewSession` event, i.e. the authority set used to
    /// resolve block authors must be refetched for the next block.
    pub new_session: bool,
    pub transactions: Vec<Transaction>,
    pub dust_registration_events: Vec<DustRegistrationEvent>,
    /// c2m-bridge events. Only populated for node 2.0+, where the
    /// `c2m-bridge` pallet exists in the runtime metadata. Empty for earlier
    /// node versions (the pallet did not yet exist there).
    pub bridge_events: Vec<indexer_common::domain::bridge::BridgeEvent>,
}

/// Runtime specific (serialized) transaction.
pub enum Transaction {
    Regular(ByteVec),
    System(ByteVec),
}

/// Make block details depending on the given protocol version.
pub async fn make_block_details(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
    content: Option<&ContentSource>,
) -> Result<BlockDetails, SubxtNodeError> {
    // TODO Replace this often repeated pattern with a macro?
    match node_version {
        NodeVersion::V0_22 => v0_22_0::make_block_details(block, content).await,
        NodeVersion::V1_0 => v1_0_0::make_block_details(block, content).await,
        NodeVersion::V2_0 => v2_0_0::make_block_details(block, content).await,
        NodeVersion::V2_1 => v2_1_0::make_block_details(block, content).await,
    }
}

/// Fetch authorities depending on the given protocol version.
pub async fn fetch_authorities(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Vec<[u8; 32]>, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::fetch_authorities(block).await,
        NodeVersion::V1_0 => v1_0_0::fetch_authorities(block).await,
        NodeVersion::V2_0 => v2_0_0::fetch_authorities(block).await,
        NodeVersion::V2_1 => v2_1_0::fetch_authorities(block).await,
    }
}

/// Decode slot depending on the given protocol version.
pub fn decode_slot(slot: &[u8], node_version: NodeVersion) -> Result<u64, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::decode_slot(slot),
        NodeVersion::V1_0 => v1_0_0::decode_slot(slot),
        NodeVersion::V2_0 => v2_0_0::decode_slot(slot),
        NodeVersion::V2_1 => v2_1_0::decode_slot(slot),
    }
}

/// Get contract state depending on the given protocol version.
pub async fn get_contract_state(
    address: SerializedContractAddress,
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<SerializedContractState, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::get_contract_state(address, block).await,
        NodeVersion::V1_0 => v1_0_0::get_contract_state(address, block).await,
        NodeVersion::V2_0 => v2_0_0::get_contract_state(address, block).await,
        NodeVersion::V2_1 => v2_1_0::get_contract_state(address, block).await,
    }
}

pub async fn get_zswap_merkle_tree_root(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Vec<u8>, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::get_zswap_merkle_tree_root(block).await,
        NodeVersion::V1_0 => v1_0_0::get_zswap_merkle_tree_root(block).await,
        NodeVersion::V2_0 => v2_0_0::get_zswap_merkle_tree_root(block).await,
        NodeVersion::V2_1 => v2_1_0::get_zswap_merkle_tree_root(block).await,
    }
}

/// Get the pure ledger state root (without StorableLedgerState wrapping) at the given block.
pub async fn get_ledger_state_root(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Option<Vec<u8>>, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::get_ledger_state_root(block).await,
        NodeVersion::V1_0 => v1_0_0::get_ledger_state_root(block).await,
        NodeVersion::V2_0 => v2_0_0::get_ledger_state_root(block).await,
        NodeVersion::V2_1 => v2_1_0::get_ledger_state_root(block).await,
    }
}

/// Get D-Parameter depending on the given protocol version.
pub async fn get_d_parameter(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<DParameter, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::get_d_parameter(block).await,
        NodeVersion::V1_0 => v1_0_0::get_d_parameter(block).await,
        NodeVersion::V2_0 => v2_0_0::get_d_parameter(block).await,
        NodeVersion::V2_1 => v2_1_0::get_d_parameter(block).await,
    }
}

/// Fetch genesis cNight registrations from pallet storage.
/// At genesis, Substrate does not emit events (Parity PR #5463), so we query
/// the cNightObservation.Mappings storage directly at block 0.
pub async fn fetch_genesis_cnight_registrations(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Vec<DustRegistrationEvent>, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::fetch_genesis_cnight_registrations(block).await,
        NodeVersion::V1_0 => v1_0_0::fetch_genesis_cnight_registrations(block).await,
        NodeVersion::V2_0 => v2_0_0::fetch_genesis_cnight_registrations(block).await,
        NodeVersion::V2_1 => v2_1_0::fetch_genesis_cnight_registrations(block).await,
    }
}

/// Get Terms and Conditions depending on the given protocol version.
pub async fn get_terms_and_conditions(
    node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Option<TermsAndConditions>, SubxtNodeError> {
    match node_version {
        NodeVersion::V0_22 => v0_22_0::get_terms_and_conditions(block).await,
        NodeVersion::V1_0 => v1_0_0::get_terms_and_conditions(block).await,
        NodeVersion::V2_0 => v2_0_0::get_terms_and_conditions(block).await,
        NodeVersion::V2_1 => v2_1_0::get_terms_and_conditions(block).await,
    }
}

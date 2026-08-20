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

use crate::domain::DustRegistrationEvent;
use indexer_common::domain::{
    BlockAuthor, BlockHash, ByteVec, ProtocolVersion, SerializedDustCommitmentMerkleTreeRoot,
    SerializedDustGenerationMerkleTreeRoot, SerializedLedgerParameters,
    SerializedZswapMerkleTreeRoot, bridge::BridgeEvent,
};
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct Block {
    // These fields come from the node.
    pub hash: BlockHash,
    pub height: u64,
    pub protocol_version: ProtocolVersion,
    pub parent_hash: BlockHash,
    pub author: Option<BlockAuthor>,
    pub timestamp: u64,
    pub zswap_merkle_tree_root: SerializedZswapMerkleTreeRoot,
    // TODO: Remove Option once support for Node < 0.22 is dropped!
    pub ledger_state_root: Option<ByteVec>,
    pub dust_registration_events: Vec<DustRegistrationEvent>,
    /// c2m-bridge events (5 variants, see indexer-common::domain::bridge), decoded from
    /// the node 2.0+ runtime (`infra/subxt_node/runtimes/v2_0_0.rs`); always empty for earlier
    /// runtimes, where the pallet does not exist.
    pub bridge_events: Vec<BridgeEvent>,

    // These fields are set after applying all transactions of this block to the ledger state.
    pub ledger_parameters: SerializedLedgerParameters,
    pub zswap_end_index: u64,
    pub dust_commitment_end_index: u64,
    pub dust_generation_end_index: u64,
    pub dust_commitment_merkle_tree_root: SerializedDustCommitmentMerkleTreeRoot,
    pub dust_generation_merkle_tree_root: SerializedDustGenerationMerkleTreeRoot,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockRef {
    pub hash: BlockHash,
    pub height: u64,
}

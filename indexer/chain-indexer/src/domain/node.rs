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

use crate::domain::{
    self, BlockRef, ContractAction, DustRegistrationEvent, SystemParametersChange,
};
use futures::Stream;
use indexer_common::domain::{
    BlockAuthor, BlockHash, ByteVec, NodeVersion, ProtocolVersion, SerializedTransaction,
    SerializedTransactionIdentifier, TransactionHash,
    ledger::{self, ZswapMerkleTreeRoot},
};
use std::{error::Error as StdError, fmt::Debug};

/// Node abstraction.
#[trait_variant::make(Send)]
pub trait Node
where
    Self: Clone + Send + Sync + 'static,
{
    /// Error type for items of the stream of finalized [Block]s.
    type Error: StdError + Send + Sync + 'static;

    /// A stream of the latest/highest finalized blocks.
    async fn highest_blocks(
        &self,
    ) -> Result<impl Stream<Item = Result<BlockRef, Self::Error>> + Send, Self::Error>;

    /// A stream of finalized [Block]s in natural parent-child order without duplicates but possibly
    /// with gaps, starting after the given block.
    fn finalized_blocks(
        &mut self,
        after: Option<BlockRef>,
    ) -> impl Stream<Item = Result<Block, Self::Error>>;

    /// Fetch system parameters (D-Parameter and Terms & Conditions) at a given block.
    async fn fetch_system_parameters(
        &self,
        block_hash: BlockHash,
        block_height: u64,
        timestamp: u64,
        node_version: NodeVersion,
    ) -> Result<SystemParametersChange, Self::Error>;

    /// Fetch serialized genesis ledger state from the chain spec's system properties.
    /// Returns the raw bytes of the genesis `LedgerState`, errs if unavailable.
    async fn fetch_genesis_ledger_state(&self) -> Result<ByteVec, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct Block {
    pub hash: BlockHash,
    pub height: u64,
    pub protocol_version: ProtocolVersion,
    pub parent_hash: BlockHash,
    pub author: Option<BlockAuthor>,
    pub timestamp: u64,
    pub zswap_merkle_tree_root: ZswapMerkleTreeRoot,
    pub ledger_state_root: Option<ByteVec>,
    pub transactions: Vec<Transaction>,
    pub dust_registration_events: Vec<DustRegistrationEvent>,
    pub bridge_events: Vec<indexer_common::domain::bridge::BridgeEvent>,
}

impl TryFrom<Block> for (domain::Block, Vec<Transaction>) {
    type Error = ledger::Error;

    fn try_from(block: Block) -> Result<(domain::Block, Vec<Transaction>), Self::Error> {
        let zswap_merkle_tree_root = block.zswap_merkle_tree_root.serialize()?;

        let transactions = block.transactions;

        let block = domain::Block {
            hash: block.hash,
            height: block.height,
            protocol_version: block.protocol_version,
            parent_hash: block.parent_hash,
            author: block.author,
            timestamp: block.timestamp,
            zswap_merkle_tree_root,
            ledger_state_root: block.ledger_state_root,
            dust_registration_events: block.dust_registration_events,
            bridge_events: block.bridge_events,
            ledger_parameters: Default::default(),
            zswap_end_index: 0,
            dust_commitment_end_index: 0,
            dust_generation_end_index: 0,
            dust_commitment_merkle_tree_root: Default::default(),
            dust_generation_merkle_tree_root: Default::default(),
        };

        Ok((block, transactions))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transaction {
    Regular(RegularTransaction),
    System(SystemTransaction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularTransaction {
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    pub raw: SerializedTransaction,
    pub identifiers: Vec<SerializedTransactionIdentifier>,
    pub contract_actions: Vec<ContractAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTransaction {
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    pub raw: SerializedTransaction,
}

impl From<&Block> for BlockRef {
    fn from(block: &Block) -> Self {
        Self {
            hash: block.hash,
            height: block.height,
        }
    }
}

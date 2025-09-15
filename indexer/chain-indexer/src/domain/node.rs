// This file is part of midnight-indexer.
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

use crate::domain::{self, ContractAction};
use futures::Stream;
use indexer_common::domain::{
    BlockAuthor, BlockHash, ProtocolVersion,
    ledger::{
        SerializedTransaction, SerializedTransactionIdentifier, TransactionHash, ZswapStateRoot,
    },
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
    ) -> Result<impl Stream<Item = Result<BlockInfo, Self::Error>> + Send, Self::Error>;

    /// A stream of finalized [Block]s in natural parent-child order without duplicates but possibly
    /// with gaps, starting after the given block.
    fn finalized_blocks(
        &mut self,
        after: Option<BlockInfo>,
    ) -> impl Stream<Item = Result<Block, Self::Error>>;
}

#[derive(Debug, Clone)]
pub struct Block {
    pub hash: BlockHash,
    pub height: u32,
    pub protocol_version: ProtocolVersion,
    pub parent_hash: BlockHash,
    pub author: Option<BlockAuthor>,
    pub timestamp: u64,
    pub zswap_state_root: ZswapStateRoot,
    pub transactions: Vec<Transaction>,
}

impl From<Block> for (domain::Block, Vec<Transaction>) {
    fn from(block: Block) -> (domain::Block, Vec<Transaction>) {
        let transactions = block.transactions;
        let block = domain::Block {
            hash: block.hash,
            height: block.height,
            protocol_version: block.protocol_version,
            parent_hash: block.parent_hash,
            author: block.author,
            timestamp: block.timestamp,
            zswap_state_root: block.zswap_state_root,
        };

        (block, transactions)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlockInfo {
    pub hash: BlockHash,
    pub height: u32,
}

impl From<&Block> for BlockInfo {
    fn from(block: &Block) -> Self {
        Self {
            hash: block.hash,
            height: block.height,
        }
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
    pub paid_fees: u128,
    pub estimated_fees: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTransaction {
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    pub raw: SerializedTransaction,
}

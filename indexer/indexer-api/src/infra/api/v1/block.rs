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

use crate::{
    domain::{self, storage::Storage},
    infra::api::{
        ApiResult, ContextExt, ResultExt,
        v1::{AsBytesExt, HexEncoded, transaction::Transaction},
    },
};
use async_graphql::{ComplexObject, Context, OneofObject, SimpleObject};
use derive_more::Debug;
use indexer_common::domain::{BlockHash, ProtocolVersion};
use std::marker::PhantomData;

/// A block with its relevant data.
#[derive(Debug, SimpleObject)]
#[graphql(complex)]
pub struct Block<S>
where
    S: Storage,
{
    /// The block hash.
    hash: HexEncoded,

    /// The block height.
    height: u32,

    /// The protocol version.
    protocol_version: u32,

    /// The UNIX timestamp.
    timestamp: u64,

    /// The hex-encoded block author.
    author: Option<HexEncoded>,

    #[graphql(skip)]
    id: u64,

    #[graphql(skip)]
    parent_hash: BlockHash,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> Block<S>
where
    S: Storage,
{
    /// The parent of this block.
    async fn parent(&self, cx: &Context<'_>) -> ApiResult<Option<Block<S>>> {
        let block = cx
            .get_storage::<S>()
            .get_block_by_hash(self.parent_hash)
            .await
            .map_err_into_server_error(|| format!("get block by hash {}", self.parent_hash))?;

        Ok(block.map(Into::into))
    }

    /// The transactions within this block.
    async fn transactions(&self, cx: &Context<'_>) -> ApiResult<Vec<Transaction<S>>> {
        let transactions = cx
            .get_storage::<S>()
            .get_transactions_by_block_id(self.id)
            .await
            .map_err_into_server_error(|| format!("get transactions by block id {}", self.id))?;

        Ok(transactions.into_iter().map(Into::into).collect())
    }
}

impl<S> From<domain::Block> for Block<S>
where
    S: Storage,
{
    fn from(value: domain::Block) -> Self {
        let domain::Block {
            id,
            hash,
            height,
            protocol_version: ProtocolVersion(protocol_version),
            author,
            timestamp,
            parent_hash,
        } = value;

        Block {
            hash: hash.hex_encode(),
            height,
            protocol_version,
            author: author.map(|author| author.hex_encode()),
            timestamp,
            id,
            parent_hash,
            _s: PhantomData,
        }
    }
}

/// Either a block hash or a block height.
#[derive(Debug, OneofObject)]
pub enum BlockOffset {
    /// A hex-encoded block hash.
    Hash(HexEncoded),

    /// A block height.
    Height(u32),
}

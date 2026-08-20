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

use crate::domain::{Block, storage::NoopStorage};
use futures::{Stream, stream};
use indexer_common::domain::BlockHash;
use std::num::NonZeroU32;

#[trait_variant::make(Send)]
pub trait BlockStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get the latest block.
    async fn get_latest_block(&self) -> Result<Option<Block>, sqlx::Error>;

    /// Get blocks for the given hashes.
    async fn get_blocks_by_hashes(&self, hashes: &[BlockHash]) -> Result<Vec<Block>, sqlx::Error>;

    /// Get a block for the given block height.
    async fn get_block_by_height(&self, height: u32) -> Result<Option<Block>, sqlx::Error>;

    /// Get a stream of all blocks starting at the given height, ordered by block height.
    fn get_blocks(
        &self,
        height: u32,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<Block, sqlx::Error>> + Send;
}

#[allow(unused_variables)]
impl BlockStorage for NoopStorage {
    async fn get_latest_block(&self) -> Result<Option<Block>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_blocks_by_hashes(&self, _hashes: &[BlockHash]) -> Result<Vec<Block>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_block_by_height(&self, height: u32) -> Result<Option<Block>, sqlx::Error> {
        unimplemented!()
    }

    fn get_blocks(
        &self,
        height: u32,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<Block, sqlx::Error>> {
        stream::empty()
    }
}

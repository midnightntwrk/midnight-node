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

use crate::domain::{Block, BlockTransactions, Transaction, node::BlockInfo};

/// Storage abstraction.
#[trait_variant::make(Send)]
pub trait Storage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get the hash and height of the highest stored block.
    async fn get_highest_block_info(&self) -> Result<Option<BlockInfo>, sqlx::Error>;

    /// Get the number of stored transactions.
    async fn get_transaction_count(&self) -> Result<u64, sqlx::Error>;

    /// Get the number of stored contract actions: deploys, calls, updates.
    async fn get_contract_action_count(&self) -> Result<(u64, u64, u64), sqlx::Error>;

    /// Get all transactions with additional block data for the given block height.
    async fn get_block_transactions(
        &self,
        block_height: u32,
    ) -> Result<BlockTransactions, sqlx::Error>;

    /// Save the given block and return the max regular transaction ID.
    async fn save_block(
        &self,
        block: &Block,
        transactions: &[Transaction],
    ) -> Result<Option<u64>, sqlx::Error>;
}

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

use crate::domain::{Transaction, Wallet};
use indexer_common::domain::ViewingKey;
use std::{num::NonZeroUsize, time::Duration};
use uuid::Uuid;

/// Sqlx transaction for some database.
pub type SqlxTransaction<D> = sqlx::Transaction<'static, D>;

/// Storage abstraction. `acquire_lock` tries to acquire an application level lock and, if
/// successful, returns a transaction which is intended to be used by all the other functions.
#[trait_variant::make(Send)]
pub trait Storage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Sqlx transaction.
    type Database: sqlx::Database;

    /// Try to acquire an application level lock for the given session ID. Return a transaction if
    /// and only if possible.
    async fn acquire_lock(
        &mut self,
        wallet_id: Uuid,
    ) -> Result<Option<SqlxTransaction<Self::Database>>, sqlx::Error>;

    /// Get at most `limit` transactions starting at the given `from` ID; it is supposed that the
    /// IDs are a gapless strictly monotonically increasing sequence.
    async fn get_transactions(
        &self,
        from: u64,
        limit: NonZeroUsize,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<Vec<Transaction>, sqlx::Error>;

    /// For the given session ID, transactionally save the given relevant `transactions` and
    /// update the last indexed transaction ID.
    async fn save_relevant_transactions(
        &self,
        viewing_key: &ViewingKey,
        transactions: &[Transaction],
        last_indexed_transaction_id: u64,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<(), sqlx::Error>;

    /// Get the IDs of active walltes, thereby marking "old" ones inactive.
    async fn active_wallets(&self, ttl: Duration) -> Result<Vec<Uuid>, sqlx::Error>;

    /// Get the wallet with the given session ID.
    async fn get_wallet_by_id(
        &self,
        session_id: Uuid,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<Wallet, sqlx::Error>;
}

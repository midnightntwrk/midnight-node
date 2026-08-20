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

use crate::domain::storage::NoopStorage;
use indexer_common::domain::{SessionId, ViewingKey};
use uuid::Uuid;

#[trait_variant::make(Send)]
pub trait WalletStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Connect a wallet, i.e. add it to the active ones, and return a random session ID.
    /// If `start_index` is provided, transactions before that index are skipped.
    async fn connect_wallet(
        &self,
        viewing_key: &ViewingKey,
        start_index: Option<u64>,
    ) -> Result<SessionId, sqlx::Error>;

    /// Disconnect a wallet, i.e. remove it from the active ones.
    async fn disconnect_wallet(&self, session_id: SessionId) -> Result<(), sqlx::Error>;

    /// Resolve a session ID to the corresponding wallet ID.
    async fn resolve_session_id(&self, session_id: SessionId) -> Result<Option<Uuid>, sqlx::Error>;

    /// Refresh the wallet's last active timestamp to avoid timing out.
    async fn keep_wallet_active(&self, wallet_id: Uuid) -> Result<(), sqlx::Error>;
}

#[allow(unused_variables)]
impl WalletStorage for NoopStorage {
    async fn connect_wallet(
        &self,
        viewing_key: &ViewingKey,
        start_index: Option<u64>,
    ) -> Result<SessionId, sqlx::Error> {
        unimplemented!()
    }

    async fn disconnect_wallet(&self, session_id: SessionId) -> Result<(), sqlx::Error> {
        unimplemented!()
    }

    async fn resolve_session_id(&self, session_id: SessionId) -> Result<Option<Uuid>, sqlx::Error> {
        unimplemented!()
    }

    async fn keep_wallet_active(&self, wallet_id: Uuid) -> Result<(), sqlx::Error> {
        unimplemented!()
    }
}

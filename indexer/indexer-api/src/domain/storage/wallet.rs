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

use crate::domain::storage::NoopStorage;
use indexer_common::domain::{SessionId, ViewingKey};
use std::fmt::Debug;

/// Storage abstraction.
#[trait_variant::make(Send)]
pub trait WalletStorage
where
    Self: Debug + Clone + Send + Sync + 'static,
{
    /// Connect a wallet, i.e. add it to the active ones.
    async fn connect_wallet(&self, viewing_key: &ViewingKey) -> Result<(), sqlx::Error>;

    /// Disconnect a wallet, i.e. remove it from the active ones.
    async fn disconnect_wallet(&self, session_id: SessionId) -> Result<(), sqlx::Error>;

    /// Set the wallet active at the current timestamp to avoid timing out.
    async fn set_wallet_active(&self, session_id: SessionId) -> Result<(), sqlx::Error>;
}

#[allow(unused_variables)]
impl WalletStorage for NoopStorage {
    async fn connect_wallet(&self, viewing_key: &ViewingKey) -> Result<(), sqlx::Error> {
        unimplemented!()
    }

    async fn disconnect_wallet(&self, session_id: SessionId) -> Result<(), sqlx::Error> {
        unimplemented!()
    }

    async fn set_wallet_active(&self, session_id: SessionId) -> Result<(), sqlx::Error> {
        unimplemented!()
    }
}

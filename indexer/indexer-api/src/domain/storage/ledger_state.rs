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

use indexer_common::domain::{BlockHash, ProtocolVersion, SerializedLedgerStateKey};

use crate::domain::storage::NoopStorage;

#[trait_variant::make(Send)]
pub trait LedgerStateStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get the protocol version and ledger state key of the highest (tip) block.
    async fn get_highest_ledger_state(
        &self,
    ) -> Result<Option<(ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error>;

    /// Get the block id, height, protocol version, and ledger state key at a specific block,
    /// by hash.
    async fn get_ledger_state_at(
        &self,
        block_hash: BlockHash,
    ) -> Result<Option<(u64, u32, ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error>;
}

#[allow(unused_variables)]
impl LedgerStateStorage for NoopStorage {
    async fn get_highest_ledger_state(
        &self,
    ) -> Result<Option<(ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_ledger_state_at(
        &self,
        block_hash: BlockHash,
    ) -> Result<Option<(u64, u32, ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error> {
        unimplemented!()
    }
}

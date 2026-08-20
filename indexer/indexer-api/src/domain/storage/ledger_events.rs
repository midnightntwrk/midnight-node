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

use crate::domain::{LedgerEvent, storage::NoopStorage};
use futures::{Stream, stream};
use indexer_common::domain::LedgerEventGrouping;
use std::num::NonZeroU32;

#[trait_variant::make(Send)]
pub trait LedgerEventStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get a stream of ledger events for the given grouping starting at the given ID, ordered by
    /// ID.
    async fn get_ledger_events(
        &self,
        grouping: LedgerEventGrouping,
        id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<LedgerEvent, sqlx::Error>> + Send;

    /// Get the ledger events for the given grouping and transaction ID.
    async fn get_ledger_events_by_transaction_id(
        &self,
        grouping: LedgerEventGrouping,
        transaction_id: u64,
    ) -> Result<Vec<LedgerEvent>, sqlx::Error>;
}

#[allow(unused_variables)]
impl LedgerEventStorage for NoopStorage {
    async fn get_ledger_events(
        &self,
        grouping: LedgerEventGrouping,
        id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<LedgerEvent, sqlx::Error>> + Send {
        stream::empty()
    }

    async fn get_ledger_events_by_transaction_id(
        &self,
        grouping: LedgerEventGrouping,
        transaction_id: u64,
    ) -> Result<Vec<LedgerEvent>, sqlx::Error> {
        unimplemented!()
    }
}

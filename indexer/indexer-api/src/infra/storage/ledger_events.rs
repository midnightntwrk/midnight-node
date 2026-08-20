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

use crate::{
    domain::{LedgerEvent, storage::ledger_events::LedgerEventStorage},
    infra::storage::Storage,
};
use async_stream::try_stream;
use fastrace::trace;
use futures::Stream;
use indexer_common::{domain::LedgerEventGrouping, stream::flatten_chunks};
use indoc::indoc;
use std::num::NonZeroU32;

impl LedgerEventStorage for Storage {
    async fn get_ledger_events(
        &self,
        grouping: LedgerEventGrouping,
        mut id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<LedgerEvent, sqlx::Error>> + Send {
        let chunks = try_stream! {
            loop {
                let ledger_events = self.get_ledger_events(grouping, id, batch_size).await?;

                match ledger_events.last() {
                    Some(ledger_event) => id = ledger_event.id + 1,
                    None => break,
                }

                yield ledger_events;
            }
        };

        flatten_chunks(chunks)
    }

    async fn get_ledger_events_by_transaction_id(
        &self,
        grouping: LedgerEventGrouping,
        transaction_id: u64,
    ) -> Result<Vec<LedgerEvent>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                ledger_events.id,
                ledger_events.raw,
                ledger_events.attributes,
                (SELECT MAX(id) FROM ledger_events WHERE grouping = $1) AS max_id,
                transactions.protocol_version
            FROM ledger_events
            INNER JOIN transactions ON transactions.id = ledger_events.transaction_id
            WHERE ledger_events.grouping = $1
            AND ledger_events.transaction_id = $2
            ORDER BY ledger_events.id
        "};

        sqlx::query_as(query)
            .bind(grouping)
            .bind(transaction_id as i64)
            .fetch_all(&*self.pool)
            .await
    }
}

impl Storage {
    #[trace(properties = {
        "grouping": "{grouping:?}",
        "id": "{id}",
        "batch_size": "{batch_size}"
    })]
    async fn get_ledger_events(
        &self,
        grouping: LedgerEventGrouping,
        id: u64,
        batch_size: NonZeroU32,
    ) -> Result<Vec<LedgerEvent>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                ledger_events.id,
                ledger_events.raw,
                ledger_events.attributes,
                (SELECT MAX(id) FROM ledger_events WHERE grouping = $1) AS max_id,
                transactions.protocol_version
            FROM ledger_events
            INNER JOIN transactions ON transactions.id = ledger_events.transaction_id
            WHERE ledger_events.grouping = $1
            AND ledger_events.id >= $2
            ORDER BY ledger_events.id
            LIMIT $3
        "};

        sqlx::query_as(query)
            .bind(grouping)
            .bind(id as i64)
            .bind(batch_size.get() as i64)
            .fetch_all(&*self.pool)
            .await
    }
}

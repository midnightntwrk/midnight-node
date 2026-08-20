// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage impl for the `contractEvents` query and subscription surface (#1161). Pattern
//! follows `get_ledger_events` in `ledger_events.rs` but joins through `blocks` for
//! block-range filtering and through `contract_event_indexed_fields` for prefix lookup.

use crate::{
    domain::{
        ContractEventRow,
        storage::contract_event::{ContractEventFilter, ContractEventStorage},
    },
    infra::storage::Storage,
};
use async_stream::try_stream;
use fastrace::trace;
use futures::Stream;
use indexer_common::stream::flatten_chunks;
use indoc::indoc;
use std::num::NonZeroU32;

#[cfg(feature = "cloud")]
type Db = sqlx::Postgres;
#[cfg(feature = "standalone")]
type Db = sqlx::Sqlite;

impl ContractEventStorage for Storage {
    #[trace(properties = {
        "limit": "{limit}",
        "offset": "{offset}"
    })]
    async fn get_contract_events(
        &self,
        filter: &ContractEventFilter,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ContractEventRow>, sqlx::Error> {
        let mut query_builder = base_query_builder(filter);
        query_builder
            .push(" ORDER BY ledger_events.id LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        query_builder
            .build_query_as::<ContractEventRow>()
            .fetch_all(&*self.pool)
            .await
    }

    async fn get_contract_events_from_id(
        &self,
        filter: &ContractEventFilter,
        mut id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ContractEventRow, sqlx::Error>> + Send {
        let chunks = try_stream! {
            loop {
                let rows = self.get_contract_events_chunk(filter, id, batch_size).await?;

                match rows.last() {
                    Some(last) => id = last.id + 1,
                    None => break,
                }

                yield rows;
            }
        };

        flatten_chunks(chunks)
    }

    #[trace]
    async fn get_contract_events_by_contract_action_ids(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, ContractEventRow)>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        #[cfg(feature = "cloud")]
        let rows = {
            let query = indoc! {"
                SELECT
                    ledger_events.id,
                    ledger_events.contract_address,
                    ledger_events.transaction_id,
                    ledger_events.contract_action_id,
                    ledger_events.raw,
                    ledger_events.attributes,
                    (SELECT MAX(id) FROM ledger_events WHERE grouping = 'Contract') AS max_id,
                    transactions.protocol_version
                FROM ledger_events
                INNER JOIN transactions ON transactions.id = ledger_events.transaction_id
                WHERE ledger_events.grouping = 'Contract'
                AND ledger_events.contract_action_id = ANY($1)
                ORDER BY ledger_events.id
            "};

            let ids = ids.iter().map(|id| *id as i64).collect::<Vec<_>>();

            sqlx::query_as::<_, ContractEventRow>(query)
                .bind(ids)
                .fetch_all(&*self.pool)
                .await?
        };

        #[cfg(feature = "standalone")]
        let rows = {
            let mut query_builder = sqlx::QueryBuilder::<Db>::new(indoc! {"
                SELECT
                    ledger_events.id,
                    ledger_events.contract_address,
                    ledger_events.transaction_id,
                    ledger_events.contract_action_id,
                    ledger_events.raw,
                    ledger_events.attributes,
                    (SELECT MAX(id) FROM ledger_events WHERE grouping = 'Contract') AS max_id,
                    transactions.protocol_version
                FROM ledger_events
                INNER JOIN transactions ON transactions.id = ledger_events.transaction_id
                WHERE ledger_events.grouping = 'Contract'
                AND ledger_events.contract_action_id IN (
            "});

            let mut separated = query_builder.separated(", ");
            for id in ids {
                separated.push_bind(*id as i64);
            }
            query_builder.push(") ORDER BY ledger_events.id");

            query_builder
                .build_query_as::<ContractEventRow>()
                .fetch_all(&*self.pool)
                .await?
        };

        Ok(rows
            .into_iter()
            .filter_map(|row| row.contract_action_id.map(|key| (key, row)))
            .collect())
    }
}

impl Storage {
    #[trace(properties = {
        "id": "{id}",
        "batch_size": "{batch_size}"
    })]
    async fn get_contract_events_chunk(
        &self,
        filter: &ContractEventFilter,
        id: u64,
        batch_size: NonZeroU32,
    ) -> Result<Vec<ContractEventRow>, sqlx::Error> {
        let mut query_builder = base_query_builder(filter);
        query_builder
            .push(" AND ledger_events.id >= ")
            .push_bind(id as i64)
            .push(" ORDER BY ledger_events.id LIMIT ")
            .push_bind(batch_size.get() as i64);

        query_builder
            .build_query_as::<ContractEventRow>()
            .fetch_all(&*self.pool)
            .await
    }
}

/// Shared SELECT + WHERE construction for both the query and subscription paths; the caller
/// appends ORDER/LIMIT/OFFSET as appropriate.
fn base_query_builder<'a>(filter: &'a ContractEventFilter) -> sqlx::QueryBuilder<'a, Db> {
    let mut query_builder = sqlx::QueryBuilder::<Db>::new(indoc! {"
        SELECT
            ledger_events.id,
            ledger_events.contract_address,
            ledger_events.transaction_id,
            ledger_events.contract_action_id,
            ledger_events.raw,
            ledger_events.attributes,
            (SELECT MAX(id) FROM ledger_events WHERE grouping = 'Contract') AS max_id,
            transactions.protocol_version
        FROM ledger_events
        INNER JOIN transactions ON transactions.id = ledger_events.transaction_id
        INNER JOIN blocks ON blocks.id = transactions.block_id
        WHERE ledger_events.grouping = 'Contract'
    "});

    query_builder
        .push(" AND ledger_events.contract_address = ")
        .push_bind(filter.contract_address.as_ref());

    if !filter.variants.is_empty() {
        // The variant column is a Postgres enum (cast to text to compare) and a SQLite TEXT.
        #[cfg(feature = "cloud")]
        query_builder
            .push(" AND ledger_events.variant::text = ANY(")
            .push_bind(&filter.variants)
            .push(") ");

        #[cfg(feature = "standalone")]
        {
            query_builder.push(" AND ledger_events.variant IN (");
            let mut separated = query_builder.separated(", ");
            for variant in &filter.variants {
                separated.push_bind(*variant);
            }
            query_builder.push(") ");
        }
    }

    if let Some(from_block) = filter.from_block {
        query_builder
            .push(" AND blocks.height >= ")
            .push_bind(from_block as i64);
    }
    if let Some(to_block) = filter.to_block {
        query_builder
            .push(" AND blocks.height <= ")
            .push_bind(to_block as i64);
    }
    if let Some(transaction_hash) = &filter.transaction_hash {
        query_builder
            .push(" AND transactions.hash = ")
            .push_bind(transaction_hash.as_ref());
    }

    for (index, field_prefix) in filter.field_prefixes.iter().enumerate() {
        let alias = format!("cef{index}");
        query_builder
            .push(format!(
                " AND EXISTS (SELECT 1 FROM contract_event_indexed_fields {alias} "
            ))
            .push(format!(
                "WHERE {alias}.ledger_event_id = ledger_events.id AND {alias}.field_name = "
            ))
            .push_bind(&field_prefix.field_name)
            .push(format!(" AND substr({alias}.field_value, 1, "));

        // Postgres substr takes an int4 length; SQLite an integer.
        #[cfg(feature = "cloud")]
        query_builder.push_bind(field_prefix.prefix.len() as i32);
        #[cfg(feature = "standalone")]
        query_builder.push_bind(field_prefix.prefix.len() as i64);

        query_builder
            .push(") = ")
            .push_bind(field_prefix.prefix.as_ref())
            .push(") ");
    }

    query_builder
}

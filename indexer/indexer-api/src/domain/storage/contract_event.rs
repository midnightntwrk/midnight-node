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

use crate::domain::{ContractEventRow, storage::NoopStorage};
use futures::{Stream, stream};
use indexer_common::domain::{ByteVec, SerializedContractAddress, TransactionHash};
use std::num::NonZeroU32;

/// Filter accepted by the indexer-api storage layer for both the contract events query and
/// subscription paths.
#[derive(Debug, Clone)]
pub struct ContractEventFilter {
    /// The emitting contract address.
    pub contract_address: SerializedContractAddress,

    /// Event variant names (per `LEDGER_EVENT_VARIANT`) to narrow to; empty means no type
    /// filter.
    pub variants: Vec<&'static str>,

    /// Prefix matches on indexed-field rows in `contract_event_indexed_fields`, combined with
    /// AND semantics.
    pub field_prefixes: Vec<FieldPrefix>,

    /// Optional lower bound on block height.
    pub from_block: Option<u32>,

    /// Optional upper bound on block height.
    pub to_block: Option<u32>,

    /// Optional transaction hash to narrow to events emitted by that transaction.
    pub transaction_hash: Option<TransactionHash>,
}

/// A prefix match on one indexed event field.
#[derive(Debug, Clone)]
pub struct FieldPrefix {
    /// The indexed field name.
    pub field_name: String,

    /// The field value prefix to match.
    pub prefix: ByteVec,
}

#[trait_variant::make(Send)]
pub trait ContractEventStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get the contract events matching the filter, ordered by ID, windowed by the given limit
    /// and offset.
    async fn get_contract_events(
        &self,
        filter: &ContractEventFilter,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ContractEventRow>, sqlx::Error>;

    /// Get a stream of contract events matching the filter, starting at the given event ID
    /// (inclusive), ordered by ID.
    async fn get_contract_events_from_id(
        &self,
        filter: &ContractEventFilter,
        id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ContractEventRow, sqlx::Error>> + Send;

    /// Batched lookup keyed on `contract_action_id` for the `ContractCall.contractEvents`
    /// nested field DataLoader (#1162). Returns `(contract_action_id, ContractEventRow)` pairs
    /// covering every requested key; rows without a populated `contract_action_id` are omitted.
    async fn get_contract_events_by_contract_action_ids(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, ContractEventRow)>, sqlx::Error>;
}

#[allow(unused_variables)]
impl ContractEventStorage for NoopStorage {
    async fn get_contract_events(
        &self,
        filter: &ContractEventFilter,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ContractEventRow>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_contract_events_from_id(
        &self,
        filter: &ContractEventFilter,
        id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ContractEventRow, sqlx::Error>> + Send {
        stream::empty()
    }

    async fn get_contract_events_by_contract_action_ids(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, ContractEventRow)>, sqlx::Error> {
        Ok(vec![])
    }
}

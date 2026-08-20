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

//! Domain representation of a contract event row read from `ledger_events`
//! plus the `contract_address` envelope column. Used by the `contractEvents`
//! query and subscription resolvers.

use indexer_common::{
    domain::{ByteVec, LedgerEventAttributes, ProtocolVersion, SerializedLedgerEvent},
    infra::sqlx::SqlxOption,
};
use sqlx::prelude::FromRow;

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ContractEventRow {
    #[sqlx(try_from = "i64")]
    pub id: u64,

    /// Emitting contract address from `ledger_events.contract_address`.
    /// Required for contract events (grouping = 'Contract').
    pub contract_address: ByteVec,

    /// The originating transaction id, surfaced to clients for
    /// `Transaction.contractActions[*]` resolution.
    #[sqlx(try_from = "i64")]
    pub transaction_id: u64,

    /// The contract_action_id linking the event to the specific `ContractCall`
    /// that emitted it. `None` when attribution is ambiguous (several calls in
    /// one transaction sharing contract address and entry point) and for rows
    /// indexed before the correlation landed; such events are excluded from
    /// the nested `ContractCall.contractEvents` surface but stay reachable via
    /// the top-level `contractEvents` query.
    #[sqlx(try_from = "SqlxOption<i64>", default)]
    pub contract_action_id: Option<u64>,

    pub raw: SerializedLedgerEvent,

    #[sqlx(json)]
    pub attributes: LedgerEventAttributes,

    #[sqlx(try_from = "i64")]
    pub max_id: u64,

    #[sqlx(try_from = "i64")]
    pub protocol_version: ProtocolVersion,
}

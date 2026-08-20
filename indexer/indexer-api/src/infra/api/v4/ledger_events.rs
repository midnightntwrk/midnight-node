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
    domain::LedgerEvent,
    infra::api::v4::{HexEncodable, HexEncoded},
};
use async_graphql::{Interface, SimpleObject};
use indexer_common::domain::LedgerEventAttributes;
use thiserror::Error;

/// A zswap related ledger event.
#[derive(Debug, SimpleObject)]
pub struct ZswapLedgerEvent {
    /// The ID of this zswap ledger event.
    id: u64,

    /// The hex-encoded serialized event.
    raw: HexEncoded,

    /// The maximum ID of all zswap ledger events.
    max_id: u64,

    /// The protocol version.
    protocol_version: u32,
}

impl TryFrom<LedgerEvent> for ZswapLedgerEvent {
    type Error = UnexpectedLedgerEvent;

    fn try_from(ledger_event: LedgerEvent) -> Result<Self, Self::Error> {
        match ledger_event.attributes {
            LedgerEventAttributes::ZswapInput { .. } | LedgerEventAttributes::ZswapOutput => {
                Ok(Self {
                    id: ledger_event.id,
                    raw: ledger_event.raw.hex_encode(),
                    max_id: ledger_event.max_id,
                    protocol_version: ledger_event.protocol_version.into(),
                })
            }

            other => Err(UnexpectedLedgerEvent(other)),
        }
    }
}

/// A dust related ledger event.
#[derive(Debug, Interface)]
#[allow(clippy::duplicated_attributes)]
#[graphql(
    field(name = "id", ty = "&u64"),
    field(name = "raw", ty = "&HexEncoded"),
    field(name = "max_id", ty = "&u64"),
    field(name = "protocol_version", ty = "&u32")
)]
pub enum DustLedgerEvent {
    // A general parameter change; possibly conveys modified dust related parameters like
    // generation rate or decay.
    ParamChange(ParamChange),

    // An initial dust UTXO.
    DustInitialUtxo(DustInitialUtxo),

    // A dtime update for a dust generation.
    DustGenerationDtimeUpdate(DustGenerationDtimeUpdate),

    // A processed dust spend.
    DustSpendProcessed(DustSpendProcessed),
}

impl TryFrom<LedgerEvent> for DustLedgerEvent {
    type Error = UnexpectedLedgerEvent;

    fn try_from(ledger_event: LedgerEvent) -> Result<Self, Self::Error> {
        match ledger_event.attributes {
            LedgerEventAttributes::ParamChange => Ok(DustLedgerEvent::ParamChange(ParamChange {
                id: ledger_event.id,
                raw: ledger_event.raw.hex_encode(),
                max_id: ledger_event.max_id,
                protocol_version: ledger_event.protocol_version.into(),
            })),

            LedgerEventAttributes::DustInitialUtxo { output, .. } => {
                Ok(DustLedgerEvent::DustInitialUtxo(DustInitialUtxo {
                    id: ledger_event.id,
                    raw: ledger_event.raw.hex_encode(),
                    max_id: ledger_event.max_id,
                    protocol_version: ledger_event.protocol_version.into(),
                    output: DustOutput {
                        nonce: output.nonce.hex_encode(),
                    },
                }))
            }

            LedgerEventAttributes::DustGenerationDtimeUpdate { .. } => Ok(
                DustLedgerEvent::DustGenerationDtimeUpdate(DustGenerationDtimeUpdate {
                    id: ledger_event.id,
                    raw: ledger_event.raw.hex_encode(),
                    max_id: ledger_event.max_id,
                    protocol_version: ledger_event.protocol_version.into(),
                }),
            ),

            LedgerEventAttributes::DustSpendProcessed { .. } => {
                Ok(DustLedgerEvent::DustSpendProcessed(DustSpendProcessed {
                    id: ledger_event.id,
                    raw: ledger_event.raw.hex_encode(),
                    max_id: ledger_event.max_id,
                    protocol_version: ledger_event.protocol_version.into(),
                }))
            }

            other => Err(UnexpectedLedgerEvent(other)),
        }
    }
}

#[derive(Debug, Error)]
#[error("unexpected ledger event {0:?}")]
pub struct UnexpectedLedgerEvent(LedgerEventAttributes);

#[derive(Debug, SimpleObject)]
// A general parameter change; possibly conveys modified dust related parameters like
// generation rate or decay.
pub struct ParamChange {
    /// The ID of this dust ledger event.
    id: u64,

    /// The hex-encoded serialized event.
    raw: HexEncoded,

    /// The maximum ID of all dust ledger events.
    max_id: u64,

    /// The protocol version.
    protocol_version: u32,
}

// An initial dust UTXO.
#[derive(Debug, SimpleObject)]
pub struct DustInitialUtxo {
    /// The ID of this dust ledger event.
    id: u64,

    /// The hex-encoded serialized event.
    raw: HexEncoded,

    /// The maximum ID of all dust ledger events.
    max_id: u64,

    /// The protocol version.
    protocol_version: u32,

    /// The dust output.
    output: DustOutput,
}

// A dtime update for a dust generation.
#[derive(Debug, SimpleObject)]
pub struct DustGenerationDtimeUpdate {
    /// The ID of this dust ledger event.
    id: u64,

    /// The hex-encoded serialized event.
    raw: HexEncoded,

    /// The maximum ID of all dust ledger events.
    max_id: u64,

    /// The protocol version.
    protocol_version: u32,
}

// A processed dust spend.
#[derive(Debug, SimpleObject)]
pub struct DustSpendProcessed {
    /// The ID of this dust ledger event.
    id: u64,

    /// The hex-encoded serialized event.
    raw: HexEncoded,

    /// The maximum ID of all dust ledger events.
    max_id: u64,

    /// The protocol version.
    protocol_version: u32,
}

/// A dust output.
#[derive(Debug, SimpleObject)]
pub struct DustOutput {
    /// The hex-encoded 32-byte nonce.
    nonce: HexEncoded,
}

impl From<indexer_common::domain::DustOutput> for DustOutput {
    fn from(dust_output: indexer_common::domain::DustOutput) -> Self {
        Self {
            nonce: dust_output.nonce.hex_encode(),
        }
    }
}

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

//! GraphQL types for c2m-bridge events, claims and pool observability.

use crate::{
    domain::bridge as domain_bridge,
    infra::api::v4::{HexEncodable, HexEncoded, directives::beta},
};
use async_graphql::{Enum, Interface, SimpleObject};
use indexer_common::domain::bridge::BridgeEventVariant as DomainBridgeEventVariant;

/// GraphQL discriminator for bridge events.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BridgeEventVariant {
    UserTransfer,
    ReserveTransfer,
    InvalidTransfer,
    UnapprovedTransfer,
    SubminimalFlushTransfer,
}

impl From<DomainBridgeEventVariant> for BridgeEventVariant {
    fn from(v: DomainBridgeEventVariant) -> Self {
        match v {
            DomainBridgeEventVariant::UserTransfer => Self::UserTransfer,
            DomainBridgeEventVariant::ReserveTransfer => Self::ReserveTransfer,
            DomainBridgeEventVariant::InvalidTransfer => Self::InvalidTransfer,
            DomainBridgeEventVariant::UnapprovedTransfer => Self::UnapprovedTransfer,
            DomainBridgeEventVariant::SubminimalFlushTransfer => Self::SubminimalFlushTransfer,
        }
    }
}

impl From<BridgeEventVariant> for DomainBridgeEventVariant {
    fn from(v: BridgeEventVariant) -> Self {
        match v {
            BridgeEventVariant::UserTransfer => Self::UserTransfer,
            BridgeEventVariant::ReserveTransfer => Self::ReserveTransfer,
            BridgeEventVariant::InvalidTransfer => Self::InvalidTransfer,
            BridgeEventVariant::UnapprovedTransfer => Self::UnapprovedTransfer,
            BridgeEventVariant::SubminimalFlushTransfer => Self::SubminimalFlushTransfer,
        }
    }
}

/// GraphQL discriminator for treasury redirection reasons.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BridgeTreasuryReason {
    Invalid,
    Unapproved,
    SubminimalFlush,
}

impl From<BridgeTreasuryReason> for domain_bridge::TreasuryReason {
    fn from(v: BridgeTreasuryReason) -> Self {
        match v {
            BridgeTreasuryReason::Invalid => Self::Invalid,
            BridgeTreasuryReason::Unapproved => Self::Unapproved,
            BridgeTreasuryReason::SubminimalFlush => Self::SubminimalFlush,
        }
    }
}

/// Approved user deposit. NIGHT credited to `recipient`.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeUserTransfer {
    pub id: u64,
    pub block_height: u64,
    pub midnight_tx_hash: HexEncoded,
    pub cardano_tx_hash: HexEncoded,
    /// Amount as 8-byte big-endian u64 NIGHT (in stars).
    pub amount: HexEncoded,
    pub recipient: HexEncoded,
}

/// Reserve top-up. NIGHT credited to the protocol Reserve pool.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeReserveTransfer {
    pub id: u64,
    pub block_height: u64,
    pub midnight_tx_hash: HexEncoded,
    pub cardano_tx_hash: HexEncoded,
    pub amount: HexEncoded,
}

/// Malformed bridge metadata. NIGHT redirected to treasury.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeInvalidTransfer {
    pub id: u64,
    pub block_height: u64,
    pub midnight_tx_hash: HexEncoded,
    pub cardano_tx_hash: HexEncoded,
    pub amount: HexEncoded,
}

/// User deposit not in `ApprovedTransactions` at observation time. NIGHT redirected to treasury.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeUnapprovedTransfer {
    pub id: u64,
    pub block_height: u64,
    pub midnight_tx_hash: HexEncoded,
    pub cardano_tx_hash: HexEncoded,
    pub amount: HexEncoded,
    /// The recipient parsed from metadata (would have received NIGHT if approved).
    pub recipient: HexEncoded,
}

/// Aggregated subminimum transfers flushed to treasury.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeSubminimalFlushTransfer {
    pub id: u64,
    pub block_height: u64,
    pub midnight_tx_hash: HexEncoded,
    pub amount: HexEncoded,
    /// Number of subminimum Cardano txs aggregated into this flush.
    pub count: u32,
}

/// Polymorphic c2m-bridge event. Each concrete variant exposes its own additional fields.
#[derive(Debug, Clone, Interface)]
#[allow(clippy::duplicated_attributes)]
#[graphql(
    field(name = "id", ty = "&u64"),
    field(name = "block_height", ty = "&u64"),
    field(name = "midnight_tx_hash", ty = "&HexEncoded")
)]
pub enum BridgeEvent {
    UserTransfer(BridgeUserTransfer),
    ReserveTransfer(BridgeReserveTransfer),
    InvalidTransfer(BridgeInvalidTransfer),
    UnapprovedTransfer(BridgeUnapprovedTransfer),
    SubminimalFlushTransfer(BridgeSubminimalFlushTransfer),
}

impl From<domain_bridge::BridgeEvent> for BridgeEvent {
    fn from(e: domain_bridge::BridgeEvent) -> Self {
        let id = e.id;
        let block_height = e.block_height;
        let midnight_tx_hash = e.midnight_tx_hash.hex_encode();
        let amount_bytes = e.amount.to_be_bytes();
        let amount = amount_bytes.hex_encode();

        // mc_tx_hash is required for non-aggregate variants; recipient is required for the user
        // variants. Fall back to an empty `HexEncoded` if the DB row violates the invariant
        // (treated as a hard error elsewhere if needed).
        let mc_or_empty = || {
            e.mc_tx_hash
                .as_ref()
                .map(|h| h.hex_encode())
                .unwrap_or_else(|| [].as_slice().hex_encode())
        };
        let recipient_or_empty = || {
            e.recipient
                .as_ref()
                .map(|r| r.as_bytes().hex_encode())
                .unwrap_or_else(|| [].as_slice().hex_encode())
        };

        match e.variant {
            DomainBridgeEventVariant::UserTransfer => Self::UserTransfer(BridgeUserTransfer {
                id,
                block_height,
                midnight_tx_hash,
                cardano_tx_hash: mc_or_empty(),
                amount,
                recipient: recipient_or_empty(),
            }),
            DomainBridgeEventVariant::ReserveTransfer => {
                Self::ReserveTransfer(BridgeReserveTransfer {
                    id,
                    block_height,
                    midnight_tx_hash,
                    cardano_tx_hash: mc_or_empty(),
                    amount,
                })
            }
            DomainBridgeEventVariant::InvalidTransfer => {
                Self::InvalidTransfer(BridgeInvalidTransfer {
                    id,
                    block_height,
                    midnight_tx_hash,
                    cardano_tx_hash: mc_or_empty(),
                    amount,
                })
            }
            DomainBridgeEventVariant::UnapprovedTransfer => {
                Self::UnapprovedTransfer(BridgeUnapprovedTransfer {
                    id,
                    block_height,
                    midnight_tx_hash,
                    cardano_tx_hash: mc_or_empty(),
                    amount,
                    recipient: recipient_or_empty(),
                })
            }
            DomainBridgeEventVariant::SubminimalFlushTransfer => {
                Self::SubminimalFlushTransfer(BridgeSubminimalFlushTransfer {
                    id,
                    block_height,
                    midnight_tx_hash,
                    amount,
                    count: e.count.unwrap_or(0),
                })
            }
        }
    }
}

/// Per-address bridge balance.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeBalance {
    /// Sum of UserTransfer amounts for this address, gross/pre-fee (16-byte big-endian u128).
    pub deposited: HexEncoded,
    /// Sum of bridge claim amounts for this address, net/post-fee (16-byte big-endian u128).
    pub claimed: HexEncoded,
    /// Remaining claimable, read from the ledger's `bridge_receiving` map (net, zero once fully
    /// claimed). 16-byte big-endian u128.
    pub balance: HexEncoded,
}

impl From<domain_bridge::BridgeBalance> for BridgeBalance {
    fn from(b: domain_bridge::BridgeBalance) -> Self {
        Self {
            deposited: b.deposited.to_be_bytes().hex_encode(),
            claimed: b.claimed.to_be_bytes().hex_encode(),
            balance: b.balance.to_be_bytes().hex_encode(),
        }
    }
}

/// Treasury inflow aggregate by reason.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgeTreasuryAggregate {
    pub reason: BridgeTreasuryReason,
    /// Cumulative amount (16-byte big-endian u128).
    pub total: HexEncoded,
    pub count: u64,
}

/// Aggregate bridge inflows snapshot.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgePoolSummary {
    /// Cumulative ReserveTransfer amount (16-byte big-endian u128).
    pub reserve_total: HexEncoded,
    pub treasury_by_reason: Vec<BridgeTreasuryAggregate>,
    /// Sum of `count` from SubminimalFlushTransfer events.
    pub subminimum_tx_count: u64,
    pub last_event_block_height: Option<u64>,
}

impl From<domain_bridge::BridgePoolSummary> for BridgePoolSummary {
    fn from(s: domain_bridge::BridgePoolSummary) -> Self {
        let treasury_by_reason = s
            .treasury_by_reason
            .into_iter()
            .filter_map(|agg| {
                let reason = match agg.reason {
                    DomainBridgeEventVariant::InvalidTransfer => BridgeTreasuryReason::Invalid,
                    DomainBridgeEventVariant::UnapprovedTransfer => {
                        BridgeTreasuryReason::Unapproved
                    }
                    DomainBridgeEventVariant::SubminimalFlushTransfer => {
                        BridgeTreasuryReason::SubminimalFlush
                    }
                    _ => return None,
                };
                Some(BridgeTreasuryAggregate {
                    reason,
                    total: agg.total.to_be_bytes().hex_encode(),
                    count: agg.count,
                })
            })
            .collect();

        Self {
            reserve_total: s.reserve_total.to_be_bytes().hex_encode(),
            treasury_by_reason,
            subminimum_tx_count: s.subminimum_tx_count,
            last_event_block_height: s.last_event_block_height,
        }
    }
}

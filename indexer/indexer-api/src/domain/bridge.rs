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

//! Domain types for the c2m-bridge GraphQL API.
//!
//! These mirror the persisted shape of `bridge_events` and `bridge_claims` rather
//! than the on-chain pallet event structure. The raw pallet event types live in
//! `indexer_common::domain::bridge`.

use indexer_common::domain::{
    UnshieldedAddress,
    bridge::{BridgeRecipient, McTxHash, MidnightTxHash},
};

/// Event variant discriminator. Re-exported here for convenience in storage queries.
pub use indexer_common::domain::bridge::BridgeEventVariant;

/// A persisted c2m-bridge event row, enriched with block context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeEvent {
    pub id: u64,
    pub block_height: u64,
    pub transaction_id: Option<u64>,
    pub variant: BridgeEventVariant,
    pub mc_tx_hash: Option<McTxHash>,
    pub amount: u64,
    pub recipient: Option<BridgeRecipient>,
    pub midnight_tx_hash: MidnightTxHash,
    pub count: Option<u32>,
}

/// The bridge-claim payload of a regular `ClaimRewardsTransaction` with `ClaimKind::CardanoBridge`,
/// looked up from `bridge_claims` and attached to the owning `RegularTransaction` so the API can
/// surface it as a `BridgeClaimTransaction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeClaim {
    pub recipient: UnshieldedAddress,
    pub amount: u128,
}

/// Aggregated balance snapshot for a single recipient address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeBalance {
    /// Sum of `UserTransfer` amounts (gross, pre-fee) over events.
    pub deposited: u128,
    /// Sum of bridge claim amounts (net, post-fee) over events.
    pub claimed: u128,
    /// Authoritative remaining-claimable, read from the ledger's `bridge_receiving` map (net,
    /// zero once fully claimed). Not `deposited - claimed`, which would carry the fee as a
    /// residual.
    pub balance: u128,
}

/// A row of treasury inflow aggregated by reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeTreasuryAggregate {
    pub reason: BridgeEventVariant,
    pub total: u128,
    pub count: u64,
}

/// Aggregate snapshot of bridge inflows to protocol pools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgePoolSummary {
    pub reserve_total: u128,
    pub treasury_by_reason: Vec<BridgeTreasuryAggregate>,
    pub subminimum_tx_count: u64,
    pub last_event_block_height: Option<u64>,
}

/// Filter for `bridge_treasury_inflows` queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreasuryReason {
    Invalid,
    Unapproved,
    SubminimalFlush,
}

impl TreasuryReason {
    pub fn as_variant(&self) -> BridgeEventVariant {
        match self {
            Self::Invalid => BridgeEventVariant::InvalidTransfer,
            Self::Unapproved => BridgeEventVariant::UnapprovedTransfer,
            Self::SubminimalFlush => BridgeEventVariant::SubminimalFlushTransfer,
        }
    }
}

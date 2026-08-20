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

use crate::domain::{
    bridge::{BridgeBalance, BridgeEvent, BridgePoolSummary, TreasuryReason},
    storage::NoopStorage,
};
use indexer_common::domain::{UnshieldedAddress, bridge::BridgeEventVariant};

/// Filters for `get_bridge_events`. All fields combined with AND; empty `variants` matches all
/// variants.
#[derive(Debug, Default, Clone)]
pub struct BridgeEventFilter {
    pub variants: Vec<BridgeEventVariant>,
    pub recipient: Option<UnshieldedAddress>,
    pub block_height_from: Option<u64>,
    pub block_height_to: Option<u64>,
    pub id_from: Option<u64>,
}

#[trait_variant::make(Send)]
pub trait BridgeStorage
where
    Self: Send + Sync,
{
    /// Fetch bridge events filtered by the given criteria, paginated.
    async fn get_bridge_events(
        &self,
        filter: &BridgeEventFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error>;

    /// Compute deposited and claimed totals for a recipient address.
    async fn get_bridge_balance(
        &self,
        recipient: UnshieldedAddress,
    ) -> Result<BridgeBalance, sqlx::Error>;

    /// Fetch ReserveTransfer events, optionally bounded by block range.
    async fn get_bridge_reserve_inflows(
        &self,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error>;

    /// Fetch treasury-redirected events, optionally filtered by reason and block range.
    async fn get_bridge_treasury_inflows(
        &self,
        reason: Option<TreasuryReason>,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error>;

    /// Compute the bridge pool summary at the given block (or latest indexed if None).
    async fn get_bridge_pool_summary(
        &self,
        at_block_height: Option<u64>,
    ) -> Result<BridgePoolSummary, sqlx::Error>;
}

#[allow(unused_variables)]
impl BridgeStorage for NoopStorage {
    async fn get_bridge_events(
        &self,
        filter: &BridgeEventFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_bridge_balance(
        &self,
        recipient: UnshieldedAddress,
    ) -> Result<BridgeBalance, sqlx::Error> {
        Ok(BridgeBalance {
            deposited: 0,
            claimed: 0,
            balance: 0,
        })
    }

    async fn get_bridge_reserve_inflows(
        &self,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_bridge_treasury_inflows(
        &self,
        reason: Option<TreasuryReason>,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_bridge_pool_summary(
        &self,
        at_block_height: Option<u64>,
    ) -> Result<BridgePoolSummary, sqlx::Error> {
        Ok(BridgePoolSummary {
            reserve_total: 0,
            treasury_by_reason: vec![],
            subminimum_tx_count: 0,
            last_event_block_height: None,
        })
    }
}

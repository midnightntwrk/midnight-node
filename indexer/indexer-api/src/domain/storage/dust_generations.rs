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
    dust::{
        DustGenerationDtimeUpdateEntry, DustGenerationEntry, DustGenerations,
        DustNullifierTransaction,
    },
    storage::NoopStorage,
};
use futures::{Stream, stream};
use indexer_common::domain::{CardanoRewardAddress, LedgerVersion};
use std::num::NonZeroU32;

/// Storage for dust generations queries and subscriptions.
#[trait_variant::make(Send)]
pub trait DustGenerationsStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get all active registrations with aggregated generation stats for stake keys.
    async fn get_dust_generations(
        &self,
        cardano_reward_addresses: &[CardanoRewardAddress],
        ledger_version: LedgerVersion,
    ) -> Result<Vec<DustGenerations>, sqlx::Error>;

    /// Get dust generation entries for a dust address within a generation-tree
    /// index range. `start_index` and `end_index` are interpreted as positions
    /// in the dust generation Merkle tree (NOT the commitment tree). Entries
    /// inserted before the dust generation/commitment split was tracked
    /// (legacy rows with NULL `generation_index`) are skipped.
    async fn get_dust_generation_entries(
        &self,
        dust_address: &[u8],
        start_index: u64,
        end_index: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustGenerationEntry, sqlx::Error>> + Send;

    /// Get dust generation dtime update events for a dust address whose
    /// transaction's block_id is in `(cutoff_block_id, upper_block_id]` and
    /// whose ledger event id exceeds `after_event_id`. The stream is ordered
    /// by ledger event id.
    async fn get_dust_generation_dtime_updates(
        &self,
        dust_address: &[u8],
        cutoff_block_id: u64,
        upper_block_id: u64,
        after_event_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustGenerationDtimeUpdateEntry, sqlx::Error>> + Send;

    /// Get transactions containing dust nullifiers matching a prefix.
    async fn get_dust_nullifier_transactions(
        &self,
        nullifier_prefixes: &[Vec<u8>],
        from_block: u64,
        to_block: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustNullifierTransaction, sqlx::Error>> + Send;
}

#[allow(unused_variables)]
impl DustGenerationsStorage for NoopStorage {
    async fn get_dust_generations(
        &self,
        cardano_reward_addresses: &[CardanoRewardAddress],
        ledger_version: LedgerVersion,
    ) -> Result<Vec<DustGenerations>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_dust_generation_entries(
        &self,
        dust_address: &[u8],
        start_index: u64,
        end_index: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustGenerationEntry, sqlx::Error>> + Send {
        stream::empty()
    }

    async fn get_dust_generation_dtime_updates(
        &self,
        dust_address: &[u8],
        cutoff_block_id: u64,
        upper_block_id: u64,
        after_event_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustGenerationDtimeUpdateEntry, sqlx::Error>> + Send {
        stream::empty()
    }

    async fn get_dust_nullifier_transactions(
        &self,
        nullifier_prefixes: &[Vec<u8>],
        from_block: u64,
        to_block: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustNullifierTransaction, sqlx::Error>> + Send {
        stream::empty()
    }
}

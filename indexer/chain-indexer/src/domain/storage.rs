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

use indexer_common::domain::{ProtocolVersion, SerializedLedgerStateKey};
use std::num::NonZeroUsize;

use crate::domain::{
    Block, BlockRef, DParameter, DustRegistrationEvent, SystemParametersChange, TermsAndConditions,
    Transaction,
};

/// Storage abstraction.
#[trait_variant::make(Send)]
pub trait Storage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Save the given block with parameters and return the max regular transaction ID.
    async fn save_block(
        &mut self,
        block: &Block,
        transactions: &[Transaction],
        dust_registration_events: &[DustRegistrationEvent],
        ledger_state_key: &SerializedLedgerStateKey,
        system_parameters_change: Option<&SystemParametersChange>,
    ) -> Result<Option<u64>, sqlx::Error>;

    /// Get the block ref, ledger state key and protocol version of the highest stored block.
    async fn get_highest_block(
        &self,
    ) -> Result<Option<(BlockRef, ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error>;

    /// Get the timestamp (milliseconds) of the highest stored block, if any. Used on resume to seed
    /// the parent-block timestamp, so the first block processed after a restart bumps its first
    /// regular transaction's well-formed `tblock` off the true parent block time rather than off its
    /// own timestamp (see chain-indexer's `apply_transactions`).
    async fn get_highest_block_timestamp(&self) -> Result<Option<u64>, sqlx::Error>;

    /// Get the protocol versions and ledger state keys of the newest `limit` blocks, ordered
    /// from oldest to newest.
    async fn get_newest_ledger_state_keys(
        &self,
        limit: NonZeroUsize,
    ) -> Result<Vec<(ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error>;

    /// Get the number of stored transactions.
    async fn get_transaction_count(&self) -> Result<u64, sqlx::Error>;

    /// Get the number of stored contract actions: deploys, calls, updates.
    async fn get_contract_action_count(&self) -> Result<(u64, u64, u64), sqlx::Error>;

    /// Get the latest D-Parameter.
    async fn get_latest_d_parameter(&self) -> Result<Option<DParameter>, sqlx::Error>;

    /// Get the latest Terms and Conditions.
    async fn get_latest_terms_and_conditions(
        &self,
    ) -> Result<Option<TermsAndConditions>, sqlx::Error>;
}

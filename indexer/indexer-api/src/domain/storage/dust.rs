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
    dust::DustGenerationStatus,
    storage::{BlockStorage, NoopStorage},
};
use indexer_common::domain::{CardanoRewardAddress, LedgerVersion};

/// DUST storage abstraction. Currently only supports the dustGenerationStatus query.
#[trait_variant::make(Send)]
pub trait DustStorage: BlockStorage {
    /// Get DUST generation status for specific Cardano reward addresses.
    async fn get_dust_generation_status(
        &self,
        cardano_reward_addresses: &[CardanoRewardAddress],
        ledger_version: LedgerVersion,
    ) -> Result<Vec<DustGenerationStatus>, sqlx::Error>;
}

#[allow(unused_variables)]
impl DustStorage for NoopStorage {
    async fn get_dust_generation_status(
        &self,
        cardano_reward_addresses: &[CardanoRewardAddress],
        ledger_version: LedgerVersion,
    ) -> Result<Vec<DustGenerationStatus>, sqlx::Error> {
        Ok(vec![])
    }
}

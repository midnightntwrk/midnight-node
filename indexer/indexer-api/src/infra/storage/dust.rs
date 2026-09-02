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

use crate::{
    domain::{dust::DustGenerationStatus, storage::dust::DustStorage},
    infra::storage::Storage,
};
use fastrace::trace;
use indexer_common::{
    domain::{ByteVec, CardanoRewardAddress, LedgerVersion, TimestampMs, TimestampSecs, ledger},
    infra::sqlx::U128BeBytes,
};
use indoc::indoc;

impl DustStorage for Storage {
    #[trace]
    async fn get_dust_generation_status(
        &self,
        cardano_reward_addresses: &[CardanoRewardAddress],
        ledger_version: LedgerVersion,
    ) -> Result<Vec<DustGenerationStatus>, sqlx::Error> {
        // Get DUST parameters for the given protocol version.
        let dust_params = ledger::dust_parameters(ledger_version)
            .expect("DUST parameters should be available for supported protocol version");
        let generation_decay_rate = dust_params.generation_decay_rate as u128;
        let night_dust_ratio = dust_params.night_dust_ratio as u128;

        let mut statuses = Vec::with_capacity(cardano_reward_addresses.len());

        for reward_address in cardano_reward_addresses {
            // Query registration info.
            let registration_query = indoc! {"
                SELECT dust_address, valid, utxo_tx_hash, utxo_output_index
                FROM cnight_registrations
                WHERE cardano_stake_key = $1
                AND removed_at IS NULL
                ORDER BY registered_at DESC
                LIMIT 1
            "};

            let (dust_address, registered, utxo_tx_hash, utxo_output_index) =
                sqlx::query_as::<_, (ByteVec, bool, Option<Vec<u8>>, Option<i64>)>(
                    registration_query,
                )
                .bind(reward_address.as_ref())
                .fetch_optional(&*self.pool)
                .await?
                .unwrap_or_default();

            let mut generation_rate = 0u128;
            let mut max_capacity = 0u128;
            let mut current_capacity = 0u128;
            let mut night_balance = 0u128;

            // Query active generation info if registered.
            if registered {
                let generation_query = indoc! {"
                    SELECT value, ctime
                    FROM dust_generation_info
                    WHERE owner = $1
                    AND dtime IS NULL
                    ORDER BY ctime DESC
                    LIMIT 1
                "};

                let result = sqlx::query_as::<_, (U128BeBytes, i64)>(generation_query)
                    .bind(dust_address.as_ref())
                    .fetch_optional(&*self.pool)
                    .await?;

                if let Some((value, ctime_raw)) = result {
                    let value = u128::from(value);
                    night_balance = value;

                    // DUST generation rate = STAR * generation_decay_rate SPECK/second.
                    generation_rate = value.saturating_mul(generation_decay_rate);

                    // dust_generation_info.ctime is in seconds (ledger convention).
                    let ctime = TimestampSecs(ctime_raw as u64);

                    // Get current timestamp from latest block.
                    // blocks.timestamp is in milliseconds (Substrate Timestamp pallet).
                    let current_time_query = indoc! {"
                        SELECT timestamp
                        FROM blocks
                        ORDER BY height DESC
                        LIMIT 1
                    "};

                    let now = sqlx::query_as::<_, (i64,)>(current_time_query)
                        .fetch_optional(&*self.pool)
                        .await?
                        .map(|(t,)| TimestampMs(t as u64))
                        .unwrap_or(ctime.to_ms());

                    let elapsed_seconds = now.elapsed_seconds_since(ctime.to_ms());

                    // Maximum capacity (static cap) = STAR * night_dust_ratio.
                    max_capacity = value.saturating_mul(night_dust_ratio);

                    // Current capacity (time-dependent) = STAR * generation_decay_rate *
                    // elapsed_seconds. Capped at max_capacity.
                    let generated_capacity = value
                        .saturating_mul(generation_decay_rate)
                        .saturating_mul(elapsed_seconds as u128);
                    current_capacity = generated_capacity.min(max_capacity);
                }
            }

            statuses.push(DustGenerationStatus {
                cardano_reward_address: reward_address.to_owned(),
                dust_address: registered.then_some(dust_address),
                registered,
                night_balance,
                generation_rate,
                max_capacity,
                current_capacity,
                utxo_tx_hash,
                utxo_output_index: utxo_output_index.map(|i| i as u32),
            });
        }

        Ok(statuses)
    }
}

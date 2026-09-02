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
    domain::{
        dust::{
            DustGenerationDtimeUpdateEntry, DustGenerationEntry, DustGenerations,
            DustNullifierTransaction, DustRegistration,
        },
        storage::dust_generations::DustGenerationsStorage,
    },
    infra::storage::Storage,
};
use async_stream::try_stream;
use fastrace::trace;
use futures::Stream;
use indexer_common::{
    domain::{
        ByteVec, CardanoRewardAddress, LedgerEventAttributes, LedgerVersion, TimestampMs,
        TimestampSecs, TransactionHash, ledger,
    },
    infra::sqlx::U128BeBytes,
};
use indoc::indoc;
use sqlx::FromRow;
use std::num::NonZeroU32;

impl DustGenerationsStorage for Storage {
    #[trace]
    async fn get_dust_generations(
        &self,
        cardano_reward_addresses: &[CardanoRewardAddress],
        ledger_version: LedgerVersion,
    ) -> Result<Vec<DustGenerations>, sqlx::Error> {
        let dust_params = ledger::dust_parameters(ledger_version)
            .expect("DUST parameters should be available for supported protocol version");
        let generation_decay_rate = dust_params.generation_decay_rate as u128;
        let night_dust_ratio = dust_params.night_dust_ratio as u128;

        let current_time_query = indoc! {"
            SELECT timestamp
            FROM blocks
            ORDER BY height DESC
            LIMIT 1
        "};

        let now = sqlx::query_as::<_, (i64,)>(current_time_query)
            .fetch_optional(&*self.pool)
            .await?
            .map(|(t,)| TimestampMs(t as u64));

        let mut results = Vec::with_capacity(cardano_reward_addresses.len());

        for reward_address in cardano_reward_addresses {
            let registration_query = indoc! {"
                SELECT dust_address, valid, utxo_tx_hash, utxo_output_index
                FROM cnight_registrations
                WHERE cardano_stake_key = $1
                AND removed_at IS NULL
                ORDER BY registered_at DESC
            "};

            let registrations = sqlx::query_as::<_, (ByteVec, bool, Option<Vec<u8>>, Option<i64>)>(
                registration_query,
            )
            .bind(reward_address.as_ref())
            .fetch_all(&*self.pool)
            .await?;

            let mut registration_data = Vec::with_capacity(registrations.len());

            for (dust_address, valid, utxo_tx_hash, utxo_output_index) in registrations {
                let generations_query = indoc! {"
                    SELECT value, ctime
                    FROM dust_generation_info
                    WHERE owner = $1
                    AND dtime IS NULL
                "};

                let generations = sqlx::query_as::<_, (U128BeBytes, i64)>(generations_query)
                    .bind(dust_address.as_ref())
                    .fetch_all(&*self.pool)
                    .await?;

                let mut night_balance = 0u128;
                let mut generation_rate = 0u128;
                let mut max_capacity = 0u128;
                let mut current_capacity = 0u128;

                for (value, ctime_raw) in &generations {
                    let value = u128::from(*value);
                    let ctime = TimestampSecs(*ctime_raw as u64);

                    night_balance = night_balance.saturating_add(value);
                    generation_rate =
                        generation_rate.saturating_add(value.saturating_mul(generation_decay_rate));
                    let gen_max = value.saturating_mul(night_dust_ratio);
                    max_capacity = max_capacity.saturating_add(gen_max);

                    if let Some(now) = now {
                        let elapsed_seconds = now.elapsed_seconds_since(ctime.to_ms());
                        let gen_capacity = value
                            .saturating_mul(generation_decay_rate)
                            .saturating_mul(elapsed_seconds as u128)
                            .min(gen_max);
                        current_capacity = current_capacity.saturating_add(gen_capacity);
                    }
                }

                registration_data.push(DustRegistration {
                    dust_address,
                    valid,
                    night_balance,
                    generation_rate,
                    max_capacity,
                    current_capacity,
                    utxo_tx_hash,
                    utxo_output_index: utxo_output_index.map(|i| i as u32),
                });
            }

            results.push(DustGenerations {
                cardano_reward_address: reward_address.to_owned(),
                registrations: registration_data,
            });
        }

        Ok(results)
    }

    async fn get_dust_generation_entries(
        &self,
        dust_address: &[u8],
        mut start_index: u64,
        end_index: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustGenerationEntry, sqlx::Error>> + Send {
        let pool = self.pool.clone();
        let dust_address = dust_address.to_vec();

        try_stream! {
            loop {
                // `generation_index >= $2` implicitly filters out legacy rows
                // (inserted before the 002 migration) whose generation_index
                // is NULL. Those rows also lack backing_night / initial_value,
                // so skipping them keeps the stream well-typed without having
                // to plumb Option fields through the domain layer.
                let query = indoc! {"
                    SELECT
                        dust_generation_info.merkle_index AS commitment_mt_index,
                        dust_generation_info.generation_index AS generation_mt_index,
                        dust_generation_info.owner,
                        dust_generation_info.value,
                        dust_generation_info.initial_value,
                        dust_generation_info.backing_night,
                        dust_generation_info.ctime,
                        dust_generation_info.transaction_id,
                        transactions.hash AS transaction_hash
                    FROM dust_generation_info
                    INNER JOIN transactions ON transactions.id = dust_generation_info.transaction_id
                    WHERE dust_generation_info.owner = $1
                    AND dust_generation_info.generation_index >= $2
                    AND dust_generation_info.generation_index <= $3
                    ORDER BY dust_generation_info.generation_index
                    LIMIT $4
                "};

                let entries = sqlx::query_as::<_, DustGenerationEntry>(query)
                    .bind(&dust_address[..])
                    .bind(start_index as i64)
                    .bind(end_index as i64)
                    .bind(batch_size.get() as i64)
                    .fetch_all(&*pool)
                    .await?;

                let Some(last_index) = entries.last().map(|e| e.generation_mt_index) else {
                    break;
                };

                for entry in entries {
                    yield entry;
                }

                if last_index >= end_index {
                    break;
                }
                start_index = last_index + 1;
            }
        }
    }

    async fn get_dust_generation_dtime_updates(
        &self,
        dust_address: &[u8],
        cutoff_block_id: u64,
        upper_block_id: u64,
        mut after_event_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustGenerationDtimeUpdateEntry, sqlx::Error>> + Send {
        let pool = self.pool.clone();
        let dust_address = dust_address.to_vec();

        try_stream! {
            loop {
                // Filter by indexed `variant` (Postgres LEDGER_EVENT_VARIANT
                // enum, SQLite TEXT-with-CHECK). Join `dust_generation_info`
                // via `night_utxo_hash`, which is stored inside the
                // attributes as a hex string (ByteVec uses
                // #[serde(with = "const_hex")] which serialises with a `0x`
                // prefix). Strip the prefix only if present (anchored, not
                // global) so any malformed input fails loudly at decode
                // time rather than being silently rewritten. The indexed
                // BYTEA/BLOB equality predicate on dgi then fires. The full
                // attributes blob (JSONB on Postgres, TEXT on SQLite) is
                // also returned so the caller can deserialise
                // `LedgerEventAttributes::DustGenerationDtimeUpdate` to
                // recover the tree_insertion_path (and dtime) without
                // further SQL extraction.
                #[cfg(feature = "cloud")]
                let query = indoc! {"
                    SELECT
                        le.id AS ledger_event_id,
                        dgi.generation_index AS generation_mt_index,
                        dgi.owner,
                        dgi.night_utxo_hash,
                        le.attributes,
                        le.transaction_id,
                        t.hash AS transaction_hash
                    FROM ledger_events le
                    JOIN transactions t ON t.id = le.transaction_id
                    JOIN dust_generation_info dgi
                      ON dgi.night_utxo_hash = decode(
                           regexp_replace(
                               le.attributes -> 'DustGenerationDtimeUpdate'
                                             -> 'generation_info'
                                            ->> 'night_utxo_hash',
                               '^0x', ''),
                           'hex')
                    WHERE le.variant = 'DustGenerationDtimeUpdate'
                    AND t.block_id > $1
                    AND t.block_id <= $5
                    AND dgi.owner = $2
                    AND le.id > $3
                    ORDER BY le.id
                    LIMIT $4
                "};

                #[cfg(feature = "standalone")]
                let query = indoc! {"
                    WITH dtime_events AS (
                        SELECT
                            le.id AS ledger_event_id,
                            le.transaction_id,
                            le.attributes,
                            json_extract(le.attributes,
                                '$.DustGenerationDtimeUpdate.generation_info.night_utxo_hash')
                                AS hash_hex
                        FROM ledger_events le
                        WHERE le.variant = 'DustGenerationDtimeUpdate'
                        AND le.id > $3
                    )
                    SELECT
                        e.ledger_event_id,
                        dgi.generation_index AS generation_mt_index,
                        dgi.owner,
                        dgi.night_utxo_hash,
                        e.attributes,
                        e.transaction_id,
                        t.hash AS transaction_hash
                    FROM dtime_events e
                    JOIN transactions t ON t.id = e.transaction_id
                    JOIN dust_generation_info dgi
                      ON dgi.night_utxo_hash = unhex(iif(
                            substr(e.hash_hex, 1, 2) = '0x',
                            substr(e.hash_hex, 3),
                            e.hash_hex))
                    WHERE t.block_id > $1
                    AND t.block_id <= $5
                    AND dgi.owner = $2
                    ORDER BY e.ledger_event_id
                    LIMIT $4
                "};

                let rows = sqlx::query_as::<_, DtimeUpdateRow>(query)
                    .bind(cutoff_block_id as i64)
                    .bind(&dust_address[..])
                    .bind(after_event_id as i64)
                    .bind(batch_size.get() as i64)
                    .bind(upper_block_id as i64)
                    .fetch_all(&*pool)
                    .await?;

                let Some(last_event_id) = rows.last().map(|row| row.ledger_event_id) else {
                    break;
                };

                for row in rows {
                    let DtimeUpdateRow {
                        ledger_event_id,
                        generation_mt_index,
                        owner,
                        night_utxo_hash,
                        attributes,
                        transaction_id,
                        transaction_hash,
                    } = row;

                    let LedgerEventAttributes::DustGenerationDtimeUpdate {
                        generation_info,
                        tree_insertion_path,
                        ..
                    } = attributes
                    else {
                        // The `WHERE le.variant = 'DustGenerationDtimeUpdate'`
                        // filter above means every row matches this variant.
                        // A mismatch here would be DB corruption; skip rather
                        // than panic.
                        continue;
                    };

                    yield DustGenerationDtimeUpdateEntry {
                        ledger_event_id: ledger_event_id as u64,
                        generation_mt_index: generation_mt_index as u64,
                        owner,
                        night_utxo_hash,
                        new_dtime: generation_info.dtime,
                        transaction_id: transaction_id as u64,
                        transaction_hash,
                        tree_insertion_path,
                    };
                }

                after_event_id = last_event_id as u64;
            }
        }
    }

    async fn get_dust_nullifier_transactions(
        &self,
        nullifier_prefixes: &[Vec<u8>],
        from_block: u64,
        to_block: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<DustNullifierTransaction, sqlx::Error>> + Send {
        let pool = self.pool.clone();
        let nullifier_prefixes = nullifier_prefixes.to_vec();

        try_stream! {
            for prefix in &nullifier_prefixes {
                let mut next_prefix = prefix.clone();
                if let Some(last) = next_prefix.last_mut() {
                    if *last < 255 {
                        *last += 1;
                    } else {
                        next_prefix.push(0);
                    }
                }

                let mut cursor = 0i64;

                loop {
                    let query = indoc! {"
                        SELECT dn.id, dn.nullifier, dn.commitment, t.id, t.hash, b.height, b.hash
                        FROM dust_nullifiers dn
                        JOIN transactions t ON t.id = dn.transaction_id
                        JOIN blocks b ON b.id = dn.block_id
                        WHERE dn.nullifier >= $1 AND dn.nullifier < $2
                        AND b.height >= $3
                        AND b.height <= $4
                        AND dn.id > $5
                        ORDER BY dn.id
                        LIMIT $6
                    "};

                    let rows = sqlx::query_as::<_, (i64, ByteVec, ByteVec, i64, TransactionHash, i64, ByteVec)>(query)
                        .bind(&prefix[..])
                        .bind(&next_prefix[..])
                        .bind(from_block as i64)
                        .bind(to_block.min(i64::MAX as u64) as i64)
                        .bind(cursor)
                        .bind(batch_size.get() as i64)
                        .fetch_all(&*pool)
                        .await?;

                    match rows.last() {
                        Some(last) => cursor = last.0,
                        None => break,
                    }

                    for (_, nullifier_le_bytes, commitment_le_bytes, transaction_id, transaction_hash, block_height, block_hash) in rows {
                        yield DustNullifierTransaction {
                            nullifier_le_bytes,
                            commitment_le_bytes,
                            transaction_id: transaction_id as u64,
                            transaction_hash,
                            block_height: block_height as u32,
                            block_hash,
                        };
                    }
                }
            }
        }
    }
}

/// Row shape returned by the dtime-updates query. The `attributes` JSONB
/// blob is deserialised into the `LedgerEventAttributes` enum so the caller
/// can extract the tree_insertion_path (and dtime) without further SQL
/// extraction.
#[derive(FromRow)]
struct DtimeUpdateRow {
    #[sqlx(rename = "ledger_event_id")]
    ledger_event_id: i64,
    #[sqlx(rename = "generation_mt_index")]
    generation_mt_index: i64,
    owner: ByteVec,
    night_utxo_hash: ByteVec,
    #[sqlx(json)]
    attributes: LedgerEventAttributes,
    transaction_id: i64,
    transaction_hash: TransactionHash,
}

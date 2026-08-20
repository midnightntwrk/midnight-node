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
        bridge::{
            BridgeBalance, BridgeEvent, BridgePoolSummary, BridgeTreasuryAggregate, TreasuryReason,
        },
        storage::bridge::{BridgeEventFilter, BridgeStorage},
    },
    infra::storage::Storage,
};
use fastrace::trace;
use indexer_common::{
    domain::{
        UnshieldedAddress,
        bridge::{BridgeEventVariant, BridgeRecipient, McTxHash, MidnightTxHash},
    },
    infra::sqlx::U128BeBytes,
};
use indoc::indoc;
use sqlx::{QueryBuilder, Row};

#[cfg(feature = "cloud")]
type Db = sqlx::Postgres;
#[cfg(feature = "standalone")]
type Db = sqlx::Sqlite;

/// SQL fragment selecting all `bridge_events` columns plus the joined block height.
const SELECT_EVENT_FRAGMENT: &str = "SELECT \
    bpe.id, b.height, bpe.transaction_id, bpe.variant, \
    bpe.mc_tx_hash, bpe.amount, bpe.recipient, bpe.midnight_tx_hash, bpe.count \
    FROM protocol_bridge_events bpe \
    JOIN blocks b ON b.id = bpe.block_id ";

fn decode_u64_be(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let len = bytes.len().min(8);
    buf[8 - len..].copy_from_slice(&bytes[bytes.len() - len..]);
    u64::from_be_bytes(buf)
}

fn map_event_row(row: &<Db as sqlx::Database>::Row) -> Result<BridgeEvent, sqlx::Error> {
    let id: i64 = row.try_get(0)?;
    let height: i64 = row.try_get(1)?;
    let transaction_id: Option<i64> = row.try_get(2)?;
    let variant: BridgeEventVariant = row.try_get(3)?;
    let mc_tx_hash: Option<Vec<u8>> = row.try_get(4)?;
    let amount: Vec<u8> = row.try_get(5)?;
    let recipient: Option<Vec<u8>> = row.try_get(6)?;
    let midnight_tx_hash: Vec<u8> = row.try_get(7)?;
    let count: Option<i32> = row.try_get(8)?;

    let mc_tx_hash = mc_tx_hash
        .map(|b| McTxHash::try_from(b).map_err(|e| sqlx::Error::Decode(Box::new(e))))
        .transpose()?;
    let midnight_tx_hash =
        MidnightTxHash::try_from(midnight_tx_hash).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let recipient = recipient
        .map(|b| BridgeRecipient::new(b).map_err(|e| sqlx::Error::Decode(Box::new(e))))
        .transpose()?;

    Ok(BridgeEvent {
        id: id as u64,
        block_height: height as u64,
        transaction_id: transaction_id.map(|i| i as u64),
        variant,
        mc_tx_hash,
        amount: decode_u64_be(&amount),
        recipient,
        midnight_tx_hash,
        count: count.map(|c| c as u32),
    })
}

fn push_filter<'a>(builder: &mut QueryBuilder<'a, Db>, filter: &'a BridgeEventFilter) -> bool {
    let mut started = false;
    let push_clause = |b: &mut QueryBuilder<'a, Db>, started: &mut bool| {
        if !*started {
            b.push(" WHERE ");
            *started = true;
        } else {
            b.push(" AND ");
        }
    };

    if !filter.variants.is_empty() {
        push_clause(builder, &mut started);
        builder.push("bpe.variant IN (");
        let mut variants = builder.separated(", ");
        for variant in filter.variants.iter().copied() {
            variants.push_bind(variant);
        }
        builder.push(")");
    }
    if let Some(recipient) = &filter.recipient {
        push_clause(builder, &mut started);
        builder
            .push("bpe.recipient = ")
            .push_bind(recipient.as_ref().to_vec());
    }
    if let Some(from) = filter.block_height_from {
        push_clause(builder, &mut started);
        builder.push("b.height >= ").push_bind(from as i64);
    }
    if let Some(to) = filter.block_height_to {
        push_clause(builder, &mut started);
        builder.push("b.height <= ").push_bind(to as i64);
    }
    if let Some(id_from) = filter.id_from {
        push_clause(builder, &mut started);
        builder.push("bpe.id > ").push_bind(id_from as i64);
    }

    started
}

impl BridgeStorage for Storage {
    #[trace]
    async fn get_bridge_events(
        &self,
        filter: &BridgeEventFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error> {
        let mut builder: QueryBuilder<'_, Db> = QueryBuilder::new(SELECT_EVENT_FRAGMENT);
        push_filter(&mut builder, filter);
        builder
            .push(" ORDER BY bpe.id LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        let rows = builder.build().fetch_all(&*self.pool).await?;
        rows.iter().map(map_event_row).collect()
    }

    #[trace]
    async fn get_bridge_balance(
        &self,
        recipient: UnshieldedAddress,
    ) -> Result<BridgeBalance, sqlx::Error> {
        let deposited_q = indoc! {"
            SELECT amount
            FROM protocol_bridge_events
            WHERE variant = $1
              AND recipient = $2
        "};

        let deposited_rows: Vec<(Vec<u8>,)> = sqlx::query_as(deposited_q)
            .bind(BridgeEventVariant::UserTransfer)
            .bind(recipient.as_ref())
            .fetch_all(&*self.pool)
            .await?;
        let deposited: u128 = deposited_rows
            .iter()
            .map(|(b,)| decode_u64_be(b) as u128)
            .sum();

        let claimed_q = indoc! {"
            SELECT amount
            FROM bridge_claims
            WHERE recipient = $1
        "};

        let claimed_rows: Vec<(U128BeBytes,)> = sqlx::query_as(claimed_q)
            .bind(recipient.as_ref())
            .fetch_all(&*self.pool)
            .await?;
        let claimed: u128 = claimed_rows.iter().map(|(b,)| u128::from(*b)).sum();

        // `balance` is the authoritative remaining-claimable read from the ledger's
        // `bridge_receiving` map by the API layer; this event-derived value is only a
        // fallback for direct callers.
        let balance = deposited.saturating_sub(claimed);

        Ok(BridgeBalance {
            deposited,
            claimed,
            balance,
        })
    }

    #[trace]
    async fn get_bridge_reserve_inflows(
        &self,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error> {
        let filter = BridgeEventFilter {
            variants: vec![BridgeEventVariant::ReserveTransfer],
            block_height_from,
            block_height_to,
            ..Default::default()
        };
        self.get_bridge_events(&filter, offset, limit).await
    }

    #[trace]
    async fn get_bridge_treasury_inflows(
        &self,
        reason: Option<TreasuryReason>,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeEvent>, sqlx::Error> {
        let mut builder: QueryBuilder<'_, Db> = QueryBuilder::new(SELECT_EVENT_FRAGMENT);
        builder.push(" WHERE ");
        match reason {
            Some(r) => {
                builder.push("bpe.variant = ").push_bind(r.as_variant());
            }
            None => {
                builder.push("bpe.variant IN (");
                builder.push_bind(BridgeEventVariant::InvalidTransfer);
                builder.push(", ");
                builder.push_bind(BridgeEventVariant::UnapprovedTransfer);
                builder.push(", ");
                builder.push_bind(BridgeEventVariant::SubminimalFlushTransfer);
                builder.push(")");
            }
        }
        if let Some(from) = block_height_from {
            builder.push(" AND b.height >= ").push_bind(from as i64);
        }
        if let Some(to) = block_height_to {
            builder.push(" AND b.height <= ").push_bind(to as i64);
        }
        builder
            .push(" ORDER BY bpe.id LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        let rows = builder.build().fetch_all(&*self.pool).await?;
        rows.iter().map(map_event_row).collect()
    }

    #[trace]
    async fn get_bridge_pool_summary(
        &self,
        at_block_height: Option<u64>,
    ) -> Result<BridgePoolSummary, sqlx::Error> {
        let bound = at_block_height.map(|h| h as i64).unwrap_or(i64::MAX);

        // Pull every relevant row; in practice volumes are low. Aggregate in app layer to avoid
        // dialect-specific SUM/CASE differences between Postgres and SQLite.
        let rows: Vec<(BridgeEventVariant, Vec<u8>, Option<i32>)> = sqlx::query_as(indoc! {"
            SELECT bpe.variant, bpe.amount, bpe.count
            FROM protocol_bridge_events bpe
            JOIN blocks b ON b.id = bpe.block_id
            WHERE b.height <= $1
        "})
        .bind(bound)
        .fetch_all(&*self.pool)
        .await?;

        let mut reserve_total = 0u128;
        let mut invalid_total = 0u128;
        let mut invalid_count = 0u64;
        let mut unapproved_total = 0u128;
        let mut unapproved_count = 0u64;
        let mut flush_total = 0u128;
        let mut flush_count = 0u64;
        let mut subminimum_tx_count = 0u64;

        for (variant, amount_bytes, count) in &rows {
            let amount = decode_u64_be(amount_bytes) as u128;
            match variant {
                BridgeEventVariant::ReserveTransfer => {
                    reserve_total = reserve_total.saturating_add(amount);
                }
                BridgeEventVariant::InvalidTransfer => {
                    invalid_total = invalid_total.saturating_add(amount);
                    invalid_count += 1;
                }
                BridgeEventVariant::UnapprovedTransfer => {
                    unapproved_total = unapproved_total.saturating_add(amount);
                    unapproved_count += 1;
                }
                BridgeEventVariant::SubminimalFlushTransfer => {
                    flush_total = flush_total.saturating_add(amount);
                    flush_count += 1;
                    if let Some(c) = count {
                        subminimum_tx_count += *c as u64;
                    }
                }
                BridgeEventVariant::UserTransfer => {}
            }
        }

        let last_height_q = indoc! {"
            SELECT MAX(b.height)
            FROM protocol_bridge_events bpe
            JOIN blocks b ON b.id = bpe.block_id
            WHERE b.height <= $1
        "};
        let last_event_block_height: Option<i64> =
            sqlx::query_as::<_, (Option<i64>,)>(last_height_q)
                .bind(bound)
                .fetch_one(&*self.pool)
                .await?
                .0;

        Ok(BridgePoolSummary {
            reserve_total,
            treasury_by_reason: vec![
                BridgeTreasuryAggregate {
                    reason: BridgeEventVariant::InvalidTransfer,
                    total: invalid_total,
                    count: invalid_count,
                },
                BridgeTreasuryAggregate {
                    reason: BridgeEventVariant::UnapprovedTransfer,
                    total: unapproved_total,
                    count: unapproved_count,
                },
                BridgeTreasuryAggregate {
                    reason: BridgeEventVariant::SubminimalFlushTransfer,
                    total: flush_total,
                    count: flush_count,
                },
            ],
            subminimum_tx_count,
            last_event_block_height: last_event_block_height.map(|h| h as u64),
        })
    }
}

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
    domain::{self, Transaction, storage::SqlxTransaction},
    infra::storage,
};
use chacha20poly1305::ChaCha20Poly1305;
use derive_more::Debug;
use fastrace::trace;
use futures::TryStreamExt;
use indexer_common::domain::{ByteVec, DecryptViewingKeyError, ViewingKey};
use indoc::indoc;
use sqlx::{
    QueryBuilder, Row,
    prelude::FromRow,
    types::{Uuid, time::OffsetDateTime},
};
use std::{num::NonZeroUsize, time::Duration};

/// Unified storage implementation for PostgreSQL (cloud) and SQLite (standalone). Uses Cargo
/// features to select the appropriate database backend at build time.
#[derive(Debug, Clone)]
pub struct Storage {
    #[debug(skip)]
    cipher: ChaCha20Poly1305,

    #[cfg(feature = "cloud")]
    pool: indexer_common::infra::pool::postgres::PostgresPool,

    #[cfg(feature = "standalone")]
    pool: indexer_common::infra::pool::sqlite::SqlitePool,
}

impl Storage {
    #[cfg(feature = "cloud")]
    pub fn new(
        cipher: ChaCha20Poly1305,
        pool: indexer_common::infra::pool::postgres::PostgresPool,
    ) -> Self {
        Self { cipher, pool }
    }

    #[cfg(feature = "standalone")]
    pub fn new(
        cipher: ChaCha20Poly1305,
        pool: indexer_common::infra::pool::sqlite::SqlitePool,
    ) -> Self {
        Self { cipher, pool }
    }
}

impl domain::storage::Storage for Storage {
    #[cfg(feature = "cloud")]
    type Database = sqlx::Postgres;

    #[cfg(feature = "standalone")]
    type Database = sqlx::Sqlite;

    #[cfg(feature = "cloud")]
    #[trace(properties = { "wallet_id": "{wallet_id}" })]
    async fn acquire_lock(
        &mut self,
        wallet_id: Uuid,
    ) -> Result<Option<SqlxTransaction<Self::Database>>, sqlx::Error> {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let mut tx = self.pool.begin().await?;

        // Convert UUID to two i32 values by hashing to u64 and splitting into two.
        let mut hasher = DefaultHasher::new();
        wallet_id.hash(&mut hasher);
        let hash = hasher.finish();
        let high = (hash >> 32) as i32;
        let low = hash as i32;

        let lock_acquired = sqlx::query("SELECT pg_try_advisory_xact_lock($1, $2)")
            .bind(high)
            .bind(low)
            .fetch_one(&mut *tx)
            .await
            .and_then(|row| row.try_get::<bool, _>(0))?;

        Ok(lock_acquired.then_some(tx))
    }

    #[cfg(feature = "standalone")]
    async fn acquire_lock(
        &mut self,
        _wallet_id: Uuid,
    ) -> Result<Option<SqlxTransaction<Self::Database>>, sqlx::Error> {
        // SQLite doesn't support advisory locks like PostgreSQL. But in standalone mode (single
        // instance) we need not exclude other replicas, i.e. "locking" is always successful.
        let tx = self.pool.begin().await?;
        Ok(Some(tx))
    }

    #[trace(properties = { "from": "{from}", "limit": "{limit}" })]
    async fn get_transactions(
        &self,
        from: u64,
        limit: NonZeroUsize,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                id,
                protocol_version,
                raw
            FROM transactions
            WHERE id >= $1
            AND variant = 'Regular'
            ORDER BY id
            LIMIT $2
        "};

        sqlx::query_as(query)
            .bind(from as i64)
            .bind(limit.get() as i32)
            .fetch_all(&mut **tx)
            .await
    }

    #[trace(properties = { "from": "{from}", "to": "{to}", "limit": "{limit}" })]
    async fn get_transactions_in_range(
        &self,
        from: u64,
        to: u64,
        limit: NonZeroUsize,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                id,
                protocol_version,
                raw
            FROM transactions
            WHERE id >= $1
            AND id < $2
            AND variant = 'Regular'
            ORDER BY id DESC
            LIMIT $3
        "};

        sqlx::query_as(query)
            .bind(from as i64)
            .bind(to as i64)
            .bind(limit.get() as i32)
            .fetch_all(&mut **tx)
            .await
    }

    #[trace]
    async fn save_relevant_transactions(
        &self,
        viewing_key: &ViewingKey,
        transactions: &[Transaction],
        last_indexed_transaction_id: u64,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<(), sqlx::Error> {
        let id = Uuid::now_v7();
        let viewing_key_hash = viewing_key.hash();
        let viewing_key = viewing_key
            .encrypt(id, &self.cipher)
            .map_err(|error| sqlx::Error::Encode(error.into()))?;

        let query = indoc! {"
            INSERT INTO wallets (
                id,
                viewing_key_hash,
                viewing_key,
                wanted_start_index,
                first_indexed_transaction_id,
                last_indexed_transaction_id,
                last_active
            )
            VALUES ($1, $2, $3, 0, 0, $4, $5)
            ON CONFLICT (viewing_key_hash)
            DO UPDATE SET last_indexed_transaction_id = $4
            RETURNING id
        "};

        let wallet_id = sqlx::query(query)
            .bind(id)
            .bind(viewing_key_hash.as_ref())
            .bind(viewing_key)
            .bind(last_indexed_transaction_id as i64)
            .bind(OffsetDateTime::now_utc())
            .fetch_one(&mut **tx)
            .await?
            .try_get::<Uuid, _>("id")?;

        if !transactions.is_empty() {
            let query = indoc! {"
                INSERT INTO relevant_transactions (
                    wallet_id,
                    transaction_id
                )
            "};

            QueryBuilder::new(query)
                .push_values(transactions, |mut q, transaction| {
                    q.push_bind(wallet_id).push_bind(transaction.id as i64);
                })
                .build()
                .execute(&mut **tx)
                .await?;
        }

        Ok(())
    }

    #[trace]
    async fn save_backward_relevant_transactions(
        &self,
        wallet_id: Uuid,
        transactions: &[Transaction],
        first_indexed_transaction_id: u64,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<(), sqlx::Error> {
        let query = indoc! {"
            UPDATE wallets
            SET first_indexed_transaction_id = $1
            WHERE id = $2
        "};

        sqlx::query(query)
            .bind(first_indexed_transaction_id as i64)
            .bind(wallet_id)
            .execute(&mut **tx)
            .await?;

        if !transactions.is_empty() {
            let query = indoc! {"
                INSERT INTO relevant_transactions (
                    wallet_id,
                    transaction_id
                )
            "};

            QueryBuilder::new(query)
                .push_values(transactions, |mut q, transaction| {
                    q.push_bind(wallet_id).push_bind(transaction.id as i64);
                })
                .build()
                .execute(&mut **tx)
                .await?;
        }

        Ok(())
    }

    #[trace]
    async fn active_wallet_ids(&self, ttl: Duration) -> Result<Vec<Uuid>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                id,
                last_active
            FROM wallets
            WHERE session_id IS NOT NULL
        "};

        let wallets = sqlx::query_as::<_, (Uuid, OffsetDateTime)>(query)
            .fetch(&*self.pool)
            .try_collect::<Vec<_>>()
            .await?;

        let min_last_active = OffsetDateTime::now_utc() - ttl;

        let outdated_ids = wallets
            .iter()
            .filter_map(|&(id, last_active)| (last_active < min_last_active).then_some(id))
            .collect::<Vec<_>>();

        if !outdated_ids.is_empty() {
            #[cfg(feature = "cloud")]
            {
                use indexer_common::infra::sqlx::postgres::ignore_deadlock_detected;

                let query = indoc! {"
                    UPDATE wallets
                    SET session_id = NULL
                    WHERE id = ANY($1)
                    AND last_active < $2
                "};

                // This could cause a "deadlock_detected" error when the indexer-api updates a
                // wallet session_id at the same time. These errors can be ignored, because this
                // operation will be executed "very soon" again.
                sqlx::query(query)
                    .bind(outdated_ids)
                    .bind(min_last_active)
                    .execute(&*self.pool)
                    .await
                    .map(|_| ())
                    .or_else(|error| ignore_deadlock_detected(error, || ()))?;
            }
        }

        #[cfg(feature = "standalone")]
        {
            for id in outdated_ids {
                let query = indoc! {"
                    UPDATE wallets
                    SET session_id = NULL
                    WHERE id = $1
                    AND last_active < $2
                "};

                // Housekeeping: if the shared writer is starved past busy_timeout, skip this
                // wallet instead of failing the poll (which would take down the whole
                // standalone process); the next poll retries.
                let result = sqlx::query(query)
                    .bind(id)
                    .bind(min_last_active)
                    .execute(&*self.pool)
                    .await;
                match result {
                    Err(error) if indexer_common::infra::pool::sqlite::is_busy_error(&error) => {
                        log::warn!("skipping wallet session cleanup, sqlite writer is busy");
                    }
                    other => {
                        other?;
                    }
                }
            }
        }

        // Return active wallet IDs.
        let ids = wallets
            .into_iter()
            .filter_map(|(id, last_active)| (last_active >= min_last_active).then_some(id))
            .collect::<Vec<_>>();
        Ok(ids)
    }

    #[trace(properties = { "id": "{id}" })]
    async fn get_wallet_by_id(
        &self,
        id: Uuid,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<domain::Wallet, sqlx::Error> {
        let query = indoc! {"
            SELECT
                id,
                viewing_key,
                wanted_start_index,
                first_indexed_transaction_id,
                last_indexed_transaction_id
            FROM wallets
            WHERE id = $1
        "};

        let wallet = sqlx::query_as::<_, storage::Wallet>(query)
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;

        domain::Wallet::try_from((wallet, &self.cipher))
            .map_err(|error| sqlx::Error::Decode(error.into()))
    }
}

/// Persistent wallet data.
#[derive(Debug, Clone, FromRow)]
pub struct Wallet {
    pub id: Uuid,

    pub viewing_key: ByteVec,

    #[sqlx(try_from = "i64")]
    pub wanted_start_index: u64,

    #[sqlx(try_from = "i64")]
    pub first_indexed_transaction_id: u64,

    #[sqlx(try_from = "i64")]
    pub last_indexed_transaction_id: u64,
}

#[cfg(all(test, feature = "standalone"))]
mod tests {
    use crate::{domain::storage::Storage as _, infra::storage::Storage};
    use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit};
    use indexer_common::infra::{
        migrations,
        pool::sqlite::{Config, SqlitePool},
    };
    use indoc::indoc;
    use sqlx::types::{Uuid, time::OffsetDateTime};
    use std::{error::Error as StdError, num::NonZeroUsize};

    // Seed a single block so that transactions can satisfy their FK.
    async fn seed_block(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        let query = indoc! {"
            INSERT INTO blocks (
                id, hash, height, protocol_version, parent_hash, author,
                timestamp, zswap_merkle_tree_root, ledger_parameters, ledger_state_key
            )
            VALUES (1, X'00', 0, 1000000, X'00', NULL, 0, X'00', X'00', X'00')
        "};
        sqlx::query(query).execute(&**pool).await?;
        Ok(1)
    }

    async fn seed_transaction(
        pool: &SqlitePool,
        id: i64,
        block_id: i64,
        variant: &str,
    ) -> Result<(), sqlx::Error> {
        let query = indoc! {"
            INSERT INTO transactions (id, block_id, variant, hash, protocol_version, raw)
            VALUES ($1, $2, $3, X'00', 1000000, X'00')
        "};
        sqlx::query(query)
            .bind(id)
            .bind(block_id)
            .bind(variant)
            .execute(&**pool)
            .await?;
        Ok(())
    }

    async fn seed_wallet(
        pool: &SqlitePool,
        id: Uuid,
        first_indexed: i64,
        wanted_start: i64,
    ) -> Result<(), sqlx::Error> {
        let query = indoc! {"
            INSERT INTO wallets (
                id, viewing_key_hash, viewing_key,
                wanted_start_index, first_indexed_transaction_id, last_indexed_transaction_id,
                last_active, session_id
            )
            VALUES ($1, X'00', X'00', $2, $3, $3, $4, NULL)
        "};
        sqlx::query(query)
            .bind(id)
            .bind(wanted_start)
            .bind(first_indexed)
            .bind(OffsetDateTime::now_utc())
            .execute(&**pool)
            .await?;
        Ok(())
    }

    async fn new_storage() -> Result<(Storage, SqlitePool), Box<dyn StdError>> {
        let pool = SqlitePool::new(Config::default()).await?;
        migrations::sqlite::run(&pool).await?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&[0u8; 32]));
        Ok((Storage::new(cipher, pool.clone()), pool))
    }

    /// Half-open `[from, to)` range, DESC order, filters `variant = 'Regular'`, honours limit.
    #[tokio::test]
    async fn get_transactions_in_range_semantics() -> Result<(), Box<dyn StdError>> {
        let (storage, pool) = new_storage().await?;
        let block_id = seed_block(&pool).await?;

        // Transactions 1..=10: even ids are Regular, odd ids are System (interleaved).
        for id in 1..=10 {
            let variant = if id % 2 == 0 { "Regular" } else { "System" };
            seed_transaction(&pool, id, block_id, variant).await?;
        }

        let limit = NonZeroUsize::new(100).unwrap();
        let mut tx = pool.begin().await?;

        // Range [3, 9) — Regular ids in that range: {4, 6, 8}, DESC.
        let result = storage
            .get_transactions_in_range(3, 9, limit, &mut tx)
            .await?;
        let ids = result.iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![8, 6, 4],
            "half-open range + Regular filter + DESC"
        );

        // `from` is inclusive.
        let result = storage
            .get_transactions_in_range(4, 9, limit, &mut tx)
            .await?;
        assert_eq!(result.first().map(|t| t.id), Some(8));
        assert!(result.iter().any(|t| t.id == 4), "`from` must be inclusive");

        // `to` is exclusive.
        let result = storage
            .get_transactions_in_range(3, 8, limit, &mut tx)
            .await?;
        assert!(!result.iter().any(|t| t.id == 8), "`to` must be exclusive");

        // Empty range collapses to empty result.
        let result = storage
            .get_transactions_in_range(5, 5, limit, &mut tx)
            .await?;
        assert!(result.is_empty());

        // Limit is respected — request 2 from [0, 11): Regular = {2,4,6,8,10}, DESC top 2 = [10,
        // 8].
        let two = NonZeroUsize::new(2).unwrap();
        let result = storage
            .get_transactions_in_range(0, 11, two, &mut tx)
            .await?;
        assert_eq!(result.iter().map(|t| t.id).collect::<Vec<_>>(), vec![10, 8]);

        Ok(())
    }

    /// With an empty `transactions` slice the cursor still advances and no relevant rows are
    /// inserted.
    #[tokio::test]
    async fn save_backward_empty_batch_advances_cursor_only() -> Result<(), Box<dyn StdError>> {
        let (storage, pool) = new_storage().await?;
        let wallet_id = Uuid::now_v7();
        seed_wallet(&pool, wallet_id, 1000, 0).await?;

        let mut tx = pool.begin().await?;
        storage
            .save_backward_relevant_transactions(wallet_id, &[], 0, &mut tx)
            .await?;
        tx.commit().await?;

        let (first_indexed,): (i64,) =
            sqlx::query_as("SELECT first_indexed_transaction_id FROM wallets WHERE id = $1")
                .bind(wallet_id)
                .fetch_one(&*pool)
                .await?;
        assert_eq!(first_indexed, 0, "cursor collapses to wanted_start");

        let (relevant_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM relevant_transactions WHERE wallet_id = $1")
                .bind(wallet_id)
                .fetch_one(&*pool)
                .await?;
        assert_eq!(
            relevant_count, 0,
            "no relevant rows inserted for empty batch"
        );

        Ok(())
    }

    /// With a non-empty batch the cursor advances and relevant rows are inserted atomically.
    #[tokio::test]
    async fn save_backward_with_batch_inserts_relevant_rows() -> Result<(), Box<dyn StdError>> {
        let (storage, pool) = new_storage().await?;
        let block_id = seed_block(&pool).await?;
        for id in [50i64, 75, 90] {
            seed_transaction(&pool, id, block_id, "Regular").await?;
        }

        let wallet_id = Uuid::now_v7();
        seed_wallet(&pool, wallet_id, 1000, 0).await?;

        // Simulate a DESC batch covering [50, 100): minimum id = 50, new cursor = 50.
        let mut tx = pool.begin().await?;
        let batch = storage
            .get_transactions_in_range(50, 100, NonZeroUsize::new(10).unwrap(), &mut tx)
            .await?;
        assert_eq!(
            batch.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![90, 75, 50]
        );

        let new_cursor = batch.last().map(|t| t.id).unwrap();
        storage
            .save_backward_relevant_transactions(wallet_id, &batch, new_cursor, &mut tx)
            .await?;
        tx.commit().await?;

        let (first_indexed,): (i64,) =
            sqlx::query_as("SELECT first_indexed_transaction_id FROM wallets WHERE id = $1")
                .bind(wallet_id)
                .fetch_one(&*pool)
                .await?;
        assert_eq!(first_indexed, 50);

        let mut ids = sqlx::query_as::<_, (i64,)>(
            "SELECT transaction_id FROM relevant_transactions WHERE wallet_id = $1",
        )
        .bind(wallet_id)
        .fetch_all(&*pool)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect::<Vec<_>>();
        ids.sort();
        assert_eq!(ids, vec![50, 75, 90]);

        Ok(())
    }
}

impl TryFrom<(Wallet, &ChaCha20Poly1305)> for domain::Wallet {
    type Error = DecryptViewingKeyError;

    fn try_from((wallet, cipher): (Wallet, &ChaCha20Poly1305)) -> Result<Self, Self::Error> {
        let Wallet {
            id,
            viewing_key,
            wanted_start_index,
            first_indexed_transaction_id,
            last_indexed_transaction_id,
        } = wallet;

        let viewing_key = ViewingKey::decrypt(&viewing_key, id, cipher)?;

        Ok(domain::Wallet {
            viewing_key,
            wanted_start_index,
            first_indexed_transaction_id,
            last_indexed_transaction_id,
        })
    }
}

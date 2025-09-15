// This file is part of midnight-indexer.
// Copyright (C) 2025 Midnight Foundation
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
        // instance) we need not exclude other, i.e. "locking" is always successful.
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

    #[trace]
    async fn save_relevant_transactions(
        &self,
        viewing_key: &ViewingKey,
        transactions: &[Transaction],
        last_indexed_transaction_id: u64,
        tx: &mut SqlxTransaction<Self::Database>,
    ) -> Result<(), sqlx::Error> {
        let id = Uuid::now_v7();
        let session_id = viewing_key.to_session_id();
        let viewing_key = viewing_key
            .encrypt(id, &self.cipher)
            .map_err(|error| sqlx::Error::Encode(error.into()))?;

        let query = indoc! {"
            INSERT INTO wallets (
                id,
                session_id,
                viewing_key,
                last_indexed_transaction_id,
                last_active
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (session_id)
            DO UPDATE SET last_indexed_transaction_id = $4
            RETURNING id
        "};

        let wallet_id = sqlx::query(query)
            .bind(id)
            .bind(session_id.as_ref())
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
    async fn active_wallets(&self, ttl: Duration) -> Result<Vec<Uuid>, sqlx::Error> {
        // Query wallets.
        let query = indoc! {"
            SELECT
                id,
                last_active
            FROM wallets
            WHERE active = TRUE
        "};

        let wallets = sqlx::query_as::<_, (Uuid, OffsetDateTime)>(query)
            .fetch(&*self.pool)
            .try_collect::<Vec<_>>()
            .await?;

        // Mark inactive wallets.
        let now = OffsetDateTime::now_utc();
        let outdated_ids = wallets
            .iter()
            .filter_map(|&(id, last_active)| (now - last_active > ttl).then_some(id))
            .collect::<Vec<_>>();
        if !outdated_ids.is_empty() {
            #[cfg(feature = "cloud")]
            {
                use indexer_common::infra::sqlx::postgres::ignore_deadlock_detected;

                let query = indoc! {"
                    UPDATE wallets
                    SET active = FALSE
                    WHERE id = ANY($1)
                "};

                // This could cause a "deadlock_detected" error when the indexer-api sets a wallet
                // active at the same time. These errors can be ignored, because this operation will
                // be executed "very soon" again.
                sqlx::query(query)
                    .bind(outdated_ids)
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
                    SET active = FALSE
                    WHERE id = ?
                "};

                sqlx::query(query).bind(id).execute(&*self.pool).await?;
            }
        }

        // Return active wallet IDs.
        let ids = wallets
            .into_iter()
            .filter_map(|(id, last_active)| (now - last_active <= ttl).then_some(id))
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
    pub last_indexed_transaction_id: u64,
}

impl TryFrom<(Wallet, &ChaCha20Poly1305)> for domain::Wallet {
    type Error = DecryptViewingKeyError;

    fn try_from((wallet, cipher): (Wallet, &ChaCha20Poly1305)) -> Result<Self, Self::Error> {
        let Wallet {
            id,
            viewing_key,
            last_indexed_transaction_id,
        } = wallet;

        let viewing_key = ViewingKey::decrypt(&viewing_key, id, cipher)?;

        Ok(domain::Wallet {
            viewing_key,
            last_indexed_transaction_id,
        })
    }
}

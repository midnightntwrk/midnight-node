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
    domain::{
        RegularTransaction, SystemTransaction, Transaction, bridge::BridgeClaim,
        storage::transaction::TransactionStorage,
    },
    infra::storage::Storage,
};
use async_stream::try_stream;
use fastrace::trace;
use futures::{Stream, StreamExt, TryStreamExt};
use indexer_common::{
    domain::{
        SerializedTransactionIdentifier, TransactionHash, TransactionVariant, UnshieldedAddress,
    },
    infra::sqlx::U128BeBytes,
    stream::flatten_chunks,
};
use indoc::indoc;
#[cfg(feature = "standalone")]
use sqlx::Sqlite;
use sqlx::{FromRow, QueryBuilder, Row, types::Uuid};
use std::{collections::HashMap, num::NonZeroU32};

#[cfg(feature = "cloud")]
type Db = sqlx::Postgres;
#[cfg(feature = "standalone")]
type Db = sqlx::Sqlite;

/// Build a `BridgeClaim` from a `bridge_claims` row selecting `recipient` and `amount`.
fn make_bridge_claim(row: &<Db as sqlx::Database>::Row) -> Result<BridgeClaim, sqlx::Error> {
    let recipient = row.try_get::<Vec<u8>, _>("recipient")?;
    let amount = row.try_get::<U128BeBytes, _>("amount")?;
    Ok(BridgeClaim {
        recipient: UnshieldedAddress::try_from(recipient)
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
        amount: amount.into(),
    })
}

impl Storage {
    /// Attach bridge-claim payloads to any regular transactions that are CardanoBridge claims, by
    /// looking them up in `bridge_claims` in a single batched query. Non-claim transactions are
    /// left untouched. Used by the `Vec`-returning read paths.
    async fn attach_bridge_claims<'a>(
        &self,
        transactions: impl IntoIterator<Item = &'a mut Transaction>,
    ) -> Result<(), sqlx::Error> {
        let regulars = transactions
            .into_iter()
            .filter_map(|transaction| match transaction {
                Transaction::Regular(regular) => Some(regular),
                _ => None,
            })
            .collect::<Vec<_>>();
        if regulars.is_empty() {
            return Ok(());
        }

        let mut builder = QueryBuilder::<Db>::new(
            "SELECT transaction_id, recipient, amount FROM bridge_claims WHERE transaction_id IN (",
        );
        let mut separated = builder.separated(", ");
        for regular in regulars.iter() {
            separated.push_bind(regular.id as i64);
        }
        builder.push(")");

        let claims = builder
            .build()
            .fetch_all(&*self.pool)
            .await?
            .iter()
            .map(|row| {
                let transaction_id = row.try_get::<i64, _>("transaction_id")? as u64;
                Ok((transaction_id, make_bridge_claim(row)?))
            })
            .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;

        for regular in regulars {
            let id = regular.id;
            regular.bridge_claim = claims.get(&id).cloned().map(Box::new);
        }

        Ok(())
    }
}

impl TransactionStorage for Storage {
    #[trace(properties = { "ids": "{ids:?}" })]
    async fn get_transactions_by_ids(&self, ids: &[u64]) -> Result<Vec<Transaction>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.id = ANY($1)

            UNION ALL

            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                NULL AS transaction_result,
                NULL AS zswap_merkle_tree_root,
                NULL AS zswap_start_index,
                NULL AS zswap_end_index,
                NULL AS dust_commitment_start_index,
                NULL AS dust_commitment_end_index,
                NULL AS dust_generation_start_index,
                NULL AS dust_generation_end_index,
                NULL AS paid_fees,
                NULL AS estimated_fees,
                NULL AS identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            WHERE transactions.id = ANY($1)
            AND transactions.variant = 'System'
        "};

        #[cfg(feature = "cloud")]
        let ids = ids.iter().map(|id| *id as i64).collect::<Vec<_>>();

        #[cfg(feature = "cloud")]
        let mut transactions = sqlx::query(query)
            .bind(ids)
            .fetch(&*self.pool)
            .map_ok(make_transaction)
            .map(|result| result.flatten())
            .try_collect::<Vec<_>>()
            .await?;

        #[cfg(feature = "standalone")]
        let mut transactions = {
            let mut qb = QueryBuilder::<Sqlite>::new("WITH ids(id) AS (VALUES (");
            let mut sep = qb.separated("), (");
            for id in ids {
                sep.push_bind(*id as i64);
            }
            qb.push(indoc! {"
                ))
                SELECT
                    transactions.id AS id,
                    transactions.variant,
                    transactions.hash,
                    transactions.protocol_version,
                    transactions.raw,
                    blocks.hash AS block_hash,
                    regular_transactions.transaction_result,
                    regular_transactions.zswap_merkle_tree_root,
                    regular_transactions.zswap_start_index,
                    regular_transactions.zswap_end_index,
                    regular_transactions.dust_commitment_start_index,
                    regular_transactions.dust_commitment_end_index,
                    regular_transactions.dust_generation_start_index,
                    regular_transactions.dust_generation_end_index,
                    regular_transactions.paid_fees,
                    regular_transactions.estimated_fees
                FROM transactions
                INNER JOIN blocks ON blocks.id = transactions.block_id
                INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
                WHERE transactions.id IN (SELECT id FROM ids)
                UNION ALL
                SELECT
                    transactions.id AS id,
                    transactions.variant,
                    transactions.hash,
                    transactions.protocol_version,
                    transactions.raw,
                    blocks.hash AS block_hash,
                    NULL AS transaction_result,
                    NULL AS zswap_merkle_tree_root,
                    NULL AS zswap_start_index,
                    NULL AS zswap_end_index,
                    NULL AS dust_commitment_start_index,
                    NULL AS dust_commitment_end_index,
                    NULL AS dust_generation_start_index,
                    NULL AS dust_generation_end_index,
                    NULL AS paid_fees,
                    NULL AS estimated_fees
                FROM transactions
                INNER JOIN blocks ON blocks.id = transactions.block_id
                WHERE transactions.variant = 'System'
                AND transactions.id IN (SELECT id FROM ids)
            "});

            qb.build()
                .fetch(&*self.pool)
                .map_ok(make_transaction)
                .map(|result| result.flatten())
                .try_collect::<Vec<_>>()
                .await?
        };

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            if let Transaction::Regular(transaction) = transaction {
                transaction.identifiers =
                    get_identifiers_for_transaction(transaction.id, &self.pool).await?;
            }
        }

        self.attach_bridge_claims(transactions.iter_mut()).await?;

        Ok(transactions)
    }

    async fn get_transactions_by_block_ids(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, Transaction)>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let mut transactions = self.fetch_transactions_by_block_ids(ids).await?;
        self.attach_bridge_claims(transactions.iter_mut().map(|(_, transaction)| transaction))
            .await?;
        Ok(transactions)
    }

    #[trace(properties = { "hash": "{hash}" })]
    async fn get_transactions_by_hash(
        &self,
        hash: TransactionHash,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.hash = $1

            UNION ALL

            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                NULL AS transaction_result,
                NULL AS zswap_merkle_tree_root,
                NULL AS zswap_start_index,
                NULL AS zswap_end_index,
                NULL AS dust_commitment_start_index,
                NULL AS dust_commitment_end_index,
                NULL AS dust_generation_start_index,
                NULL AS dust_generation_end_index,
                NULL AS paid_fees,
                NULL AS estimated_fees,
                NULL AS identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            WHERE transactions.hash = $1
            AND transactions.variant = 'System'

            ORDER BY id DESC
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            SELECT
                transactions.id as id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.hash = $1

            UNION ALL

            SELECT
                transactions.id as id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                NULL AS transaction_result,
                NULL AS zswap_merkle_tree_root,
                NULL AS zswap_start_index,
                NULL AS zswap_end_index,
                NULL AS dust_commitment_start_index,
                NULL AS dust_commitment_end_index,
                NULL AS dust_generation_start_index,
                NULL AS dust_generation_end_index,
                NULL AS paid_fees,
                NULL AS estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            WHERE transactions.hash = $1
            AND transactions.variant = 'System'

            ORDER BY id
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query(query)
            .bind(hash.as_ref())
            .fetch(&*self.pool)
            .map_ok(make_transaction)
            .map(|result| result.flatten())
            .try_collect::<Vec<_>>()
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            if let Transaction::Regular(transaction) = transaction {
                transaction.identifiers =
                    get_identifiers_for_transaction(transaction.id, &self.pool).await?;
            }
        }

        self.attach_bridge_claims(transactions.iter_mut()).await?;

        Ok(transactions)
    }

    #[trace(properties = { "identifier": "{identifier}" })]
    async fn get_transactions_by_identifier(
        &self,
        identifier: &SerializedTransactionIdentifier,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE $1 = ANY(regular_transactions.identifiers)
            ORDER BY transactions.id
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE EXISTS (
                SELECT 1
                FROM transaction_identifiers
                WHERE transaction_identifiers.transaction_id = transactions.id
                AND transaction_identifiers.identifier = $1
            )
            ORDER BY transactions.id
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, RegularTransaction>(query)
            .bind(identifier)
            .fetch(&*self.pool)
            .map_ok(Transaction::Regular)
            .try_collect::<Vec<_>>()
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            if let Transaction::Regular(transaction) = transaction {
                transaction.identifiers =
                    get_identifiers_for_transaction(transaction.id, &self.pool).await?;
            }
        }

        self.attach_bridge_claims(transactions.iter_mut()).await?;

        Ok(transactions)
    }

    fn get_relevant_transactions(
        &self,
        wallet_id: Uuid,
        mut index: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<RegularTransaction, sqlx::Error>> + Send {
        let chunks = try_stream! {
            loop {
                let transactions = self
                    .get_relevant_transactions(wallet_id, index, batch_size)
                    .await?;

                match transactions.last() {
                    Some(transaction) => index = transaction.zswap_end_index,
                    None => break,
                }

                yield transactions;
            }
        };

        flatten_chunks(chunks)
    }

    fn get_transactions_by_unshielded_address(
        &self,
        address: UnshieldedAddress,
        mut transaction_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<Transaction, sqlx::Error>> + Send {
        let chunks = try_stream! {
            loop {
                let transactions = self
                    .get_transactions_by_unshielded_address(address, transaction_id, batch_size)
                    .await?;

                match transactions.last() {
                    Some(transaction) => transaction_id = transaction.id() + 1,
                    None => break,
                };

                yield transactions;
            }
        };

        flatten_chunks(chunks)
    }

    #[trace(properties = { "address": "{address}" })]
    async fn get_highest_transaction_id_for_unshielded_address(
        &self,
        address: UnshieldedAddress,
    ) -> Result<Option<u64>, sqlx::Error> {
        // Highest transaction id across both link columns for the owner. Each
        // inner MAX is a top-1 reverse probe of unshielded_utxos(owner, <link>);
        // aggregate MAX ignores NULLs (unspent rows, absent owner) in both
        // Postgres and SQLite, so no IS NOT NULL guards are needed.
        let query = indoc! {"
            SELECT MAX(tx_id) FROM (
                SELECT MAX(creating_transaction_id) AS tx_id
                FROM unshielded_utxos
                WHERE owner = $1

                UNION ALL

                SELECT MAX(spending_transaction_id)
                FROM unshielded_utxos
                WHERE owner = $1
            ) AS t
        "};

        let (id,) = sqlx::query_as::<_, (Option<i64>,)>(query)
            .bind(address.as_ref())
            .fetch_one(&*self.pool)
            .await?;

        Ok(id.map(|id| id as u64))
    }

    #[allow(clippy::type_complexity)]
    #[trace(properties = { "wallet_id": "{wallet_id}" })]
    async fn get_highest_zswap_end_indices(
        &self,
        wallet_id: Uuid,
    ) -> Result<(Option<u64>, Option<u64>, Option<u64>), sqlx::Error> {
        let query = indoc! {"
            SELECT (
                SELECT MAX(zswap_end_index)
                FROM regular_transactions
            ),
            (
                SELECT zswap_end_index
                FROM regular_transactions
                WHERE id = (
                    SELECT MAX(last_indexed_transaction_id)
                    FROM wallets
                )
            ),
            (
                SELECT zswap_end_index
                FROM regular_transactions
                INNER JOIN relevant_transactions ON regular_transactions.id = relevant_transactions.transaction_id
                INNER JOIN wallets ON wallets.id = relevant_transactions.wallet_id
                WHERE wallets.id = $1
                AND wallets.session_id IS NOT NULL
                ORDER BY zswap_end_index DESC
                LIMIT 1
            )
        "};

        let (
            highest_zswap_end_index,
            highest_checked_zswap_end_index,
            highest_relevant_zswap_end_index,
        ) = sqlx::query_as::<_, (Option<i64>, Option<i64>, Option<i64>)>(query)
            .bind(wallet_id)
            .fetch_one(&*self.pool)
            .await?;

        Ok((
            highest_zswap_end_index.map(|n| n as u64),
            highest_checked_zswap_end_index.map(|n| n as u64),
            highest_relevant_zswap_end_index.map(|n| n as u64),
        ))
    }
}

impl Storage {
    #[trace(properties = {
        "wallet_id": "{wallet_id}",
        "index": "{index}",
        "batch_size": "{batch_size}"
    })]
    async fn get_relevant_transactions(
        &self,
        wallet_id: Uuid,
        index: u64,
        batch_size: NonZeroU32,
    ) -> Result<Vec<RegularTransaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN relevant_transactions ON transactions.id = relevant_transactions.transaction_id
            INNER JOIN wallets ON wallets.id = relevant_transactions.wallet_id
            WHERE wallets.id = $1
            AND wallets.session_id IS NOT NULL
            AND regular_transactions.zswap_start_index >= $2
            ORDER BY transactions.id
            LIMIT $3
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN relevant_transactions ON transactions.id = relevant_transactions.transaction_id
            INNER JOIN wallets ON wallets.id = relevant_transactions.wallet_id
            WHERE wallets.id = $1
            AND wallets.session_id IS NOT NULL
            AND regular_transactions.zswap_start_index >= $2
            ORDER BY transactions.id
            LIMIT $3
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, RegularTransaction>(query)
            .bind(wallet_id)
            .bind(index as i64)
            .bind(batch_size.get() as i64)
            .fetch(&*self.pool)
            .try_collect::<Vec<_>>()
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            transaction.identifiers =
                get_identifiers_for_transaction(transaction.id, &self.pool).await?;
        }

        Ok(transactions)
    }

    #[trace(properties = {
        "address": "{address}",
        "transaction_id": "{transaction_id}",
        "batch_size": "{batch_size}"
    })]
    async fn get_transactions_by_unshielded_address(
        &self,
        address: UnshieldedAddress,
        transaction_id: u64,
        batch_size: NonZeroU32,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        // Collect matching tx ids in a deduplicated UNION subquery (replacing
        // the old SELECT DISTINCT), then probe transactions by primary key.
        // LEFT JOIN regular_transactions: the chain-indexer writes exactly one
        // such row per Regular tx (atomically), so NULLs occur exactly for System.
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            LEFT JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.id IN (
                SELECT creating_transaction_id
                FROM unshielded_utxos
                WHERE owner = $1
                AND creating_transaction_id >= $2

                UNION

                SELECT spending_transaction_id
                FROM unshielded_utxos
                WHERE owner = $1
                AND spending_transaction_id IS NOT NULL
                AND spending_transaction_id >= $2
            )
            ORDER BY transactions.id
            LIMIT $3
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            LEFT JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.id IN (
                SELECT creating_transaction_id
                FROM unshielded_utxos
                WHERE owner = $1
                AND creating_transaction_id >= $2

                UNION

                SELECT spending_transaction_id
                FROM unshielded_utxos
                WHERE owner = $1
                AND spending_transaction_id IS NOT NULL
                AND spending_transaction_id >= $2
            )
            ORDER BY transactions.id
            LIMIT $3
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query(query)
            .bind(address.as_ref())
            .bind(transaction_id as i64)
            .bind(batch_size.get() as i64)
            .fetch(&*self.pool)
            .map_ok(make_transaction)
            .map(|result| result.flatten())
            .try_collect::<Vec<_>>()
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            if let Transaction::Regular(transaction) = transaction {
                transaction.identifiers =
                    get_identifiers_for_transaction(transaction.id, &self.pool).await?;
            }
        }

        self.attach_bridge_claims(transactions.iter_mut()).await?;

        Ok(transactions)
    }

    #[cfg(feature = "cloud")]
    async fn fetch_transactions_by_block_ids(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, Transaction)>, sqlx::Error> {
        let ids = ids.iter().map(|id| *id as i64).collect::<Vec<_>>();

        let query = indoc! {"
            SELECT
                transactions.block_id,
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.block_id = ANY($1)

            UNION ALL

            SELECT
                transactions.block_id,
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                NULL AS transaction_result,
                NULL AS zswap_merkle_tree_root,
                NULL AS zswap_start_index,
                NULL AS zswap_end_index,
                NULL AS dust_commitment_start_index,
                NULL AS dust_commitment_end_index,
                NULL AS dust_generation_start_index,
                NULL AS dust_generation_end_index,
                NULL AS paid_fees,
                NULL AS estimated_fees,
                NULL AS identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            WHERE transactions.block_id = ANY($1)
            AND transactions.variant = 'System'

            ORDER BY block_id, id
        "};

        sqlx::query(query)
            .bind(ids)
            .fetch(&*self.pool)
            .map_ok(make_transaction_with_block_id)
            .map(|result| result.flatten())
            .try_collect()
            .await
    }

    #[cfg(feature = "standalone")]
    async fn fetch_transactions_by_block_ids(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, Transaction)>, sqlx::Error> {
        use sqlx::{QueryBuilder, Sqlite};

        let mut qb = QueryBuilder::<Sqlite>::new("WITH block_ids(id) AS (VALUES (");
        let mut sep = qb.separated("), (");
        for id in ids {
            sep.push_bind(*id as i64);
        }
        qb.push(indoc! {"
            ))
            SELECT
                transactions.block_id AS block_id,
                transactions.id AS id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.zswap_merkle_tree_root,
                regular_transactions.zswap_start_index,
                regular_transactions.zswap_end_index,
                regular_transactions.dust_commitment_start_index,
                regular_transactions.dust_commitment_end_index,
                regular_transactions.dust_generation_start_index,
                regular_transactions.dust_generation_end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.block_id IN (SELECT id FROM block_ids)
            UNION ALL
            SELECT
                transactions.block_id,
                transactions.id,
                transactions.variant,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                NULL AS transaction_result,
                NULL AS zswap_merkle_tree_root,
                NULL AS zswap_start_index,
                NULL AS zswap_end_index,
                NULL AS dust_commitment_start_index,
                NULL AS dust_commitment_end_index,
                NULL AS dust_generation_start_index,
                NULL AS dust_generation_end_index,
                NULL AS paid_fees,
                NULL AS estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            WHERE transactions.block_id IN (SELECT id FROM block_ids)
            AND transactions.variant = 'System'
            ORDER BY block_id, id
        "});

        let mut transactions = qb
            .build()
            .fetch(&*self.pool)
            .map_ok(make_transaction_with_block_id)
            .map(|result| result.flatten())
            .try_collect::<Vec<_>>()
            .await?;

        for (_, transaction) in transactions.iter_mut() {
            if let Transaction::Regular(transaction) = transaction {
                transaction.identifiers =
                    get_identifiers_for_transaction(transaction.id, &self.pool).await?;
            }
        }

        Ok(transactions)
    }
}

#[cfg(feature = "cloud")]
fn make_transaction(row: sqlx::postgres::PgRow) -> Result<Transaction, sqlx::Error> {
    let variant = row.try_get::<TransactionVariant, _>("variant")?;
    let transaction = match variant {
        TransactionVariant::Regular => Transaction::Regular(RegularTransaction::from_row(&row)?),
        TransactionVariant::System => Transaction::System(SystemTransaction::from_row(&row)?),
    };
    Ok(transaction)
}

#[cfg(feature = "standalone")]
fn make_transaction(row: sqlx::sqlite::SqliteRow) -> Result<Transaction, sqlx::Error> {
    let variant = row.try_get::<TransactionVariant, _>("variant")?;
    let transaction = match variant {
        TransactionVariant::Regular => Transaction::Regular(RegularTransaction::from_row(&row)?),
        TransactionVariant::System => Transaction::System(SystemTransaction::from_row(&row)?),
    };
    Ok(transaction)
}

#[cfg(feature = "cloud")]
fn make_transaction_with_block_id(
    row: sqlx::postgres::PgRow,
) -> Result<(u64, Transaction), sqlx::Error> {
    let block_id = row.try_get::<i64, _>("block_id")? as u64;
    let transaction = make_transaction(row)?;
    Ok((block_id, transaction))
}

#[cfg(feature = "standalone")]
fn make_transaction_with_block_id(
    row: sqlx::sqlite::SqliteRow,
) -> Result<(u64, Transaction), sqlx::Error> {
    let block_id = row.try_get::<i64, _>("block_id")? as u64;
    let transaction = make_transaction(row)?;
    Ok((block_id, transaction))
}

#[cfg(feature = "standalone")]
async fn get_identifiers_for_transaction(
    transaction_id: u64,
    pool: &indexer_common::infra::pool::sqlite::SqlitePool,
) -> Result<Vec<SerializedTransactionIdentifier>, sqlx::Error> {
    use futures::TryStreamExt;

    let query = indoc! {"
        SELECT identifier
        FROM transaction_identifiers
        WHERE transaction_id = $1
        ORDER BY id
    "};

    sqlx::query_as::<_, (SerializedTransactionIdentifier,)>(query)
        .bind(transaction_id as i64)
        .fetch(&**pool)
        .map_ok(|(identifier,)| identifier)
        .try_collect()
        .await
}

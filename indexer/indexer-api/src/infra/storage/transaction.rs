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
    domain::{Transaction, storage::transaction::TransactionStorage},
    infra::storage::Storage,
};
use async_stream::try_stream;
use fastrace::trace;
use futures::Stream;
use indexer_common::{
    domain::{
        SessionId,
        ledger::{RawUnshieldedAddress, SerializedTransactionIdentifier, TransactionHash},
    },
    stream::flatten_chunks,
};
use indoc::indoc;
use std::num::NonZeroU32;

impl TransactionStorage for Storage {
    #[trace(properties = { "id": "{id}" })]
    async fn get_transaction_by_id(&self, id: u64) -> Result<Option<Transaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.id = $1
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
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.id = $1
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transaction = sqlx::query_as::<_, Transaction>(query)
            .bind(id as i64)
            .fetch_optional(&*self.pool)
            .await?;

        #[cfg(feature = "standalone")]
        if let Some(transaction) = &mut transaction {
            transaction.identifiers =
                get_identifiers_for_transaction(transaction.id, &self.pool).await?;
        }

        Ok(transaction)
    }

    #[trace(properties = { "id": "{id}" })]
    async fn get_transactions_by_block_id(&self, id: u64) -> Result<Vec<Transaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.block_id = $1
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
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.block_id = $1
            ORDER BY transactions.id
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, Transaction>(query)
            .bind(id as i64)
            .fetch_all(&*self.pool)
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            transaction.identifiers =
                get_identifiers_for_transaction(transaction.id, &self.pool).await?;
        }

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
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.hash = $1
            ORDER BY transactions.id DESC
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
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            WHERE transactions.hash = $1
            ORDER BY transactions.id DESC
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, Transaction>(query)
            .bind(hash.as_ref())
            .fetch_all(&*self.pool)
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            transaction.identifiers =
                get_identifiers_for_transaction(transaction.id, &self.pool).await?;
        }

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
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
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
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN transaction_identifiers ON transactions.id = transaction_identifiers.transaction_id
            WHERE transaction_identifiers.identifier = $1
            ORDER BY transactions.id
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, Transaction>(query)
            .bind(identifier)
            .fetch_all(&*self.pool)
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            transaction.identifiers =
                get_identifiers_for_transaction(transaction.id, &self.pool).await?;
        }

        Ok(transactions)
    }

    fn get_relevant_transactions(
        &self,
        session_id: SessionId,
        mut index: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<Transaction, sqlx::Error>> + Send {
        let chunks = try_stream! {
            loop {
                let transactions = self
                    .get_relevant_transactions(session_id, index, batch_size)
                    .await?;

                match transactions.last() {
                    Some(transaction) => index = transaction.end_index + 1,
                    None => break,
                }

                yield transactions;
            }
        };

        flatten_chunks(chunks)
    }

    fn get_transactions_involving_unshielded(
        &self,
        address: RawUnshieldedAddress,
        mut transaction_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<Transaction, sqlx::Error>> + Send {
        let chunks = try_stream! {
            loop {
                let transactions = self
                    .get_transactions_involving_unshielded(address, transaction_id, batch_size)
                    .await?;

                match transactions.last() {
                    Some(transaction) => transaction_id = transaction.id + 1,
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
        address: RawUnshieldedAddress,
    ) -> Result<Option<u64>, sqlx::Error> {
        let query = indoc! {"
            SELECT MAX(transactions.id)
            FROM transactions
            INNER JOIN unshielded_utxos ON
                unshielded_utxos.creating_transaction_id = transactions.id OR
                unshielded_utxos.spending_transaction_id = transactions.id
            WHERE unshielded_utxos.owner = $1
        "};

        let (id,) = sqlx::query_as::<_, (Option<i64>,)>(query)
            .bind(address.as_ref())
            .fetch_one(&*self.pool)
            .await?;

        Ok(id.map(|id| id as u64))
    }

    #[trace(properties = { "session_id": "{session_id}" })]
    async fn get_highest_end_indices(
        &self,
        session_id: SessionId,
    ) -> Result<(Option<u64>, Option<u64>, Option<u64>), sqlx::Error> {
        let query = indoc! {"
            SELECT (
                SELECT MAX(end_index)
                FROM regular_transactions
            ),
            (
                SELECT end_index
                FROM regular_transactions
                WHERE id = (
                    SELECT MAX(last_indexed_transaction_id)
                    FROM wallets
                )
            ),
            (
                SELECT end_index
                FROM regular_transactions
                INNER JOIN relevant_transactions ON regular_transactions.id = relevant_transactions.transaction_id
                INNER JOIN wallets ON wallets.id = relevant_transactions.wallet_id
                WHERE wallets.session_id = $1
                ORDER BY end_index DESC
                LIMIT 1
            )
        "};

        let (highest_end_index, highest_checked_end_index, highest_relevant_end_index) =
            sqlx::query_as::<_, (Option<i64>, Option<i64>, Option<i64>)>(query)
                .bind(session_id.as_ref())
                .fetch_one(&*self.pool)
                .await?;

        Ok((
            highest_end_index.map(|n| n as u64),
            highest_checked_end_index.map(|n| n as u64),
            highest_relevant_end_index.map(|n| n as u64),
        ))
    }
}

impl Storage {
    #[trace(properties = {
        "session_id": "{session_id}",
        "index": "{index}",
        "batch_size": "{batch_size}"
    })]
    async fn get_relevant_transactions(
        &self,
        session_id: SessionId,
        index: u64,
        batch_size: NonZeroU32,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN relevant_transactions ON transactions.id = relevant_transactions.transaction_id
            INNER JOIN wallets ON wallets.id = relevant_transactions.wallet_id
            WHERE wallets.session_id = $1
            AND regular_transactions.start_index >= $2
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
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN relevant_transactions ON transactions.id = relevant_transactions.transaction_id
            INNER JOIN wallets ON wallets.id = relevant_transactions.wallet_id
            WHERE wallets.session_id = $1
            AND regular_transactions.start_index >= $2
            ORDER BY transactions.id
            LIMIT $3
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, Transaction>(query)
            .bind(session_id.as_ref())
            .bind(index as i64)
            .bind(batch_size.get() as i64)
            .fetch_all(&*self.pool)
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
    async fn get_transactions_involving_unshielded(
        &self,
        address: RawUnshieldedAddress,
        transaction_id: u64,
        batch_size: NonZeroU32,
    ) -> Result<Vec<Transaction>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT DISTINCT
                transactions.id,
                transactions.hash,
                transactions.protocol_version,
                transactions.raw,
                blocks.hash AS block_hash,
                regular_transactions.transaction_result,
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees,
                regular_transactions.identifiers
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN unshielded_utxos ON
                unshielded_utxos.creating_transaction_id = transactions.id OR
                unshielded_utxos.spending_transaction_id = transactions.id
            WHERE unshielded_utxos.owner = $1
            AND transactions.id >= $2
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
                regular_transactions.merkle_tree_root,
                regular_transactions.start_index,
                regular_transactions.end_index,
                regular_transactions.paid_fees,
                regular_transactions.estimated_fees
            FROM transactions
            INNER JOIN blocks ON blocks.id = transactions.block_id
            INNER JOIN regular_transactions ON regular_transactions.id = transactions.id
            INNER JOIN unshielded_utxos ON
                unshielded_utxos.creating_transaction_id = transactions.id OR
                unshielded_utxos.spending_transaction_id = transactions.id
            WHERE unshielded_utxos.owner = $1
            AND transactions.id >= $2
            ORDER BY transactions.id
            LIMIT $3
        "};

        #[cfg_attr(feature = "cloud", allow(unused_mut))]
        let mut transactions = sqlx::query_as::<_, Transaction>(query)
            .bind(address.as_ref())
            .bind(transaction_id as i64)
            .bind(batch_size.get() as i64)
            .fetch_all(&*self.pool)
            .await?;

        #[cfg(feature = "standalone")]
        for transaction in transactions.iter_mut() {
            transaction.identifiers =
                get_identifiers_for_transaction(transaction.id, &self.pool).await?;
        }

        Ok(transactions)
    }
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
    "};

    sqlx::query_as::<_, (SerializedTransactionIdentifier,)>(query)
        .bind(transaction_id as i64)
        .fetch(&**pool)
        .map_ok(|(identifier,)| identifier)
        .try_collect()
        .await
}

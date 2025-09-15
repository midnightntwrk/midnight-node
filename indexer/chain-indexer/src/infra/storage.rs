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

use crate::domain::{
    self, Block, BlockTransactions, ContractAction, RegularTransaction, SystemTransaction,
    Transaction, TransactionVariant, node::BlockInfo,
};
use fastrace::trace;
use futures::{TryFutureExt, TryStreamExt};
use indexer_common::{
    domain::{
        BlockHash, ByteVec,
        ledger::{ContractAttributes, ContractBalance, UnshieldedUtxo},
    },
    infra::sqlx::U128BeBytes,
};
use indoc::indoc;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Type, types::Json};
use std::iter;

#[cfg(feature = "cloud")]
/// Sqlx transaction for Postgres.
type SqlxTransaction = sqlx::Transaction<'static, sqlx::Postgres>;

#[cfg(feature = "standalone")]
/// Sqlx transaction for Sqlite.
type SqlxTransaction = sqlx::Transaction<'static, sqlx::Sqlite>;

/// Unified storage implementation for PostgreSQL (cloud) and SQLite (standalone). Uses Cargo
/// features to select the appropriate database backend at build time.
#[derive(Debug, Clone)]
pub struct Storage {
    #[cfg(feature = "cloud")]
    pool: indexer_common::infra::pool::postgres::PostgresPool,

    #[cfg(feature = "standalone")]
    pool: indexer_common::infra::pool::sqlite::SqlitePool,
}

impl Storage {
    #[cfg(feature = "cloud")]
    pub fn new(pool: indexer_common::infra::pool::postgres::PostgresPool) -> Self {
        Self { pool }
    }

    #[cfg(feature = "standalone")]
    pub fn new(pool: indexer_common::infra::pool::sqlite::SqlitePool) -> Self {
        Self { pool }
    }
}

impl domain::storage::Storage for Storage {
    #[trace]
    async fn get_highest_block_info(&self) -> Result<Option<BlockInfo>, sqlx::Error> {
        let query = indoc! {"
            SELECT hash, height
            FROM blocks
            ORDER BY height DESC
            LIMIT 1
        "};

        sqlx::query_as::<_, (ByteVec, i64)>(query)
            .fetch_optional(&*self.pool)
            .await?
            .map(|(hash, height)| {
                let hash = BlockHash::try_from(hash.as_ref())
                    .map_err(|error| sqlx::Error::Decode(error.into()))?;

                Ok(BlockInfo {
                    hash,
                    height: height as u32,
                })
            })
            .transpose()
    }

    #[trace]
    async fn get_transaction_count(&self) -> Result<u64, sqlx::Error> {
        let query = indoc! {"
            SELECT count(*) 
            FROM transactions
        "};

        let (count,) = sqlx::query_as::<_, (i64,)>(query)
            .fetch_one(&*self.pool)
            .await?;

        Ok(count as u64)
    }

    #[trace]
    async fn get_contract_action_count(&self) -> Result<(u64, u64, u64), sqlx::Error> {
        let query = indoc! {"
            SELECT count(*) 
            FROM contract_actions
            WHERE variant = $1
        "};

        let (deploy_count,) = sqlx::query_as::<_, (i64,)>(query)
            .bind(ContractActionVariant::Deploy)
            .fetch_one(&*self.pool)
            .await?;
        let (call_count,) = sqlx::query_as::<_, (i64,)>(query)
            .bind(ContractActionVariant::Call)
            .fetch_one(&*self.pool)
            .await?;
        let (update_count,) = sqlx::query_as::<_, (i64,)>(query)
            .bind(ContractActionVariant::Update)
            .fetch_one(&*self.pool)
            .await?;

        Ok((deploy_count as u64, call_count as u64, update_count as u64))
    }

    #[trace(properties = { "block_height": "{block_height}" })]
    async fn get_block_transactions(
        &self,
        block_height: u32,
    ) -> Result<BlockTransactions, sqlx::Error> {
        let query = indoc! {"
            SELECT
                id,
                protocol_version,
                parent_hash,
                timestamp
            FROM blocks
            WHERE height = $1
        "};

        let (block_id, protocol_version, block_parent_hash, block_timestamp) =
            sqlx::query_as::<_, (i64, i64, ByteVec, i64)>(query)
                .bind(block_height as i64)
                .fetch_one(&*self.pool)
                .await?;
        let block_parent_hash = BlockHash::try_from(block_parent_hash.as_ref())
            .map_err(|error| sqlx::Error::Decode(error.into()))?;

        let query = indoc! {"
            SELECT
                variant,
                raw
            FROM transactions
            WHERE block_id = $1
        "};

        let transactions = sqlx::query_as::<_, (TransactionVariant, ByteVec)>(query)
            .bind(block_id)
            .fetch(&*self.pool)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(BlockTransactions {
            transactions,
            protocol_version: (protocol_version as u32).into(),
            block_parent_hash,
            block_timestamp: block_timestamp as u64,
        })
    }

    #[trace]
    async fn save_block(
        &self,
        block: &Block,
        transactions: &[Transaction],
    ) -> Result<Option<u64>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let max_transaction_id = save_block(block, transactions, &mut tx).await?;
        tx.commit().await?;

        Ok(max_transaction_id)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "cloud", sqlx(type_name = "CONTRACT_ACTION_VARIANT"))]
enum ContractActionVariant {
    /// A contract deployment.
    #[default]
    Deploy,

    /// A contract call.
    Call,

    /// A contract update.
    Update,
}

impl From<&ContractAttributes> for ContractActionVariant {
    fn from(attributes: &ContractAttributes) -> Self {
        match attributes {
            ContractAttributes::Deploy => Self::Deploy,
            ContractAttributes::Call { .. } => Self::Call,
            ContractAttributes::Update => Self::Update,
        }
    }
}

#[trace]
async fn save_block(
    block: &Block,
    transactions: &[Transaction],
    tx: &mut SqlxTransaction,
) -> Result<Option<u64>, sqlx::Error> {
    let query = indoc! {"
        INSERT INTO blocks (
            hash,
            height,
            protocol_version,
            parent_hash,
            author,
            timestamp
        )
    "};

    let block_id = QueryBuilder::new(query)
        .push_values([block], |mut q, block| {
            let Block {
                hash,
                height,
                protocol_version,
                parent_hash,
                author,
                timestamp,
                ..
            } = block;

            q.push_bind(hash.as_ref())
                .push_bind(*height as i64)
                .push_bind(protocol_version.0 as i64)
                .push_bind(parent_hash.as_ref())
                .push_bind(author.as_ref().map(|a| a.as_ref()))
                .push_bind(*timestamp as i64);
        })
        .push(" RETURNING id")
        .build_query_as::<(i64,)>()
        .fetch_one(&mut **tx)
        .map_ok(|(id,)| id)
        .await?;

    save_transactions(transactions, block_id, tx).await
}

#[trace(properties = { "block_id": "{block_id}" })]
async fn save_transactions(
    transactions: &[Transaction],
    block_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<Option<u64>, sqlx::Error> {
    let mut highest_transaction_id = None;

    for transaction in transactions {
        let query = indoc! {"
            INSERT INTO transactions (
                block_id,
                variant,
                hash,
                protocol_version,
                raw
            )
        "};

        let hash = transaction.hash();
        let transaction_id = QueryBuilder::new(query)
            .push_values(iter::once(transaction), |mut q, transaction| {
                q.push_bind(block_id)
                    .push_bind(transaction.variant())
                    .push_bind(hash.as_ref())
                    .push_bind(transaction.protocol_version().0 as i64)
                    .push_bind(transaction.raw());
            })
            .push(" RETURNING id")
            .build_query_as::<(i64,)>()
            .fetch_one(&mut **tx)
            .map_ok(|(id,)| id)
            .await?;

        match transaction {
            Transaction::Regular(transaction) => {
                highest_transaction_id = Some(
                    save_regular_transaction(transaction, transaction_id, block_id, tx).await?,
                );
            }

            Transaction::System(transaction) => {
                save_system_transaction(transaction, transaction_id, block_id, tx).await?
            }
        }
    }

    Ok(highest_transaction_id)
}

#[trace(properties = { "block_id": "{block_id}" })]
async fn save_regular_transaction(
    transaction: &RegularTransaction,
    transaction_id: i64,
    block_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<u64, sqlx::Error> {
    #[cfg(feature = "cloud")]
    let query = indoc! {"
        INSERT INTO regular_transactions (
            id,
            transaction_result,
            merkle_tree_root,
            start_index,
            end_index,
            paid_fees,
            estimated_fees,
            identifiers
        )
    "};
    #[cfg(feature = "standalone")]
    let query = indoc! {"
        INSERT INTO regular_transactions (
            id,
            transaction_result,
            merkle_tree_root,
            start_index,
            end_index,
            paid_fees,
            estimated_fees
        )
    "};

    let transaction_id = QueryBuilder::new(query)
        .push_values(iter::once(transaction), |mut q, transaction| {
            q.push_bind(transaction_id)
                .push_bind(Json(&transaction.transaction_result))
                .push_bind(&transaction.merkle_tree_root)
                .push_bind(transaction.start_index as i64)
                .push_bind(transaction.end_index as i64)
                .push_bind(U128BeBytes::from(transaction.paid_fees))
                .push_bind(U128BeBytes::from(transaction.estimated_fees));
            #[cfg(feature = "cloud")]
            q.push_bind(&transaction.identifiers);
        })
        .push(" RETURNING id")
        .build_query_as::<(i64,)>()
        .fetch_one(&mut **tx)
        .map_ok(|(id,)| id)
        .await?;

    #[cfg(feature = "standalone")]
    save_identifiers(&transaction.identifiers, transaction_id, tx).await?;

    let contract_action_ids =
        save_contract_actions(&transaction.contract_actions, transaction_id, tx).await?;
    let contract_balances = transaction
        .contract_actions
        .iter()
        .zip(contract_action_ids.iter())
        .flat_map(|(action, &action_id)| {
            action
                .extracted_balances
                .iter()
                .map(move |&balance| (action_id, balance))
        })
        .collect::<Vec<_>>();
    save_contract_balances(contract_balances, tx).await?;

    save_unshielded_utxos(
        &transaction.created_unshielded_utxos,
        transaction_id,
        false,
        tx,
    )
    .await?;
    save_unshielded_utxos(
        &transaction.spent_unshielded_utxos,
        transaction_id,
        true,
        tx,
    )
    .await?;

    Ok(transaction_id as u64)
}

#[trace(properties = { "block_id": "{block_id}" })]
async fn save_system_transaction(
    _transaction: &SystemTransaction,
    _transaction_id: i64,
    block_id: i64,
    _tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    // TODO: Store DistributeReserve and other (DUST-related) system transactions.
    Ok(())
}

#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_unshielded_utxos(
    utxos: &[UnshieldedUtxo],
    transaction_id: i64,
    spent: bool,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if utxos.is_empty() {
        return Ok(());
    }

    if spent {
        for &utxo in utxos {
            let query = indoc! {"
                INSERT INTO unshielded_utxos (
                    creating_transaction_id,
                    spending_transaction_id,
                    owner,
                    token_type,
                    value,
                    output_index,
                    intent_hash
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (intent_hash, output_index)
                DO UPDATE SET spending_transaction_id = $2
                WHERE unshielded_utxos.spending_transaction_id IS NULL
            "};

            let UnshieldedUtxo {
                owner,
                token_type,
                value,
                intent_hash,
                output_index,
            } = utxo;

            sqlx::query(query)
                .bind(transaction_id)
                .bind(transaction_id)
                .bind(owner.as_ref())
                .bind(token_type.as_ref())
                .bind(U128BeBytes::from(value))
                .bind(output_index as i32)
                .bind(intent_hash.as_ref())
                .execute(&mut **tx)
                .await?;
        }
    } else {
        let query_base = indoc! {"
            INSERT INTO unshielded_utxos (
                creating_transaction_id,
                owner,
                token_type,
                value,
                output_index,
                intent_hash
            )
        "};

        QueryBuilder::new(query_base)
            .push_values(utxos.iter(), |mut q, utxo| {
                let UnshieldedUtxo {
                    owner,
                    token_type,
                    value,
                    intent_hash,
                    output_index,
                } = utxo;

                q.push_bind(transaction_id)
                    .push_bind(owner.as_ref())
                    .push_bind(token_type.as_ref())
                    .push_bind(U128BeBytes::from(*value))
                    .push_bind(*output_index as i32)
                    .push_bind(intent_hash.as_ref());
            })
            .build()
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_contract_actions(
    contract_actions: &[ContractAction],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<Vec<i64>, sqlx::Error> {
    if contract_actions.is_empty() {
        return Ok(Vec::new());
    }

    let query = indoc! {"
        INSERT INTO contract_actions (
            transaction_id,
            variant,
            address,
            state,
            chain_state,
            attributes
        )
    "};

    let contract_action_ids = QueryBuilder::new(query)
        .push_values(contract_actions.iter(), |mut q, action| {
            q.push_bind(transaction_id)
                .push_bind(ContractActionVariant::from(&action.attributes))
                .push_bind(&action.address)
                .push_bind(&action.state)
                .push_bind(&action.chain_state)
                .push_bind(Json(&action.attributes));
        })
        .push(" RETURNING id")
        .build_query_as::<(i64,)>()
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect();

    Ok(contract_action_ids)
}

#[trace]
async fn save_contract_balances(
    balances: Vec<(i64, ContractBalance)>,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if !balances.is_empty() {
        let query = indoc! {"
            INSERT INTO contract_balances (
                contract_action_id,
                token_type,
                amount
            )
        "};

        QueryBuilder::new(query)
            .push_values(balances.iter(), |mut q, (action_id, balance)| {
                q.push_bind(*action_id)
                    .push_bind(balance.token_type.as_ref())
                    .push_bind(U128BeBytes::from(balance.amount));
            })
            .build()
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

#[cfg(feature = "standalone")]
async fn save_identifiers(
    identifiers: &[indexer_common::domain::ledger::SerializedTransactionIdentifier],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if !identifiers.is_empty() {
        let query = indoc! {"
            INSERT INTO transaction_identifiers (
                transaction_id,
                identifier
            )
        "};

        QueryBuilder::new(query)
            .push_values(identifiers.iter(), |mut q, identifier| {
                q.push_bind(transaction_id).push_bind(identifier);
            })
            .build()
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

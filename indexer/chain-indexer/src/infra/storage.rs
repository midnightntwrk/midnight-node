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

use crate::domain::{
    self, Block, BlockRef, ContractAction, DParameter, DustRegistrationEvent, RegularTransaction,
    SystemParametersChange, SystemTransaction, TermsAndConditions, Transaction,
};
use fastrace::trace;
use futures::TryFutureExt;
use indexer_common::{
    domain::{
        BlockHash, ByteVec, ContractAttributes, ContractBalance, LedgerEvent,
        LedgerEventAttributes, LedgerEventGrouping, ProtocolVersion, SerializedLedgerStateKey,
        TermsAndConditionsHash, UnshieldedUtxo, bridge::BridgeEvent,
    },
    infra::sqlx::U128BeBytes,
};
use indoc::indoc;
use log::debug;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Type, types::Json};
use std::num::NonZeroUsize;

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
    async fn save_block(
        &mut self,
        block: &Block,
        transactions: &[Transaction],
        dust_registration_events: &[DustRegistrationEvent],
        ledger_state_key: &SerializedLedgerStateKey,
        system_parameters_change: Option<&SystemParametersChange>,
    ) -> Result<Option<u64>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let max_transaction_id = save_block(
            block,
            transactions,
            dust_registration_events,
            ledger_state_key,
            &mut tx,
        )
        .await?;

        if let Some(change) = system_parameters_change {
            save_system_parameters_change(change, &mut tx).await?;
        }

        tx.commit().await?;

        Ok(max_transaction_id)
    }

    #[trace]
    async fn get_highest_block(
        &self,
    ) -> Result<Option<(BlockRef, ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error> {
        let query = indoc! {"
            SELECT hash, height, protocol_version, ledger_state_key
            FROM blocks
            ORDER BY height DESC
            LIMIT 1
        "};

        sqlx::query_as::<_, (ByteVec, i64, i64, SerializedLedgerStateKey)>(query)
            .fetch_optional(&*self.pool)
            .await?
            .map(|(hash, height, protocol_version, key)| {
                let hash = BlockHash::try_from(hash.as_ref())
                    .map_err(|error| sqlx::Error::Decode(error.into()))?;

                let block_ref = BlockRef {
                    hash,
                    height: height as u64,
                };

                let protocol_version = ProtocolVersion::try_from(protocol_version)
                    .map_err(|error| sqlx::Error::Decode(error.into()))?;

                Ok((block_ref, protocol_version, key))
            })
            .transpose()
    }

    #[trace]
    async fn get_highest_block_timestamp(&self) -> Result<Option<u64>, sqlx::Error> {
        let query = indoc! {"
            SELECT timestamp
            FROM blocks
            ORDER BY height DESC
            LIMIT 1
        "};

        let timestamp = sqlx::query_as::<_, (i64,)>(query)
            .fetch_optional(&*self.pool)
            .await?
            .map(|(timestamp,)| timestamp as u64);

        Ok(timestamp)
    }

    #[trace]
    async fn get_newest_ledger_state_keys(
        &self,
        limit: NonZeroUsize,
    ) -> Result<Vec<(ProtocolVersion, SerializedLedgerStateKey)>, sqlx::Error> {
        let query = indoc! {"
            SELECT protocol_version, ledger_state_key
            FROM blocks
            ORDER BY height DESC
            LIMIT $1
        "};

        let mut keys = sqlx::query_as::<_, (i64, SerializedLedgerStateKey)>(query)
            .bind(limit.get() as i64)
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|(protocol_version, key)| {
                let protocol_version = ProtocolVersion::try_from(protocol_version)
                    .map_err(|error| sqlx::Error::Decode(error.into()))?;
                Ok((protocol_version, key))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        keys.reverse();

        Ok(keys)
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

    #[trace]
    async fn get_latest_d_parameter(&self) -> Result<Option<DParameter>, sqlx::Error> {
        let query = indoc! {"
            SELECT num_permissioned_candidates, num_registered_candidates
            FROM system_parameters_d
            ORDER BY block_height DESC
            LIMIT 1
        "};

        let result = sqlx::query_as::<_, (i32, i32)>(query)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(result.map(|(num_perm, num_reg)| DParameter {
            num_permissioned_candidates: num_perm as u16,
            num_registered_candidates: num_reg as u16,
        }))
    }

    #[trace]
    async fn get_latest_terms_and_conditions(
        &self,
    ) -> Result<Option<TermsAndConditions>, sqlx::Error> {
        let query = indoc! {"
            SELECT hash, url
            FROM system_parameters_terms_and_conditions
            ORDER BY block_height DESC
            LIMIT 1
        "};

        sqlx::query_as::<_, (ByteVec, String)>(query)
            .fetch_optional(&*self.pool)
            .await?
            .map(|(hash_bytes, url)| {
                let hash = TermsAndConditionsHash::try_from(hash_bytes)
                    .map_err(|error| sqlx::Error::Decode(error.into()))?;
                Ok(TermsAndConditions { hash, url })
            })
            .transpose()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "cloud", sqlx(type_name = "CONTRACT_ACTION_VARIANT"))]
enum ContractActionVariant {
    Deploy,
    Call,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "cloud", sqlx(type_name = "LEDGER_EVENT_VARIANT"))]
pub enum LedgerEventVariant {
    ZswapInput,
    ZswapOutput,
    ParamChange,
    DustInitialUtxo,
    DustGenerationDtimeUpdate,
    DustSpendProcessed,
    // Contract event variants (per MIP-107 / CoIP-442 LogEventType enum).
    ShieldedSpend,
    ShieldedReceive,
    ShieldedMint,
    ShieldedBurn,
    UnshieldedSpend,
    UnshieldedReceive,
    UnshieldedMint,
    UnshieldedBurn,
    Paused,
    Unpaused,
    Misc,
}

impl From<&LedgerEventAttributes> for LedgerEventVariant {
    fn from(attributes: &LedgerEventAttributes) -> Self {
        match attributes {
            LedgerEventAttributes::ZswapInput { .. } => Self::ZswapInput,
            LedgerEventAttributes::ZswapOutput => Self::ZswapOutput,
            LedgerEventAttributes::ParamChange => Self::ParamChange,
            LedgerEventAttributes::DustInitialUtxo { .. } => Self::DustInitialUtxo,
            LedgerEventAttributes::DustGenerationDtimeUpdate { .. } => {
                Self::DustGenerationDtimeUpdate
            }
            LedgerEventAttributes::DustSpendProcessed { .. } => Self::DustSpendProcessed,
            LedgerEventAttributes::ContractShieldedSpend { .. } => Self::ShieldedSpend,
            LedgerEventAttributes::ContractShieldedReceive { .. } => Self::ShieldedReceive,
            LedgerEventAttributes::ContractShieldedMint { .. } => Self::ShieldedMint,
            LedgerEventAttributes::ContractShieldedBurn { .. } => Self::ShieldedBurn,
            LedgerEventAttributes::ContractUnshieldedSpend { .. } => Self::UnshieldedSpend,
            LedgerEventAttributes::ContractUnshieldedReceive { .. } => Self::UnshieldedReceive,
            LedgerEventAttributes::ContractUnshieldedMint { .. } => Self::UnshieldedMint,
            LedgerEventAttributes::ContractUnshieldedBurn { .. } => Self::UnshieldedBurn,
            LedgerEventAttributes::ContractPaused { .. } => Self::Paused,
            LedgerEventAttributes::ContractUnpaused { .. } => Self::Unpaused,
            LedgerEventAttributes::ContractMisc { .. } => Self::Misc,
        }
    }
}

#[trace]
async fn save_block(
    block: &Block,
    transactions: &[Transaction],
    dust_registration_events: &[DustRegistrationEvent],
    ledger_state_key: &SerializedLedgerStateKey,
    tx: &mut SqlxTransaction,
) -> Result<Option<u64>, sqlx::Error> {
    let query = indoc! {"
        INSERT INTO blocks (
            hash,
            height,
            protocol_version,
            parent_hash,
            author,
            timestamp,
            zswap_merkle_tree_root,
            ledger_parameters,
            ledger_state_key,
            zswap_end_index,
            dust_commitment_end_index,
            dust_generation_end_index,
            dust_commitment_merkle_tree_root,
            dust_generation_merkle_tree_root
        )
    "};

    let block_id = QueryBuilder::new(query)
        .push_values([()], |mut q, _| {
            let Block {
                hash,
                height,
                protocol_version,
                parent_hash,
                author,
                timestamp,
                zswap_merkle_tree_root,
                ledger_parameters,
                zswap_end_index,
                dust_commitment_end_index,
                dust_generation_end_index,
                dust_commitment_merkle_tree_root,
                dust_generation_merkle_tree_root,
                ..
            } = block;

            q.push_bind(hash.as_ref())
                .push_bind(*height as i64)
                .push_bind(protocol_version.into_i64())
                .push_bind(parent_hash.as_ref())
                .push_bind(author.as_ref().map(|a| a.as_ref()))
                .push_bind(*timestamp as i64)
                .push_bind(zswap_merkle_tree_root.as_ref())
                .push_bind(ledger_parameters.as_ref())
                .push_bind(ledger_state_key)
                .push_bind(*zswap_end_index as i64)
                .push_bind(*dust_commitment_end_index as i64)
                .push_bind(*dust_generation_end_index as i64)
                .push_bind(dust_commitment_merkle_tree_root.as_ref())
                .push_bind(dust_generation_merkle_tree_root.as_ref());
        })
        .push(" RETURNING id")
        .build_query_as::<(i64,)>()
        .fetch_one(&mut **tx)
        .map_ok(|(id,)| id)
        .await?;

    let max_transaction_id = save_transactions(transactions, block_id, tx).await?;

    save_dust_registration_events(dust_registration_events, block_id, block.timestamp, tx).await?;

    save_bridge_events(&block.bridge_events, block_id, tx).await?;

    Ok(max_transaction_id)
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
            .push_values([()], |mut q, _| {
                q.push_bind(block_id)
                    .push_bind(transaction.variant())
                    .push_bind(hash.as_ref())
                    .push_bind(transaction.protocol_version().into_i64())
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
                save_system_transaction(transaction, transaction_id, tx).await?
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
            zswap_merkle_tree_root,
            zswap_start_index,
            zswap_end_index,
            dust_commitment_start_index,
            dust_commitment_end_index,
            dust_generation_start_index,
            dust_generation_end_index,
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
            zswap_merkle_tree_root,
            zswap_start_index,
            zswap_end_index,
            dust_commitment_start_index,
            dust_commitment_end_index,
            dust_generation_start_index,
            dust_generation_end_index,
            paid_fees,
            estimated_fees
        )
    "};

    let transaction_id = QueryBuilder::new(query)
        .push_values([()], |mut q, _| {
            q.push_bind(transaction_id)
                .push_bind(Json(&transaction.transaction_result))
                .push_bind(&transaction.zswap_merkle_tree_root)
                .push_bind(transaction.zswap_start_index as i64)
                .push_bind(transaction.zswap_end_index as i64)
                .push_bind(transaction.dust_commitment_start_index as i64)
                .push_bind(transaction.dust_commitment_end_index as i64)
                .push_bind(transaction.dust_generation_start_index as i64)
                .push_bind(transaction.dust_generation_end_index as i64)
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

    save_created_unshielded_utxos(&transaction.created_unshielded_utxos, transaction_id, tx)
        .await?;
    save_spent_unshielded_utxos(&transaction.spent_unshielded_utxos, transaction_id, tx).await?;

    save_ledger_events(
        &transaction.ledger_events,
        &transaction.contract_actions,
        &contract_action_ids,
        transaction_id,
        tx,
    )
    .await?;

    save_dust_generation_info(&transaction.ledger_events, transaction_id, tx).await?;

    save_dust_nullifiers(&transaction.ledger_events, transaction_id, block_id, tx).await?;

    save_zswap_nullifiers(&transaction.ledger_events, transaction_id, block_id, tx).await?;

    // Persist a Cardano-bridge claim when the regular transaction is a `ClaimRewards` with
    // `ClaimKind::CardanoBridge` (populated by indexer-common's apply path).
    if let Some(claim) = &transaction.bridge_claim {
        save_bridge_claim(transaction_id, claim.recipient.as_ref(), claim.amount, tx).await?;
    }

    Ok(transaction_id as u64)
}

#[trace]
async fn save_system_transaction(
    transaction: &SystemTransaction,
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    save_created_unshielded_utxos(&transaction.created_unshielded_utxos, transaction_id, tx)
        .await?;

    save_ledger_events(&transaction.ledger_events, &[], &[], transaction_id, tx).await?;

    save_dust_generation_info(&transaction.ledger_events, transaction_id, tx).await
}

/// Save the contract actions and their balances, returning the freshly
/// assigned `contract_actions.id`s in insertion order so the caller can
/// correlate contract events with the emitting `ContractCall` (ticket #1162).
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
            zswap_state,
            attributes
        )
    "};

    let contract_action_ids = QueryBuilder::new(query)
        .push_values(contract_actions.iter(), |mut q, action| {
            q.push_bind(transaction_id)
                .push_bind(ContractActionVariant::from(&action.attributes))
                .push_bind(&action.address)
                .push_bind(&action.state)
                .push_bind(&action.zswap_state)
                .push_bind(Json(&action.attributes));
        })
        .push(" RETURNING id")
        .build_query_as::<(i64,)>()
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|(id,)| id)
        .collect::<Vec<_>>();

    let contract_balances = contract_actions
        .iter()
        .zip(&contract_action_ids)
        .flat_map(|(action, &action_id)| {
            action
                .extracted_balances
                .iter()
                .map(move |&balance| (action_id, balance))
        })
        .collect::<Vec<_>>();
    save_contract_balances(&contract_balances, tx).await?;

    Ok(contract_action_ids)
}

#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_created_unshielded_utxos(
    utxos: &[UnshieldedUtxo],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if utxos.is_empty() {
        return Ok(());
    }

    debug!(transaction_id, utxos:?; "saving created unshielded UTXOs");

    let query_base = indoc! {"
        INSERT INTO unshielded_utxos (
            creating_transaction_id,
            owner,
            token_type,
            value,
            intent_hash,
            output_index,
            ctime,
            initial_nonce,
            registered_for_dust_generation
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
                ctime,
                initial_nonce,
                registered_for_dust_generation,
            } = utxo;

            q.push_bind(transaction_id)
                .push_bind(owner.as_ref())
                .push_bind(token_type.as_ref())
                .push_bind(U128BeBytes::from(value))
                .push_bind(intent_hash.as_ref())
                .push_bind(*output_index as i64)
                .push_bind(ctime.map(|n| n as i64))
                .push_bind(initial_nonce.as_ref())
                .push_bind(registered_for_dust_generation);
        })
        .build()
        .execute(&mut **tx)
        .await?;

    Ok(())
}

#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_spent_unshielded_utxos(
    utxos: &[UnshieldedUtxo],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if utxos.is_empty() {
        return Ok(());
    }

    debug!(transaction_id, utxos:?; "saving spent unshielded UTXOs");

    let rows_affected;

    #[cfg(feature = "cloud")]
    {
        let query = indoc! {"
            UPDATE unshielded_utxos
            SET spending_transaction_id = $1
            WHERE (intent_hash, output_index) IN (
                SELECT * FROM UNNEST($2::BYTEA[], $3::BIGINT[])
            )
            AND spending_transaction_id IS NULL
        "};

        let (intent_hashes, output_indices) = utxos
            .iter()
            .map(|utxo| (utxo.intent_hash.as_ref(), utxo.output_index as i64))
            .unzip::<_, _, Vec<_>, Vec<_>>();

        rows_affected = sqlx::query(query)
            .bind(transaction_id)
            .bind(&intent_hashes)
            .bind(&output_indices)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    }

    #[cfg(feature = "standalone")]
    {
        let mut query = QueryBuilder::new(indoc! {"
            WITH pairs(intent_hash, output_index) AS (
        "});

        let (first, rest) = utxos.split_first().unwrap(); // utxos is non-empty!
        query
            .push("SELECT ")
            .push_bind(first.intent_hash.as_ref())
            .push(", ")
            .push_bind(first.output_index as i64);
        for utxo in rest {
            query
                .push(" UNION ALL SELECT ")
                .push_bind(utxo.intent_hash.as_ref())
                .push(", ")
                .push_bind(utxo.output_index as i64);
        }

        rows_affected = query
            .push(indoc! {"
                )
                UPDATE unshielded_utxos
                SET spending_transaction_id =
            "})
            .push_bind(transaction_id)
            .push(indoc! {"
                WHERE EXISTS (
                    SELECT 1
                    FROM pairs
                    WHERE intent_hash = unshielded_utxos.intent_hash
                    AND output_index = unshielded_utxos.output_index
                )
                AND spending_transaction_id IS NULL
            "})
            .build()
            .execute(&mut **tx)
            .await?
            .rows_affected();
    }

    if rows_affected != utxos.len() as u64 {
        return Err(sqlx::Error::Protocol(format!(
            "expected {} spent UTXOs but updated {rows_affected}: {utxos:?}",
            utxos.len()
        )));
    }

    Ok(())
}

/// Save the ledger events, correlating contract events with the emitting
/// `ContractCall` via `correlate_contract_action_ids` (ticket #1162).
/// `contract_actions` and `contract_action_ids` are the actions of the same
/// transaction with their freshly assigned ids; pass empty slices for
/// transactions without contract actions (e.g. system transactions).
#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_ledger_events(
    ledger_events: &[LedgerEvent],
    contract_actions: &[ContractAction],
    contract_action_ids: &[i64],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if ledger_events.is_empty() {
        return Ok(());
    }

    let correlated_action_ids =
        correlate_contract_action_ids(ledger_events, contract_actions, contract_action_ids);

    let query = indoc! {"
        INSERT INTO ledger_events (
            transaction_id,
            variant,
            grouping,
            raw,
            attributes,
            contract_address,
            contract_action_id
        )
    "};

    let mut qb = QueryBuilder::new(query);
    qb.push_values(
        ledger_events.iter().zip(&correlated_action_ids),
        |mut q, (ledger_event, &correlated_action_id)| {
            q.push_bind(transaction_id)
                .push_bind(LedgerEventVariant::from(&ledger_event.attributes))
                .push_bind(ledger_event.grouping)
                .push_bind(ledger_event.raw.as_ref())
                .push_bind(Json(&ledger_event.attributes))
                .push_bind(
                    ledger_event
                        .contract_address
                        .as_ref()
                        .map(|address| address.as_ref()),
                )
                .push_bind(
                    correlated_action_id
                        .or_else(|| ledger_event.contract_action_id.map(|id| id as i64)),
                );
        },
    );
    qb.push(" RETURNING id");

    // SQLite + Postgres both return RETURNING rows in the order the rows
    // were inserted (the multi-row INSERT is one statement so row order is
    // deterministic), so we can zip ids back onto ledger_events by index.
    let inserted_ids = qb.build_query_as::<(i64,)>().fetch_all(&mut **tx).await?;

    if inserted_ids.len() != ledger_events.len() {
        return Err(sqlx::Error::Protocol(format!(
            "save_ledger_events: expected {} RETURNING ids, was {}",
            ledger_events.len(),
            inserted_ids.len()
        )));
    }

    save_contract_event_indexed_fields(ledger_events, inserted_ids.iter().map(|(id,)| *id), tx)
        .await?;

    Ok(())
}

/// Populate the `contract_event_indexed_fields` sidecar for any contract
/// events in this batch. No-op for zswap/dust events. Called inside the same
/// transaction as `save_ledger_events` with the freshly captured RETURNING ids.
///
/// `ids` must be in the same order as `ledger_events`; the function pairs them
/// by index to associate each contract event with its persisted id.
#[trace]
async fn save_contract_event_indexed_fields<I: IntoIterator<Item = i64>>(
    ledger_events: &[LedgerEvent],
    ids: I,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    let pairs = ledger_events
        .iter()
        .zip(ids)
        .filter(|(ledger_event, _)| matches!(ledger_event.grouping, LedgerEventGrouping::Contract))
        .map(|(ledger_event, id)| (id, ledger_event.indexable_contract_fields()))
        .filter(|(_, fields)| !fields.is_empty())
        .collect::<Vec<_>>();

    if pairs.is_empty() {
        return Ok(());
    }

    // Multi-row insert across all contract events in the batch, one row per
    // (ledger_event_id, field_name, field_value).
    let mut qb = QueryBuilder::new(indoc! {"
        INSERT INTO contract_event_indexed_fields (
            ledger_event_id,
            field_name,
            field_value
        )
    "});
    qb.push_values(
        pairs
            .iter()
            .flat_map(|(id, fields)| fields.iter().map(move |field| (*id, field))),
        |mut q, (id, (name, value))| {
            q.push_bind(id).push_bind(*name).push_bind(value.as_ref());
        },
    );
    qb.build().execute(&mut **tx).await?;

    Ok(())
}

/// Pair each ledger event with the id of the emitting `ContractCall` by
/// matching `(contract_address, entry_point)` against the contract actions of
/// the same transaction (ticket #1162). Only an unambiguous match is
/// attributed: if several calls in the same transaction share address and
/// entry point, the events stay unattributed (`NULL`) rather than risking
/// wrong attribution; they remain reachable via the top-level `contractEvents`
/// query. Zswap and dust events map to `None`.
fn correlate_contract_action_ids(
    ledger_events: &[LedgerEvent],
    contract_actions: &[ContractAction],
    contract_action_ids: &[i64],
) -> Vec<Option<i64>> {
    ledger_events
        .iter()
        .map(|ledger_event| {
            let contract_address = ledger_event.contract_address.as_ref()?;
            let entry_point = ledger_event.attributes.contract_entry_point()?;

            let mut matches =
                contract_actions
                    .iter()
                    .zip(contract_action_ids)
                    .filter(|(action, _)| {
                        matches!(
                            &action.attributes,
                            ContractAttributes::Call { entry_point: action_entry_point }
                                if action_entry_point.as_bytes() == entry_point.as_ref()
                        ) && action.address == *contract_address
                    });

            let (_, &contract_action_id) = matches.next()?;
            matches.next().is_none().then_some(contract_action_id)
        })
        .collect()
}

#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_dust_generation_info(
    ledger_events: &[LedgerEvent],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if ledger_events.is_empty() {
        return Ok(());
    }

    for ledger_event in ledger_events {
        match &ledger_event.attributes {
            LedgerEventAttributes::DustInitialUtxo {
                output,
                generation_info,
                generation_index,
            } => {
                let query = indoc! {"
                    INSERT INTO dust_generation_info (
                        night_utxo_hash,
                        value,
                        owner,
                        nonce,
                        ctime,
                        merkle_index,
                        generation_index,
                        backing_night,
                        initial_value,
                        dtime,
                        transaction_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "};

                let dtime = if generation_info.dtime == u64::MAX {
                    None
                } else {
                    Some(generation_info.dtime as i64)
                };

                sqlx::query(query)
                    .bind(generation_info.night_utxo_hash.as_ref())
                    .bind(U128BeBytes::from(&generation_info.value))
                    .bind(generation_info.owner.as_ref())
                    .bind(generation_info.nonce.as_ref())
                    .bind(generation_info.ctime as i64)
                    .bind(output.mt_index as i64)
                    .bind(*generation_index as i64)
                    .bind(output.backing_night.as_ref())
                    .bind(U128BeBytes::from(&output.initial_value))
                    .bind(dtime)
                    .bind(transaction_id)
                    .execute(&mut **tx)
                    .await?;
            }

            LedgerEventAttributes::DustGenerationDtimeUpdate {
                generation_info, ..
            } => {
                let query = indoc! {"
                    UPDATE dust_generation_info
                    SET dtime = $1
                    WHERE night_utxo_hash = $2
                "};

                sqlx::query(query)
                    .bind(generation_info.dtime as i64)
                    .bind(generation_info.night_utxo_hash.as_ref())
                    .execute(&mut **tx)
                    .await?;
            }

            // Other event types (ZswapInput, ZswapOutput, ParamChange, DustSpendProcessed)
            // are not relevant to dust_generation_info table.
            _ => {}
        }
    }

    Ok(())
}

#[trace(properties = { "transaction_id": "{transaction_id}", "block_id": "{block_id}" })]
async fn save_dust_nullifiers(
    ledger_events: &[LedgerEvent],
    transaction_id: i64,
    block_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    let nullifier_events = ledger_events
        .iter()
        .filter_map(|event| match &event.attributes {
            LedgerEventAttributes::DustSpendProcessed {
                nullifier,
                commitment,
            } => Some((nullifier, commitment)),

            _ => None,
        })
        .collect::<Vec<_>>();

    if nullifier_events.is_empty() {
        return Ok(());
    }

    let query = indoc! {"
        INSERT INTO dust_nullifiers (
            nullifier,
            commitment,
            transaction_id,
            block_id
        )
    "};

    QueryBuilder::new(query)
        .push_values(nullifier_events, |mut q, (nullifier, commitment)| {
            q.push_bind(nullifier.as_ref())
                .push_bind(commitment.as_ref())
                .push_bind(transaction_id)
                .push_bind(block_id);
        })
        .build()
        .execute(&mut **tx)
        .await?;

    Ok(())
}

#[trace(properties = { "transaction_id": "{transaction_id}", "block_id": "{block_id}" })]
async fn save_zswap_nullifiers(
    ledger_events: &[LedgerEvent],
    transaction_id: i64,
    block_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    let nullifier_events = ledger_events
        .iter()
        .filter_map(|event| match &event.attributes {
            LedgerEventAttributes::ZswapInput { nullifier } => Some(nullifier),

            _ => None,
        })
        .collect::<Vec<_>>();

    if nullifier_events.is_empty() {
        return Ok(());
    }

    let query = indoc! {"
        INSERT INTO zswap_nullifiers (
            transaction_id,
            block_id,
            nullifier
        )
    "};

    QueryBuilder::new(query)
        .push_values(nullifier_events, |mut q, nullifier| {
            q.push_bind(transaction_id)
                .push_bind(block_id)
                .push_bind(nullifier.as_ref());
        })
        .build()
        .execute(&mut **tx)
        .await?;

    Ok(())
}

#[trace]
async fn save_contract_balances(
    balances: &[(i64, ContractBalance)],
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if balances.is_empty() {
        return Ok(());
    }

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

    Ok(())
}

#[cfg(feature = "standalone")]
async fn save_identifiers(
    identifiers: &[indexer_common::domain::SerializedTransactionIdentifier],
    transaction_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if identifiers.is_empty() {
        return Ok(());
    }

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

    Ok(())
}

#[trace]
async fn save_dust_registration_events(
    events: &[DustRegistrationEvent],
    block_id: i64,
    block_timestamp: u64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    for event in events {
        match event {
            DustRegistrationEvent::Registration {
                cardano_stake_key,
                dust_address,
            } => {
                let query = indoc! {"
                    INSERT INTO cnight_registrations (
                        cardano_stake_key,
                        dust_address,
                        valid,
                        registered_at,
                        block_id
                    )
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT (cardano_stake_key, dust_address)
                    DO UPDATE SET
                        valid = EXCLUDED.valid,
                        registered_at = EXCLUDED.registered_at,
                        removed_at = NULL,
                        block_id = EXCLUDED.block_id
                "};

                sqlx::query(query)
                    .bind(cardano_stake_key.as_ref())
                    .bind(dust_address.as_ref())
                    .bind(true)
                    .bind(block_timestamp as i64)
                    .bind(block_id)
                    .execute(&mut **tx)
                    .await?;
            }

            DustRegistrationEvent::Deregistration {
                cardano_stake_key,
                dust_address,
            } => {
                let query = indoc! {"
                    UPDATE cnight_registrations
                    SET valid = $1,
                        removed_at = $2,
                        block_id = $3
                    WHERE cardano_stake_key = $4
                    AND dust_address = $5
                "};

                sqlx::query(query)
                    .bind(false)
                    .bind(block_timestamp as i64)
                    .bind(block_id)
                    .bind(cardano_stake_key.as_ref())
                    .bind(dust_address.as_ref())
                    .execute(&mut **tx)
                    .await?;
            }

            DustRegistrationEvent::MappingAdded {
                cardano_stake_key,
                dust_address,
                utxo_id,
                utxo_index,
            } => {
                let query = indoc! {"
                    UPDATE cnight_registrations
                    SET utxo_tx_hash = $1,
                        utxo_output_index = $2
                    WHERE cardano_stake_key = $3
                    AND dust_address = $4
                "};

                sqlx::query(query)
                    .bind(utxo_id.as_ref())
                    .bind(*utxo_index as i64)
                    .bind(cardano_stake_key.as_ref())
                    .bind(dust_address.as_ref())
                    .execute(&mut **tx)
                    .await?;
            }

            DustRegistrationEvent::MappingRemoved {
                cardano_stake_key,
                dust_address,
                ..
            } => {
                let query = indoc! {"
                    UPDATE cnight_registrations
                    SET utxo_tx_hash = NULL,
                        utxo_output_index = NULL
                    WHERE cardano_stake_key = $1
                    AND dust_address = $2
                "};

                sqlx::query(query)
                    .bind(cardano_stake_key.as_ref())
                    .bind(dust_address.as_ref())
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }

    Ok(())
}

#[trace(properties = { "change": "{change:?}" })]
async fn save_system_parameters_change(
    change: &SystemParametersChange,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    if let Some(ref d_parameter) = change.d_parameter {
        let query = indoc! {"
            INSERT INTO system_parameters_d (
                block_height,
                block_hash,
                timestamp,
                num_permissioned_candidates,
                num_registered_candidates
            )
            VALUES ($1, $2, $3, $4, $5)
        "};

        sqlx::query(query)
            .bind(change.block_height as i64)
            .bind(change.block_hash.as_ref())
            .bind(change.timestamp as i64)
            .bind(d_parameter.num_permissioned_candidates as i32)
            .bind(d_parameter.num_registered_candidates as i32)
            .execute(&mut **tx)
            .await?;
    }

    if let Some(ref terms_and_conditions) = change.terms_and_conditions {
        let query = indoc! {"
            INSERT INTO system_parameters_terms_and_conditions (
                block_height,
                block_hash,
                timestamp,
                hash,
                url
            )
            VALUES ($1, $2, $3, $4, $5)
        "};

        sqlx::query(query)
            .bind(change.block_height as i64)
            .bind(change.block_hash.as_ref())
            .bind(change.timestamp as i64)
            .bind(terms_and_conditions.hash.as_ref())
            .bind(&terms_and_conditions.url)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

/// Save a bridge claim parsed from a regular `ClaimRewardsTransaction` with
/// `ClaimKind::CardanoBridge`, as populated on the transaction by the indexer-common apply path
/// and persisted from `save_regular_transaction`.
#[trace(properties = { "transaction_id": "{transaction_id}" })]
async fn save_bridge_claim(
    transaction_id: i64,
    recipient: &[u8],
    amount: u128,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    let query = indoc! {"
        INSERT INTO bridge_claims (transaction_id, recipient, amount)
        VALUES ($1, $2, $3)
    "};

    sqlx::query(query)
        .bind(transaction_id)
        .bind(recipient)
        .bind(U128BeBytes::from(amount))
        .execute(&mut **tx)
        .await?;

    Ok(())
}

#[trace(properties = { "block_id": "{block_id}" })]
async fn save_bridge_events(
    events: &[BridgeEvent],
    block_id: i64,
    tx: &mut SqlxTransaction,
) -> Result<(), sqlx::Error> {
    for event in events {
        let query = indoc! {"
            INSERT INTO protocol_bridge_events (
                block_id,
                transaction_id,
                variant,
                mc_tx_hash,
                amount,
                recipient,
                midnight_tx_hash,
                count
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "};

        let amount = event.amount().to_be_bytes().to_vec();
        let count = match event {
            BridgeEvent::SubminimalFlushTransfer { count, .. } => Some(*count as i32),
            _ => None,
        };

        // Link to the system transaction the handler produced: its hash equals the event's
        // `midnight_tx_hash`, saved in this same DB transaction by `save_transactions` above.
        let link_query = indoc! {"
            SELECT id
            FROM transactions
            WHERE block_id = $1 AND hash = $2
        "};
        let transaction_id = sqlx::query_as::<_, (i64,)>(link_query)
            .bind(block_id)
            .bind(event.midnight_tx_hash().as_ref())
            .fetch_optional(&mut **tx)
            .await?
            .map(|(id,)| id);

        sqlx::query(query)
            .bind(block_id)
            .bind(transaction_id)
            .bind(event.variant())
            .bind(event.mc_tx_hash().map(|h| h.as_ref()))
            .bind(amount)
            .bind(event.recipient().map(|r| r.as_bytes()))
            .bind(event.midnight_tx_hash().as_ref())
            .bind(count)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod contract_event_variant_tests {
    use super::*;
    use indexer_common::domain::{AddressOrContract, ByteVec};

    fn bv(bytes: &[u8]) -> ByteVec {
        ByteVec::from(bytes.to_vec())
    }

    // The From impl maps each ContractEvent attribute variant to the matching
    // SQL LedgerEventVariant. A missing arm would fail the match exhaustively;
    // this test pins the mapping so renames don't silently shift names.
    #[test]
    fn from_attributes_to_variant_covers_every_contract_event_variant() {
        let cases: Vec<(LedgerEventAttributes, LedgerEventVariant)> = vec![
            (
                LedgerEventAttributes::ContractShieldedSpend {
                    version: 1,
                    entry_point: bv(b""),
                    nullifier: bv(&[0; 32]),
                },
                LedgerEventVariant::ShieldedSpend,
            ),
            (
                LedgerEventAttributes::ContractShieldedReceive {
                    version: 1,
                    entry_point: bv(b""),
                    commitment: bv(&[0; 32]),
                    ciphertext: None,
                    receiving_contract_address: None,
                },
                LedgerEventVariant::ShieldedReceive,
            ),
            (
                LedgerEventAttributes::ContractShieldedMint {
                    version: 1,
                    entry_point: bv(b""),
                    commitment: bv(&[0; 32]),
                    domain_sep: bv(&[0; 32]),
                    amount: None,
                },
                LedgerEventVariant::ShieldedMint,
            ),
            (
                LedgerEventAttributes::ContractShieldedBurn {
                    version: 1,
                    entry_point: bv(b""),
                    nullifier: bv(&[0; 32]),
                    amount: None,
                },
                LedgerEventVariant::ShieldedBurn,
            ),
            (
                LedgerEventAttributes::ContractUnshieldedSpend {
                    version: 1,
                    entry_point: bv(b""),
                    sender: AddressOrContract::User(bv(&[0; 32])),
                    domain_sep: bv(&[0; 32]),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                LedgerEventVariant::UnshieldedSpend,
            ),
            (
                LedgerEventAttributes::ContractUnshieldedReceive {
                    version: 1,
                    entry_point: bv(b""),
                    recipient: AddressOrContract::Contract(bv(&[0; 32])),
                    domain_sep: bv(&[0; 32]),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                LedgerEventVariant::UnshieldedReceive,
            ),
            (
                LedgerEventAttributes::ContractUnshieldedMint {
                    version: 1,
                    entry_point: bv(b""),
                    domain_sep: bv(&[0; 32]),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                LedgerEventVariant::UnshieldedMint,
            ),
            (
                LedgerEventAttributes::ContractUnshieldedBurn {
                    version: 1,
                    entry_point: bv(b""),
                    sender: AddressOrContract::User(bv(&[0; 32])),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                LedgerEventVariant::UnshieldedBurn,
            ),
            (
                LedgerEventAttributes::ContractPaused {
                    version: 1,
                    entry_point: bv(b""),
                },
                LedgerEventVariant::Paused,
            ),
            (
                LedgerEventAttributes::ContractUnpaused {
                    version: 1,
                    entry_point: bv(b""),
                },
                LedgerEventVariant::Unpaused,
            ),
            (
                LedgerEventAttributes::ContractMisc {
                    version: 1,
                    entry_point: bv(b""),
                    name: bv(&[0; 32]),
                    payload: bv(&[0; 32]),
                },
                LedgerEventVariant::Misc,
            ),
        ];

        for (attrs, expected) in cases {
            let got = LedgerEventVariant::from(&attrs);
            assert_eq!(got, expected, "wrong variant for {:?}", attrs);
        }
    }

    // Sanity: existing zswap/dust mappings still work after the new arms.
    #[test]
    fn from_attributes_existing_variants_still_mapped() {
        let zi = LedgerEventVariant::from(&LedgerEventAttributes::ZswapInput {
            nullifier: bv(&[0; 32]),
        });
        let zo = LedgerEventVariant::from(&LedgerEventAttributes::ZswapOutput);
        let pc = LedgerEventVariant::from(&LedgerEventAttributes::ParamChange);
        let dsp = LedgerEventVariant::from(&LedgerEventAttributes::DustSpendProcessed {
            nullifier: bv(&[0; 32]),
            commitment: bv(&[0; 32]),
        });

        assert_eq!(zi, LedgerEventVariant::ZswapInput);
        assert_eq!(zo, LedgerEventVariant::ZswapOutput);
        assert_eq!(pc, LedgerEventVariant::ParamChange);
        assert_eq!(dsp, LedgerEventVariant::DustSpendProcessed);
    }
}

#[cfg(test)]
mod contract_event_correlation_tests {
    use super::*;
    use indexer_common::domain::ByteVec;

    fn bv(bytes: &[u8]) -> ByteVec {
        ByteVec::from(bytes.to_vec())
    }

    fn call_action(address: &[u8], entry_point: &str) -> ContractAction {
        ContractAction {
            address: bv(address),
            state: bv(b""),
            zswap_state: bv(b""),
            extracted_balances: vec![],
            attributes: ContractAttributes::Call {
                entry_point: entry_point.to_string(),
            },
        }
    }

    fn deploy_action(address: &[u8]) -> ContractAction {
        ContractAction {
            address: bv(address),
            state: bv(b""),
            zswap_state: bv(b""),
            extracted_balances: vec![],
            attributes: ContractAttributes::Deploy,
        }
    }

    fn contract_event(address: &[u8], entry_point: &[u8]) -> LedgerEvent {
        LedgerEvent::contract_event(
            bv(b"raw"),
            bv(address),
            None,
            LedgerEventAttributes::ContractShieldedSpend {
                version: 1,
                entry_point: bv(entry_point),
                nullifier: bv(&[0; 32]),
            },
        )
    }

    #[test]
    fn correlates_event_to_single_matching_call() {
        let actions = vec![
            call_action(&[0x01; 32], "foo"),
            call_action(&[0x02; 32], "bar"),
        ];
        let events = vec![
            contract_event(&[0x01; 32], b"foo"),
            contract_event(&[0x02; 32], b"bar"),
        ];

        let correlated = correlate_contract_action_ids(&events, &actions, &[10, 20]);
        assert_eq!(correlated, vec![Some(10), Some(20)]);
    }

    #[test]
    fn leaves_ambiguous_match_unattributed() {
        // Two calls to the same contract address and entry point in one
        // transaction: attribution would be a guess, so it stays None.
        let actions = vec![
            call_action(&[0x01; 32], "foo"),
            call_action(&[0x01; 32], "foo"),
        ];
        let events = vec![contract_event(&[0x01; 32], b"foo")];

        let correlated = correlate_contract_action_ids(&events, &actions, &[10, 20]);
        assert_eq!(correlated, vec![None]);
    }

    #[test]
    fn same_address_different_entry_points_disambiguate() {
        let actions = vec![
            call_action(&[0x01; 32], "foo"),
            call_action(&[0x01; 32], "bar"),
        ];
        let events = vec![
            contract_event(&[0x01; 32], b"bar"),
            contract_event(&[0x01; 32], b"foo"),
        ];

        let correlated = correlate_contract_action_ids(&events, &actions, &[10, 20]);
        assert_eq!(correlated, vec![Some(20), Some(10)]);
    }

    #[test]
    fn ignores_deploy_actions_and_unmatched_addresses() {
        let actions = vec![deploy_action(&[0x01; 32])];
        let events = vec![
            contract_event(&[0x01; 32], b"foo"),
            contract_event(&[0x03; 32], b"foo"),
        ];

        let correlated = correlate_contract_action_ids(&events, &actions, &[10]);
        assert_eq!(correlated, vec![None, None]);
    }

    #[test]
    fn zswap_and_dust_events_stay_unattributed() {
        let actions = vec![call_action(&[0x01; 32], "foo")];
        let events = vec![
            LedgerEvent {
                grouping: LedgerEventGrouping::Zswap,
                raw: bv(b"raw"),
                attributes: LedgerEventAttributes::ZswapInput {
                    nullifier: bv(&[0; 32]),
                },
                contract_action_id: None,
                contract_address: None,
            },
            LedgerEvent {
                grouping: LedgerEventGrouping::Dust,
                raw: bv(b"raw"),
                attributes: LedgerEventAttributes::ParamChange,
                contract_action_id: None,
                contract_address: None,
            },
        ];

        let correlated = correlate_contract_action_ids(&events, &actions, &[10]);
        assert_eq!(correlated, vec![None, None]);
    }
}

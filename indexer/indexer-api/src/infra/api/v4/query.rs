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
        LedgerStateCacheError,
        bridge::TreasuryReason,
        storage::{Storage, bridge::BridgeEventFilter},
    },
    infra::api::{
        ApiError, ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{
            CardanoNetworkId, CardanoRewardAddress, HexEncoded,
            block::{Block, BlockOffset},
            bridge::{
                BridgeBalance, BridgeEvent, BridgeEventVariant, BridgePoolSummary,
                BridgeTreasuryReason,
            },
            contract::Contract,
            contract_action::{ContractAction, ContractActionOffset},
            contract_event::{ContractEvent, ContractEventFilter},
            directives::beta,
            dust::DustGenerationStatus,
            dust_generations::DustGenerations,
            merkle_tree_collapsed_update::MerkleTreeCollapsedUpdate,
            spo::{
                CommitteeMember, EpochInfo, EpochPerf, FirstValidEpoch, PoolMetadata,
                PresenceEvent, RegisteredStat, RegisteredTotals, Spo, SpoComposite, SpoIdentity,
                StakeShare,
            },
            system_parameters::{DParameterChange, TermsAndConditionsChange},
            transaction::{Transaction, TransactionOffset},
        },
    },
};
use async_graphql::{Context, Object};
use fastrace::trace;
use indexer_common::domain::{LedgerVersion, UnshieldedAddress, ledger};
use std::marker::PhantomData;

const DEFAULT_PERFORMANCE_LIMIT: i64 = 20;

/// GraphQL queries.
pub struct Query<S> {
    _s: PhantomData<S>,
}

impl<S> Default for Query<S> {
    fn default() -> Self {
        Self { _s: PhantomData }
    }
}

#[Object]
impl<S> Query<S>
where
    S: Storage,
{
    /// Find a block for the given optional offset; if not present, the latest block is returned.
    #[trace(properties = { "offset": "{offset:?}" })]
    async fn block(
        &self,
        cx: &Context<'_>,
        offset: Option<BlockOffset>,
    ) -> ApiResult<Option<Block<S>>> {
        let block = match offset {
            Some(BlockOffset::Hash(hash)) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid block hash")?;

                cx.get_block_by_hash_loader::<S>()
                    .load_one(hash)
                    .await
                    .map_err_into_server_error(|| format!("get block by hash {hash}"))?
            }

            Some(BlockOffset::Height(height)) => cx
                .get_storage::<S>()
                .get_block_by_height(height)
                .await
                .map_err_into_server_error(|| format!("get block by height {height}"))?,

            None => cx
                .get_storage::<S>()
                .get_latest_block()
                .await
                .map_err_into_server_error(|| "get latest block")?,
        };

        Ok(block.map(Into::into))
    }

    /// Get a Merkle tree collapsed update for the given zswap state index range.
    #[trace(properties = { "start_index": "{start_index}", "end_index": "{end_index}" })]
    async fn zswap_merkle_tree_collapsed_update(
        &self,
        cx: &Context<'_>,
        start_index: u64,
        end_index: u64,
    ) -> ApiResult<MerkleTreeCollapsedUpdate> {
        let storage = cx.get_storage::<S>();

        let (protocol_version, _) = storage
            .get_highest_ledger_state()
            .await
            .map_err_into_server_error(|| "get highest ledger state")?
            .some_or_server_error(|| "no ledger state available")?;

        cx.get_ledger_state_cache()
            .make_zswap_collapsed_update(start_index, end_index, storage, protocol_version)
            .await
            .map_err(|error| match error {
                error @ LedgerStateCacheError::Ledger(ledger::Error::InvalidUpdate(_)) => {
                    ApiError::client("invalid start_index and/or end_index", error)
                }

                error => ApiError::server("create zswap Merkle tree collapsed update", error),
            })
            .map(MerkleTreeCollapsedUpdate::from)
    }

    /// Find transactions for the given offset.
    #[trace(properties = { "offset": "{offset:?}" })]
    async fn transactions(
        &self,
        cx: &Context<'_>,
        offset: TransactionOffset,
    ) -> ApiResult<Vec<Transaction<S>>> {
        let storage = cx.get_storage::<S>();

        match offset {
            TransactionOffset::Hash(hash) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid transaction hash")?;

                let transactions = storage
                    .get_transactions_by_hash(hash)
                    .await
                    .map_err_into_server_error(|| format!("get transactions by hash {hash}"))?
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>();

                Ok(transactions)
            }

            TransactionOffset::Identifier(identifier) => {
                let identifier = identifier
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid transaction identifier")?;

                let transactions = storage
                    .get_transactions_by_identifier(&identifier)
                    .await
                    .map_err_into_server_error(|| {
                        format!("get transactions by identifier {identifier}")
                    })?
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>();

                Ok(transactions)
            }
        }
    }

    /// Find a contract action for the given address and optional offset.
    #[trace(properties = { "address": "{address}", "offset": "{offset:?}" })]
    async fn contract_action(
        &self,
        cx: &Context<'_>,
        address: HexEncoded,
        offset: Option<ContractActionOffset>,
    ) -> ApiResult<Option<ContractAction<S>>> {
        let storage = cx.get_storage::<S>();

        let address = &address
            .hex_decode()
            .map_err_into_client_error(|| "invalid address")?;

        let contract_action = match offset {
            Some(ContractActionOffset::BlockOffset(BlockOffset::Hash(hash))) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid offset")?;

                storage
                    .get_contract_action_by_address_and_block_hash(address, hash)
                    .await
                    .map_err_into_server_error(|| {
                        format!("get contract action by address {address} and block hash {hash}")
                    })?
            }

            Some(ContractActionOffset::BlockOffset(BlockOffset::Height(height))) => storage
                .get_contract_action_by_address_and_block_height(address, height)
                .await
                .map_err_into_server_error(|| {
                    format!("get contract action by address {address} and block height {height}")
                })?,

            Some(ContractActionOffset::TransactionOffset(TransactionOffset::Hash(hash))) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid offset")?;

                storage
                    .get_contract_action_by_address_and_transaction_hash(address, hash)
                    .await
                    .map_err_into_server_error(|| {
                        format!(
                            "get contract action by address {address} and transaction hash {hash}"
                        )
                    })?
            }

            Some(ContractActionOffset::TransactionOffset(TransactionOffset::Identifier(
                identifier,
            ))) => {
                let identifier = identifier
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid identifier")?;

                storage
                    .get_contract_action_by_address_and_transaction_identifier(
                        address,
                        &identifier,
                    )
                    .await
                    .map_err_into_server_error(|| format!("get contract action by address {address} and transaction identifier {identifier}"))?
            }

            None => storage
                .get_latest_contract_action_by_address(address)
                .await
                .map_err_into_server_error(|| {
                    format!("get latest contract action by address {address}")
                })?,
        };

        Ok(contract_action.map(Into::into))
    }

    /// Find a contract by address, resolved as of the given block offset (or its latest state if no
    /// offset is given). Returns null if the contract has no action at or before that block.
    #[graphql(directive = beta::apply())]
    #[trace(properties = { "address": "{address}", "offset": "{offset:?}" })]
    async fn contract(
        &self,
        cx: &Context<'_>,
        address: HexEncoded,
        offset: Option<BlockOffset>,
    ) -> ApiResult<Option<Contract<S>>> {
        let storage = cx.get_storage::<S>();

        let address = &address
            .hex_decode()
            .map_err_into_client_error(|| "invalid address")?;

        let contract_action = match offset {
            Some(BlockOffset::Hash(hash)) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid offset")?;

                storage
                    .get_contract_action_by_address_as_of_block_hash(address, hash)
                    .await
                    .map_err_into_server_error(|| {
                        format!("get contract by address {address} as of block hash {hash}")
                    })?
            }

            Some(BlockOffset::Height(height)) => storage
                .get_contract_action_by_address_as_of_block_height(address, height)
                .await
                .map_err_into_server_error(|| {
                    format!("get contract by address {address} as of block height {height}")
                })?,

            None => storage
                .get_latest_contract_action_by_address(address)
                .await
                .map_err_into_server_error(|| {
                    format!("get latest contract action by address {address}")
                })?,
        };

        Ok(contract_action.map(Into::into))
    }

    /// Get DUST generation status for specific Cardano reward addresses.
    #[trace]
    async fn dust_generation_status(
        &self,
        cx: &Context<'_>,
        cardano_reward_addresses: Vec<CardanoRewardAddress>,
    ) -> ApiResult<Vec<DustGenerationStatus>> {
        // DOS protection: limit to 10 reward addresses.
        (cardano_reward_addresses.len() <= 10)
            .then_some(())
            .some_or_client_error(|| "maximum of ten reward addresses allowed")?;

        let storage = cx.get_storage::<S>();
        let network_id = cx.get_network_id();
        let expected_cardano_network = CardanoNetworkId::from(network_id);

        // Convert Bech32 CardanoRewardAddress to binary, validating network.
        let address = cardano_reward_addresses
            .into_iter()
            .map(|key| key.decode_for_network(expected_cardano_network))
            .collect::<Result<Vec<_>, _>>()
            .map_err_into_client_error(|| "invalid Cardano reward address")?;

        let status_list = storage
            .get_dust_generation_status(&address, LedgerVersion::LATEST)
            .await
            .map_err_into_server_error(|| "get DUST generation status")?;

        Ok(status_list
            .into_iter()
            .map(|s| (s, network_id).into())
            .collect())
    }

    /// Get all active DUST registrations and aggregated generation stats for Cardano reward
    /// addresses.
    #[trace]
    async fn dust_generations(
        &self,
        cx: &Context<'_>,
        cardano_reward_addresses: Vec<CardanoRewardAddress>,
    ) -> ApiResult<Vec<DustGenerations>> {
        (cardano_reward_addresses.len() <= 10)
            .then_some(())
            .some_or_client_error(|| "maximum of ten reward addresses allowed")?;

        let storage = cx.get_storage::<S>();
        let network_id = cx.get_network_id();
        let expected_cardano_network = CardanoNetworkId::from(network_id);

        let addresses = cardano_reward_addresses
            .into_iter()
            .map(|key| key.decode_for_network(expected_cardano_network))
            .collect::<Result<Vec<_>, _>>()
            .map_err_into_client_error(|| "invalid Cardano reward address")?;

        let data = storage
            .get_dust_generations(&addresses, LedgerVersion::LATEST)
            .await
            .map_err_into_server_error(|| "get DUST generations")?;

        Ok(data
            .into_iter()
            .map(|d| DustGenerations::from_domain(d, network_id))
            .collect())
    }

    /// Get a collapsed Merkle tree update for the dust commitment tree.
    #[trace(properties = { "start_index": "{start_index}", "end_index": "{end_index}" })]
    #[graphql(directive = beta::apply())]
    async fn dust_commitment_merkle_tree_update(
        &self,
        cx: &Context<'_>,
        start_index: u64,
        end_index: u64,
    ) -> ApiResult<MerkleTreeCollapsedUpdate> {
        let storage = cx.get_storage::<S>();

        let (protocol_version, _) = storage
            .get_highest_ledger_state()
            .await
            .map_err_into_server_error(|| "get highest ledger state")?
            .some_or_server_error(|| "no ledger state available")?;

        cx.get_ledger_state_cache()
            .dust_commitments_collapsed_update(start_index, end_index, storage, protocol_version)
            .await
            .map_err(|error| match error {
                error @ LedgerStateCacheError::Ledger(ledger::Error::InvalidUpdate(_)) => {
                    ApiError::client("invalid start_index and/or end_index", error)
                }

                error => ApiError::server("create dust commitment collapsed update", error),
            })
            .map(MerkleTreeCollapsedUpdate::from)
    }

    /// Get a collapsed Merkle tree update for the dust generation tree.
    #[trace(properties = { "start_index": "{start_index}", "end_index": "{end_index}" })]
    #[graphql(directive = beta::apply())]
    async fn dust_generation_merkle_tree_update(
        &self,
        cx: &Context<'_>,
        start_index: u64,
        end_index: u64,
    ) -> ApiResult<MerkleTreeCollapsedUpdate> {
        let storage = cx.get_storage::<S>();

        let (protocol_version, _) = storage
            .get_highest_ledger_state()
            .await
            .map_err_into_server_error(|| "get highest ledger state")?
            .some_or_server_error(|| "no ledger state available")?;

        cx.get_ledger_state_cache()
            .dust_generations_collapsed_update(start_index, end_index, storage, protocol_version)
            .await
            .map_err(|error| match error {
                error @ LedgerStateCacheError::Ledger(ledger::Error::InvalidUpdate(_)) => {
                    ApiError::client("invalid start_index and/or end_index", error)
                }

                error => ApiError::server("create dust generation collapsed update", error),
            })
            .map(MerkleTreeCollapsedUpdate::from)
    }

    /// Find contract events matching the filter, ordered by ID; `limit` defaults to 100 and is
    /// capped at 500, `offset` defaults to 0.
    ///
    /// Block-range bounds (`fromBlock`, `toBlock`) live on `ContractEventFilter`
    /// for symmetry with the subscription. `limit`/`offset` are top-level args.
    #[graphql(directive = beta::apply())]
    #[trace(properties = { "limit": "{limit:?}", "offset": "{offset:?}" })]
    async fn contract_events(
        &self,
        cx: &Context<'_>,
        filter: ContractEventFilter,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ApiResult<Vec<ContractEvent<S>>> {
        let storage = cx.get_storage::<S>();

        let filter = filter
            .into_domain()
            .map_err(|error| ApiError::client("invalid contract event filter", error))?;

        let limit = limit.unwrap_or(100).clamp(1, 500) as u32;
        let offset = offset.unwrap_or(0).max(0) as u32;

        let rows = storage
            .get_contract_events(&filter, limit, offset)
            .await
            .map_err_into_server_error(|| "get contract events")?;

        rows.into_iter()
            .map(ContractEvent::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err_into_server_error(|| "convert contract event row to GraphQL type")
    }

    /// Get the full history of D-parameter changes for governance auditability.
    #[trace]
    async fn d_parameter_history(&self, cx: &Context<'_>) -> ApiResult<Vec<DParameterChange>> {
        let storage = cx.get_storage::<S>();

        let history = storage
            .get_d_parameter_history()
            .await
            .map_err_into_server_error(|| "get D-parameter history")?;

        Ok(history.into_iter().map(DParameterChange::from).collect())
    }

    /// Get the full history of Terms and Conditions changes for governance auditability.
    #[trace]
    async fn terms_and_conditions_history(
        &self,
        cx: &Context<'_>,
    ) -> ApiResult<Vec<TermsAndConditionsChange>> {
        let storage = cx.get_storage::<S>();

        let history = storage
            .get_terms_and_conditions_history()
            .await
            .map_err_into_server_error(|| "get Terms and Conditions history")?;

        Ok(history
            .into_iter()
            .map(TermsAndConditionsChange::from)
            .collect())
    }

    /// List SPO identities with pagination.
    #[trace]
    async fn spo_identities(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ApiResult<Vec<SpoIdentity>> {
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(50).clamp(1, 500) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;

        let identities = storage
            .get_spo_identities(limit, offset)
            .await
            .map_err_into_server_error(|| "get SPO identities")?;

        Ok(identities.into_iter().map(Into::into).collect())
    }

    /// Get SPO identity by pool ID.
    #[trace]
    async fn spo_identity_by_pool_id(
        &self,
        cx: &Context<'_>,
        pool_id_hex: String,
    ) -> ApiResult<Option<SpoIdentity>> {
        let pool_id = normalize_hex(&pool_id_hex);
        let storage = cx.get_storage::<S>();

        let identity = storage
            .get_spo_identity_by_pool_id(&pool_id)
            .await
            .map_err_into_server_error(|| "get SPO identity by pool ID")?;

        Ok(identity.map(Into::into))
    }

    /// Get total count of SPOs.
    #[trace]
    async fn spo_count(&self, cx: &Context<'_>) -> ApiResult<Option<i64>> {
        let storage = cx.get_storage::<S>();

        let count = storage
            .get_spo_count()
            .await
            .map_err_into_server_error(|| "get SPO count")?;

        Ok(Some(count))
    }

    /// Get pool metadata by pool ID.
    #[trace]
    async fn pool_metadata(
        &self,
        cx: &Context<'_>,
        pool_id_hex: String,
    ) -> ApiResult<Option<PoolMetadata>> {
        let pool_id = normalize_hex(&pool_id_hex);
        let storage = cx.get_storage::<S>();

        let metadata = storage
            .get_pool_metadata(&pool_id)
            .await
            .map_err_into_server_error(|| "get pool metadata")?;

        Ok(metadata.map(Into::into))
    }

    /// List pool metadata with pagination.
    #[trace]
    async fn pool_metadata_list(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
        with_name_only: Option<bool>,
    ) -> ApiResult<Vec<PoolMetadata>> {
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(50).clamp(1, 500) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;
        let with_name_only = with_name_only.unwrap_or(false);

        let metadata = storage
            .get_pool_metadata_list(limit, offset, with_name_only)
            .await
            .map_err_into_server_error(|| "get pool metadata list")?;

        Ok(metadata.into_iter().map(Into::into).collect())
    }

    /// Get SPO with metadata by pool ID.
    #[trace]
    async fn spo_by_pool_id(
        &self,
        cx: &Context<'_>,
        pool_id_hex: String,
    ) -> ApiResult<Option<Spo>> {
        let pool_id = normalize_hex(&pool_id_hex);
        let storage = cx.get_storage::<S>();

        let spo = storage
            .get_spo_by_pool_id(&pool_id)
            .await
            .map_err_into_server_error(|| "get SPO by pool ID")?;

        Ok(spo.map(Into::into))
    }

    /// List SPOs with optional search.
    #[trace]
    async fn spo_list(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
        search: Option<String>,
    ) -> ApiResult<Vec<Spo>> {
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(20).clamp(1, 200) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;
        let search_ref = search.as_deref().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        let spos = storage
            .get_spo_list(limit, offset, search_ref)
            .await
            .map_err_into_server_error(|| "get SPO list")?;

        Ok(spos.into_iter().map(Into::into).collect())
    }

    /// Get composite SPO data (identity + metadata + performance).
    #[trace]
    async fn spo_composite_by_pool_id(
        &self,
        cx: &Context<'_>,
        pool_id_hex: String,
    ) -> ApiResult<Option<SpoComposite>> {
        let pool_id = normalize_hex(&pool_id_hex);
        let storage = cx.get_storage::<S>();

        let composite = storage
            .get_spo_composite_by_pool_id(&pool_id, DEFAULT_PERFORMANCE_LIMIT)
            .await
            .map_err_into_server_error(|| "get SPO composite by pool ID")?;

        Ok(composite.map(Into::into))
    }

    /// Get SPO identifiers ordered by performance.
    #[trace]
    async fn stake_pool_operators(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
    ) -> ApiResult<Vec<String>> {
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(20).clamp(1, 100) as i64;

        let ids = storage
            .get_stake_pool_operator_ids(limit)
            .await
            .map_err_into_server_error(|| "get stake pool operators")?;

        Ok(ids)
    }

    /// Get latest SPO performance entries.
    #[trace]
    async fn spo_performance_latest(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ApiResult<Vec<EpochPerf>> {
        let storage = cx.get_storage::<S>();
        let limit = limit
            .unwrap_or(DEFAULT_PERFORMANCE_LIMIT as i32)
            .clamp(1, 500) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;

        let perfs = storage
            .get_spo_performance_latest(limit, offset)
            .await
            .map_err_into_server_error(|| "get SPO performance latest")?;

        Ok(perfs.into_iter().map(Into::into).collect())
    }

    /// Get SPO performance by SPO key.
    #[trace]
    async fn spo_performance_by_spo_sk(
        &self,
        cx: &Context<'_>,
        spo_sk_hex: String,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ApiResult<Vec<EpochPerf>> {
        let spo_sk = normalize_hex(&spo_sk_hex);
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(100).clamp(1, 500) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;

        let perfs = storage
            .get_spo_performance_by_spo_sk(&spo_sk, limit, offset)
            .await
            .map_err_into_server_error(|| "get SPO performance by SPO key")?;

        Ok(perfs.into_iter().map(Into::into).collect())
    }

    /// Get epoch performance for all SPOs.
    #[trace]
    async fn epoch_performance(
        &self,
        cx: &Context<'_>,
        epoch: i64,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> ApiResult<Vec<EpochPerf>> {
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(100).clamp(1, 500) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;

        let perfs = storage
            .get_epoch_performance(epoch, limit, offset)
            .await
            .map_err_into_server_error(|| "get epoch performance")?;

        Ok(perfs.into_iter().map(Into::into).collect())
    }

    /// Get current epoch information.
    #[trace]
    async fn current_epoch_info(&self, cx: &Context<'_>) -> ApiResult<Option<EpochInfo>> {
        let storage = cx.get_storage::<S>();

        let info = storage
            .get_current_epoch_info()
            .await
            .map_err_into_server_error(|| "get current epoch info")?;

        Ok(info.map(Into::into))
    }

    /// Get epoch utilization (produced/expected ratio).
    #[trace]
    async fn epoch_utilization(&self, cx: &Context<'_>, epoch: i32) -> ApiResult<Option<f64>> {
        let storage = cx.get_storage::<S>();

        let utilization = storage
            .get_epoch_utilization(epoch as i64)
            .await
            .map_err_into_server_error(|| "get epoch utilization")?;

        Ok(utilization)
    }

    /// Get committee membership for an epoch.
    #[trace]
    async fn committee(&self, cx: &Context<'_>, epoch: i64) -> ApiResult<Vec<CommitteeMember>> {
        let storage = cx.get_storage::<S>();

        let members = storage
            .get_committee(epoch)
            .await
            .map_err_into_server_error(|| "get committee")?;

        Ok(members.into_iter().map(Into::into).collect())
    }

    /// Get cumulative registration totals for an epoch range.
    #[trace]
    async fn registered_totals_series(
        &self,
        cx: &Context<'_>,
        from_epoch: i64,
        to_epoch: i64,
    ) -> ApiResult<Vec<RegisteredTotals>> {
        let storage = cx.get_storage::<S>();

        let totals = storage
            .get_registered_totals_series(from_epoch, to_epoch)
            .await
            .map_err_into_server_error(|| "get registered totals series")?;

        Ok(totals.into_iter().map(Into::into).collect())
    }

    /// Get registration statistics for an epoch range.
    #[trace]
    async fn registered_spo_series(
        &self,
        cx: &Context<'_>,
        from_epoch: i64,
        to_epoch: i64,
    ) -> ApiResult<Vec<RegisteredStat>> {
        let storage = cx.get_storage::<S>();

        let stats = storage
            .get_registered_spo_series(from_epoch, to_epoch)
            .await
            .map_err_into_server_error(|| "get registered SPO series")?;

        Ok(stats.into_iter().map(Into::into).collect())
    }

    /// Get raw presence events for an epoch range.
    #[trace]
    async fn registered_presence(
        &self,
        cx: &Context<'_>,
        from_epoch: i64,
        to_epoch: i64,
    ) -> ApiResult<Vec<PresenceEvent>> {
        let storage = cx.get_storage::<S>();

        let events = storage
            .get_registered_presence(from_epoch, to_epoch)
            .await
            .map_err_into_server_error(|| "get registered presence")?;

        Ok(events.into_iter().map(Into::into).collect())
    }

    /// Get first valid epoch for each SPO identity.
    #[trace]
    async fn registered_first_valid_epochs(
        &self,
        cx: &Context<'_>,
        upto_epoch: Option<i64>,
    ) -> ApiResult<Vec<FirstValidEpoch>> {
        let storage = cx.get_storage::<S>();

        let epochs = storage
            .get_registered_first_valid_epochs(upto_epoch)
            .await
            .map_err_into_server_error(|| "get registered first valid epochs")?;

        Ok(epochs.into_iter().map(Into::into).collect())
    }

    /// Get stake distribution with search and ordering.
    #[trace]
    async fn stake_distribution(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
        search: Option<String>,
        order_by_stake_desc: Option<bool>,
    ) -> ApiResult<Vec<StakeShare>> {
        let storage = cx.get_storage::<S>();
        let limit = limit.unwrap_or(50).clamp(1, 500) as i64;
        let offset = offset.unwrap_or(0).max(0) as i64;
        let search_ref = search.as_deref().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        let order_desc = order_by_stake_desc.unwrap_or(true);

        let (shares, _total) = storage
            .get_stake_distribution(limit, offset, search_ref, order_desc)
            .await
            .map_err_into_server_error(|| "get stake distribution")?;

        Ok(shares.into_iter().map(Into::into).collect())
    }

    /// List c2m-bridge events with optional filters.
    #[trace]
    #[allow(clippy::too_many_arguments)]
    #[graphql(directive = beta::apply())]
    async fn bridge_events(
        &self,
        cx: &Context<'_>,
        recipient: Option<HexEncoded>,
        variant: Option<BridgeEventVariant>,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> ApiResult<Vec<BridgeEvent>> {
        let storage = cx.get_storage::<S>();
        let recipient = recipient
            .map(|h| h.hex_decode::<UnshieldedAddress>())
            .transpose()
            .map_err_into_client_error(|| "invalid recipient address")?;

        let filter = BridgeEventFilter {
            variants: variant.map(Into::into).into_iter().collect(),
            recipient,
            block_height_from,
            block_height_to,
            id_from: None,
        };
        let events = storage
            .get_bridge_events(
                &filter,
                offset.unwrap_or(0),
                limit.unwrap_or(100).min(1_000),
            )
            .await
            .map_err_into_server_error(|| "get bridge events")?;

        Ok(events.into_iter().map(Into::into).collect())
    }

    /// Get the c2m-bridge balance summary (deposited, claimed, balance) for an address.
    #[trace]
    #[graphql(directive = beta::apply())]
    async fn bridge_balance(
        &self,
        cx: &Context<'_>,
        address: HexEncoded,
    ) -> ApiResult<BridgeBalance> {
        let storage = cx.get_storage::<S>();
        let address = address
            .hex_decode::<UnshieldedAddress>()
            .map_err_into_client_error(|| "invalid recipient address")?;

        let mut balance = storage
            .get_bridge_balance(address)
            .await
            .map_err_into_server_error(|| "get bridge balance")?;

        // Override `balance` with the authoritative remaining-claimable from the ledger's
        // `bridge_receiving` map (net of fees, zero once fully claimed). `deposited - claimed` over
        // events would instead carry the bridge fee as a residual.
        balance.balance = match storage
            .get_highest_ledger_state()
            .await
            .map_err_into_server_error(|| "get highest ledger state")?
        {
            Some((protocol_version, ledger_state_key)) => {
                ledger::LedgerState::load(&ledger_state_key, protocol_version.ledger_version())
                    .map_err_into_server_error(|| "load ledger state")?
                    .bridge_receiving(address)
            }
            None => 0,
        };

        Ok(balance.into())
    }

    /// Convenience query for a recipient's deposit history. By default returns only successful
    /// `UserTransfer` events; pass `includeUnapproved: true` to also include `UnapprovedTransfer`.
    #[trace]
    #[graphql(directive = beta::apply())]
    async fn bridge_deposits(
        &self,
        cx: &Context<'_>,
        recipient: HexEncoded,
        include_unapproved: Option<bool>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> ApiResult<Vec<BridgeEvent>> {
        let storage = cx.get_storage::<S>();
        let recipient = recipient
            .hex_decode::<UnshieldedAddress>()
            .map_err_into_client_error(|| "invalid recipient address")?;

        let mut variants = vec![BridgeEventVariant::UserTransfer.into()];
        if include_unapproved.unwrap_or(false) {
            variants.push(BridgeEventVariant::UnapprovedTransfer.into());
        }
        let filter = BridgeEventFilter {
            variants,
            recipient: Some(recipient),
            ..Default::default()
        };
        let events = storage
            .get_bridge_events(
                &filter,
                offset.unwrap_or(0),
                limit.unwrap_or(100).min(1_000),
            )
            .await
            .map_err_into_server_error(|| "get bridge deposits")?;

        Ok(events.into_iter().map(Into::into).collect())
    }

    /// List Reserve top-up events (ReserveTransfer), optionally bounded by block height.
    #[trace]
    #[graphql(directive = beta::apply())]
    async fn bridge_reserve_inflows(
        &self,
        cx: &Context<'_>,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> ApiResult<Vec<BridgeEvent>> {
        let storage = cx.get_storage::<S>();
        let events = storage
            .get_bridge_reserve_inflows(
                block_height_from,
                block_height_to,
                offset.unwrap_or(0),
                limit.unwrap_or(100).min(1_000),
            )
            .await
            .map_err_into_server_error(|| "get bridge reserve inflows")?;

        Ok(events.into_iter().map(Into::into).collect())
    }

    /// List treasury-redirected events (Invalid, Unapproved, SubminimalFlush), optionally
    /// filtered by reason and block range.
    #[trace]
    #[allow(clippy::too_many_arguments)]
    #[graphql(directive = beta::apply())]
    async fn bridge_treasury_inflows(
        &self,
        cx: &Context<'_>,
        reason: Option<BridgeTreasuryReason>,
        block_height_from: Option<u64>,
        block_height_to: Option<u64>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> ApiResult<Vec<BridgeEvent>> {
        let storage = cx.get_storage::<S>();
        let reason: Option<TreasuryReason> = reason.map(Into::into);
        let events = storage
            .get_bridge_treasury_inflows(
                reason,
                block_height_from,
                block_height_to,
                offset.unwrap_or(0),
                limit.unwrap_or(100).min(1_000),
            )
            .await
            .map_err_into_server_error(|| "get bridge treasury inflows")?;

        Ok(events.into_iter().map(Into::into).collect())
    }

    /// Aggregate snapshot of bridge inflows to protocol pools (Reserve and Treasury).
    #[trace]
    #[graphql(directive = beta::apply())]
    async fn bridge_pool_summary(
        &self,
        cx: &Context<'_>,
        at_block: Option<u64>,
    ) -> ApiResult<BridgePoolSummary> {
        let storage = cx.get_storage::<S>();
        let summary = storage
            .get_bridge_pool_summary(at_block)
            .await
            .map_err_into_server_error(|| "get bridge pool summary")?;

        Ok(summary.into())
    }
}

/// Normalize hex string by stripping 0x prefix and lowercasing.
fn normalize_hex(input: &str) -> String {
    let s = input
        .strip_prefix("0x")
        .unwrap_or(input)
        .strip_prefix("0X")
        .unwrap_or(input);
    s.to_ascii_lowercase()
}

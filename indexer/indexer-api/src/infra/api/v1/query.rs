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
    domain::storage::Storage,
    infra::api::{
        ApiResult, ContextExt, ResultExt,
        v1::{
            HexEncoded,
            block::{Block, BlockOffset},
            contract_action::{ContractAction, ContractActionOffset},
            transaction::{Transaction, TransactionOffset},
        },
    },
};
use async_graphql::{Context, Object};
use fastrace::trace;
use std::marker::PhantomData;

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
    pub async fn block(
        &self,
        cx: &Context<'_>,
        offset: Option<BlockOffset>,
    ) -> ApiResult<Option<Block<S>>> {
        let storage = cx.get_storage::<S>();

        let block = match offset {
            Some(BlockOffset::Hash(hash)) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid block hash")?;

                storage
                    .get_block_by_hash(hash)
                    .await
                    .map_err_into_server_error(|| format!("get block by hash {hash}"))?
            }

            Some(BlockOffset::Height(height)) => storage
                .get_block_by_height(height)
                .await
                .map_err_into_server_error(|| format!("get block by height {height}"))?,

            None => storage
                .get_latest_block()
                .await
                .map_err_into_server_error(|| "get latest block")?,
        };

        Ok(block.map(Into::into))
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
}

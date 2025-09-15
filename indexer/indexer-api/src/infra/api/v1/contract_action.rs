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
    domain::{self, storage::Storage},
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v1::{
            AsBytesExt, HexEncoded,
            block::BlockOffset,
            transaction::{Transaction, TransactionOffset},
            unshielded::ContractBalance,
        },
    },
};
use async_graphql::{ComplexObject, Context, Interface, OneofObject, SimpleObject};
use derive_more::Debug;
use indexer_common::domain::ledger::SerializedContractAddress;
use std::marker::PhantomData;

/// A contract action.
#[derive(Debug, Clone, Interface)]
#[allow(clippy::duplicated_attributes)]
#[graphql(
    field(name = "address", ty = "HexEncoded"),
    field(name = "state", ty = "HexEncoded"),
    field(name = "chain_state", ty = "HexEncoded"),
    field(name = "transaction", ty = "Transaction<S>")
)]
pub enum ContractAction<S: Storage> {
    /// A contract deployment.
    Deploy(ContractDeploy<S>),

    /// A contract call.
    Call(ContractCall<S>),

    /// A contract update.
    Update(ContractUpdate<S>),
}

impl<S> From<domain::ContractAction> for ContractAction<S>
where
    S: Storage,
{
    fn from(action: domain::ContractAction) -> Self {
        let domain::ContractAction {
            id,
            address,
            state,
            attributes,
            chain_state,
            transaction_id,
            ..
        } = action;

        match attributes {
            domain::ContractAttributes::Deploy => ContractAction::Deploy(ContractDeploy {
                address: address.hex_encode(),
                state: state.hex_encode(),
                chain_state: chain_state.hex_encode(),
                transaction_id,
                contract_action_id: id,
                _s: PhantomData,
            }),

            domain::ContractAttributes::Call { entry_point } => {
                ContractAction::Call(ContractCall {
                    address: address.hex_encode(),
                    state: state.hex_encode(),
                    entry_point: entry_point.hex_encode(),
                    chain_state: chain_state.hex_encode(),
                    transaction_id,
                    contract_action_id: id,
                    raw_address: address,
                    _s: PhantomData,
                })
            }

            domain::ContractAttributes::Update => ContractAction::Update(ContractUpdate {
                address: address.hex_encode(),
                state: state.hex_encode(),
                chain_state: chain_state.hex_encode(),
                transaction_id,
                contract_action_id: id,
                _s: PhantomData,
            }),
        }
    }
}

/// A contract deployment.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct ContractDeploy<S>
where
    S: Storage,
{
    /// The hex-encoded serialized address.
    address: HexEncoded,

    /// The hex-encoded serialized state.
    state: HexEncoded,

    /// The hex-encoded serialized contract-specific zswap state.
    chain_state: HexEncoded,

    #[graphql(skip)]
    transaction_id: u64,

    #[graphql(skip)]
    contract_action_id: u64,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ContractDeploy<S>
where
    S: Storage,
{
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }

    /// Unshielded token balances held by this contract.
    /// According to the architecture, deployed contracts must have zero balance.
    async fn unshielded_balances(&self, cx: &Context<'_>) -> ApiResult<Vec<ContractBalance>> {
        let storage = cx.get_storage::<S>();
        let balances = storage
            .get_unshielded_balances_by_action_id(self.contract_action_id)
            .await
            .map_err_into_server_error(|| {
                format!(
                    "get contract balances by action id {}",
                    self.contract_action_id
                )
            })?;

        Ok(balances.into_iter().map(Into::into).collect())
    }
}

/// A contract call.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct ContractCall<S>
where
    S: Storage,
{
    /// The hex-encoded serialized address.
    address: HexEncoded,

    /// The hex-encoded serialized state.
    state: HexEncoded,

    /// The hex-encoded serialized contract-specific zswap state.
    chain_state: HexEncoded,

    /// The hex-encoded serialized entry point.
    entry_point: HexEncoded,

    #[graphql(skip)]
    transaction_id: u64,

    #[graphql(skip)]
    contract_action_id: u64,

    #[graphql(skip)]
    raw_address: SerializedContractAddress,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ContractCall<S>
where
    S: Storage,
{
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }

    async fn deploy(&self, cx: &Context<'_>) -> ApiResult<ContractDeploy<S>> {
        let action = cx
            .get_storage::<S>()
            .get_contract_deploy_by_address(&self.raw_address)
            .await
            .map_err_into_server_error(|| {
                format!("get contract deploy by address {}", self.raw_address)
            })?
            .expect("contract call has contract deploy");

        let deploy = match ContractAction::from(action) {
            ContractAction::Deploy(deploy) => deploy,
            _ => panic!("unexpected contract action"),
        };

        Ok(deploy)
    }

    /// Unshielded token balances held by this contract.
    async fn unshielded_balances(&self, cx: &Context<'_>) -> ApiResult<Vec<ContractBalance>> {
        let storage = cx.get_storage::<S>();
        let balances = storage
            .get_unshielded_balances_by_action_id(self.contract_action_id)
            .await
            .map_err_into_server_error(|| {
                format!(
                    "get contract balances by action id {}",
                    self.contract_action_id
                )
            })?;

        Ok(balances.into_iter().map(Into::into).collect())
    }
}

/// A contract update.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct ContractUpdate<S>
where
    S: Storage,
{
    /// The hex-encoded serialized address.
    address: HexEncoded,

    /// The hex-encoded serialized state.
    state: HexEncoded,

    /// The hex-encoded serialized contract-specific zswap state.
    chain_state: HexEncoded,

    #[graphql(skip)]
    transaction_id: u64,

    #[graphql(skip)]
    contract_action_id: u64,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ContractUpdate<S>
where
    S: Storage,
{
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }

    /// Unshielded token balances held by this contract after the update.
    async fn unshielded_balances(&self, cx: &Context<'_>) -> ApiResult<Vec<ContractBalance>> {
        let storage = cx.get_storage::<S>();
        let balances = storage
            .get_unshielded_balances_by_action_id(self.contract_action_id)
            .await
            .map_err_into_server_error(|| {
                format!(
                    "get contract balances by action id {}",
                    self.contract_action_id
                )
            })?;

        Ok(balances.into_iter().map(Into::into).collect())
    }
}

/// Either a block offset or a transaction offset.
#[derive(Debug, OneofObject)]
pub enum ContractActionOffset {
    /// Either a block hash or a block height.
    BlockOffset(BlockOffset),

    /// Either a transaction hash or a transaction identifier.
    TransactionOffset(TransactionOffset),
}

async fn get_transaction_by_id<S>(id: u64, cx: &Context<'_>) -> ApiResult<Transaction<S>>
where
    S: Storage,
{
    let transaction = cx
        .get_storage::<S>()
        .get_transaction_by_id(id)
        .await
        .map_err_into_server_error(|| format!("get transaction by ID {id})"))?
        .ok_or_server_error(|| format!("transaction with ID {id} not found"))?;

    Ok(transaction.into())
}

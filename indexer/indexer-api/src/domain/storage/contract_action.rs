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

use crate::domain::{ContractAction, ContractBalance, storage::NoopStorage};
use futures::{Stream, stream};
use indexer_common::domain::{
    BlockHash,
    ledger::{SerializedContractAddress, SerializedTransactionIdentifier, TransactionHash},
};
use std::{fmt::Debug, num::NonZeroU32};

#[trait_variant::make(Send)]
pub trait ContractActionStorage
where
    Self: Debug + Clone + Send + Sync + 'static,
{
    /// Get the contract deploy for the given address.
    async fn get_contract_deploy_by_address(
        &self,
        address: &SerializedContractAddress,
    ) -> Result<Option<ContractAction>, sqlx::Error>;

    /// Get the latest contract action for the given address.
    async fn get_latest_contract_action_by_address(
        &self,
        address: &SerializedContractAddress,
    ) -> Result<Option<ContractAction>, sqlx::Error>;

    /// Get the latest contract action for the given address and block hash.
    async fn get_contract_action_by_address_and_block_hash(
        &self,
        address: &SerializedContractAddress,
        hash: BlockHash,
    ) -> Result<Option<ContractAction>, sqlx::Error>;

    /// Get the latest contract action for the given address and block height.
    async fn get_contract_action_by_address_and_block_height(
        &self,
        address: &SerializedContractAddress,
        height: u32,
    ) -> Result<Option<ContractAction>, sqlx::Error>;

    /// Get the latest contract action for the given address and transaction hash.
    async fn get_contract_action_by_address_and_transaction_hash(
        &self,
        address: &SerializedContractAddress,
        hash: TransactionHash,
    ) -> Result<Option<ContractAction>, sqlx::Error>;

    /// Get the latest contract action for the given address and transaction identifier.
    async fn get_contract_action_by_address_and_transaction_identifier(
        &self,
        address: &SerializedContractAddress,
        identifier: &SerializedTransactionIdentifier,
    ) -> Result<Option<ContractAction>, sqlx::Error>;

    /// Get the contract actions for the transaction with the given id, ordered by transaction ID.
    async fn get_contract_actions_by_transaction_id(
        &self,
        id: u64,
    ) -> Result<Vec<ContractAction>, sqlx::Error>;

    /// Get a stream of contract actions for the given address starting at the given contract_action
    /// ID, ordered by transaction ID.
    fn get_contract_actions_by_address(
        &self,
        address: &SerializedContractAddress,
        contract_action_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ContractAction, sqlx::Error>> + Send;

    /// Get unshielded token balances for a contract action.
    async fn get_unshielded_balances_by_action_id(
        &self,
        contract_action_id: u64,
    ) -> Result<Vec<ContractBalance>, sqlx::Error>;

    /// Get the ID for the first contract action in a transaction in a block with the given block
    /// height or higher.
    async fn get_contract_action_id_by_block_height(
        &self,
        block_height: u32,
    ) -> Result<Option<u64>, sqlx::Error>;
}

#[allow(unused_variables)]
impl ContractActionStorage for NoopStorage {
    async fn get_contract_deploy_by_address(
        &self,
        address: &SerializedContractAddress,
    ) -> Result<Option<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_latest_contract_action_by_address(
        &self,
        address: &SerializedContractAddress,
    ) -> Result<Option<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_contract_action_by_address_and_block_hash(
        &self,
        address: &SerializedContractAddress,
        hash: BlockHash,
    ) -> Result<Option<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_contract_action_by_address_and_block_height(
        &self,
        address: &SerializedContractAddress,
        height: u32,
    ) -> Result<Option<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_contract_action_by_address_and_transaction_hash(
        &self,
        address: &SerializedContractAddress,
        hash: TransactionHash,
    ) -> Result<Option<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_contract_action_by_address_and_transaction_identifier(
        &self,
        address: &SerializedContractAddress,
        identifier: &SerializedTransactionIdentifier,
    ) -> Result<Option<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_contract_actions_by_transaction_id(
        &self,
        id: u64,
    ) -> Result<Vec<ContractAction>, sqlx::Error> {
        unimplemented!()
    }

    fn get_contract_actions_by_address(
        &self,
        address: &SerializedContractAddress,
        contract_action_id: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ContractAction, sqlx::Error>> + Send {
        stream::empty()
    }

    async fn get_unshielded_balances_by_action_id(
        &self,
        contract_action_id: u64,
    ) -> Result<Vec<ContractBalance>, sqlx::Error> {
        unimplemented!()
    }

    async fn get_contract_action_id_by_block_height(
        &self,
        block_height: u32,
    ) -> Result<Option<u64>, sqlx::Error> {
        unimplemented!()
    }
}

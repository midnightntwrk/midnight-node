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
    UnshieldedUtxo,
    storage::{
        NoopStorage, block::BlockStorage, contract_action::ContractActionStorage,
        transaction::TransactionStorage, wallet::WalletStorage,
    },
};
use indexer_common::domain::ledger::RawUnshieldedAddress;
use sqlx::Error;
use std::fmt::Debug;

/// Storage abstraction for unshielded UTXO operations.
#[trait_variant::make(Send)]
pub trait UnshieldedUtxoStorage
where
    Self: BlockStorage
        + ContractActionStorage
        + TransactionStorage
        + WalletStorage
        + Debug
        + Clone
        + Send
        + Sync
        + 'static,
{
    /// Get all unshielded UTXOs for a given address.
    async fn get_unshielded_utxos_by_address(
        &self,
        address: RawUnshieldedAddress,
    ) -> Result<Vec<UnshieldedUtxo>, sqlx::Error>;

    /// Get unshielded UTXOs created by a specific transaction, ordered by output index.
    async fn get_unshielded_utxos_created_by_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, sqlx::Error>;

    /// Get unshielded UTXOs spent by a specific transaction, ordered by output index.
    async fn get_unshielded_utxos_spent_by_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, sqlx::Error>;

    /// Get unshielded UTXOs created in a specific transaction for a specific address, ordered by
    /// output index.
    async fn get_unshielded_utxos_by_address_created_by_transaction(
        &self,
        address: RawUnshieldedAddress,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, sqlx::Error>;

    /// Get unshielded UTXOs spent in a specific transaction for a specific address, ordered by
    /// output index.
    async fn get_unshielded_utxos_by_address_spent_by_transaction(
        &self,
        address: RawUnshieldedAddress,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, sqlx::Error>;
}

#[allow(unused_variables)]
impl UnshieldedUtxoStorage for NoopStorage {
    async fn get_unshielded_utxos_by_address(
        &self,
        address: RawUnshieldedAddress,
    ) -> Result<Vec<UnshieldedUtxo>, Error> {
        unimplemented!()
    }

    async fn get_unshielded_utxos_created_by_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, Error> {
        unimplemented!()
    }

    async fn get_unshielded_utxos_spent_by_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, Error> {
        unimplemented!()
    }

    async fn get_unshielded_utxos_by_address_created_by_transaction(
        &self,
        address: RawUnshieldedAddress,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, Error> {
        unimplemented!()
    }

    async fn get_unshielded_utxos_by_address_spent_by_transaction(
        &self,
        address: RawUnshieldedAddress,
        transaction_id: u64,
    ) -> Result<Vec<UnshieldedUtxo>, Error> {
        unimplemented!()
    }
}

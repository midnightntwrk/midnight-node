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

use crate::domain::{self, storage::Storage};
use async_graphql::dataloader::Loader;
use derive_more::Deref;
use indexer_common::domain::BlockHash;
use itertools::Itertools;
use std::{collections::HashMap, sync::Arc};

#[derive(Deref)]
pub struct BlockByHashLoader<S>(S);

impl<S: Storage> BlockByHashLoader<S> {
    pub fn new(storage: S) -> Self {
        Self(storage)
    }
}

impl<S: Storage> Loader<BlockHash> for BlockByHashLoader<S> {
    type Value = domain::Block;
    type Error = Arc<sqlx::Error>;

    async fn load(
        &self,
        keys: &[BlockHash],
    ) -> Result<HashMap<BlockHash, domain::Block>, Arc<sqlx::Error>> {
        let blocks = self
            .get_blocks_by_hashes(keys)
            .await
            .map_err(Arc::new)?
            .into_iter()
            .map(|b| (b.hash, b))
            .collect();

        Ok(blocks)
    }
}

#[derive(Deref)]
pub struct TransactionByIdLoader<S>(S);

impl<S: Storage> TransactionByIdLoader<S> {
    pub fn new(storage: S) -> Self {
        Self(storage)
    }
}

impl<S: Storage> Loader<u64> for TransactionByIdLoader<S> {
    type Value = domain::Transaction;
    type Error = Arc<sqlx::Error>;

    async fn load(
        &self,
        keys: &[u64],
    ) -> Result<HashMap<u64, domain::Transaction>, Arc<sqlx::Error>> {
        let transactions = self
            .get_transactions_by_ids(keys)
            .await
            .map_err(Arc::new)?
            .into_iter()
            .map(|t| (t.id(), t))
            .collect::<HashMap<_, _>>();

        Ok(transactions)
    }
}

#[derive(Deref)]
pub struct TransactionsByBlockIdLoader<S>(S);

impl<S: Storage> TransactionsByBlockIdLoader<S> {
    pub fn new(storage: S) -> Self {
        Self(storage)
    }
}

impl<S: Storage> Loader<u64> for TransactionsByBlockIdLoader<S> {
    type Value = Vec<domain::Transaction>;
    type Error = Arc<sqlx::Error>;

    async fn load(
        &self,
        keys: &[u64],
    ) -> Result<HashMap<u64, Vec<domain::Transaction>>, Arc<sqlx::Error>> {
        let transactions = self
            .get_transactions_by_block_ids(keys)
            .await
            .map_err(Arc::new)?
            .into_iter()
            .into_group_map();

        Ok(transactions)
    }
}

#[derive(Deref)]
pub struct ContractActionsByTransactionIdLoader<S>(S);

impl<S: Storage> ContractActionsByTransactionIdLoader<S> {
    pub fn new(storage: S) -> Self {
        Self(storage)
    }
}

impl<S: Storage> Loader<u64> for ContractActionsByTransactionIdLoader<S> {
    type Value = Vec<domain::ContractAction>;
    type Error = Arc<sqlx::Error>;

    async fn load(
        &self,
        keys: &[u64],
    ) -> Result<HashMap<u64, Vec<domain::ContractAction>>, Arc<sqlx::Error>> {
        let actions = self
            .get_contract_actions_by_transaction_ids(keys)
            .await
            .map_err(Arc::new)?
            .into_iter()
            .into_group_map_by(|action| action.transaction_id);

        Ok(actions)
    }
}

#[derive(Deref)]
pub struct ContractEventsByContractActionIdLoader<S>(S);

impl<S: Storage> ContractEventsByContractActionIdLoader<S> {
    pub fn new(storage: S) -> Self {
        Self(storage)
    }
}

impl<S: Storage> Loader<u64> for ContractEventsByContractActionIdLoader<S> {
    type Value = Vec<domain::ContractEventRow>;
    type Error = Arc<sqlx::Error>;

    async fn load(
        &self,
        keys: &[u64],
    ) -> Result<HashMap<u64, Vec<domain::ContractEventRow>>, Arc<sqlx::Error>> {
        let events = self
            .get_contract_events_by_contract_action_ids(keys)
            .await
            .map_err(Arc::new)?
            .into_iter()
            .into_group_map();

        Ok(events)
    }
}

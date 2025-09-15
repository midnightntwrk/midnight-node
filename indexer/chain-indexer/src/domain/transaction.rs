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

use crate::domain::{ContractAction, node};
use indexer_common::domain::{
    ByteArray, ProtocolVersion,
    ledger::{
        SerializedTransaction, SerializedTransactionIdentifier, SerializedZswapStateRoot,
        TransactionHash, TransactionResult, UnshieldedUtxo,
    },
};
use sqlx::{FromRow, Type};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transaction {
    Regular(RegularTransaction),
    System(SystemTransaction),
}

impl Transaction {
    pub fn variant(&self) -> TransactionVariant {
        match self {
            Transaction::Regular(_) => TransactionVariant::Regular,
            Transaction::System(_) => TransactionVariant::System,
        }
    }

    pub fn hash(&self) -> TransactionHash {
        match self {
            Transaction::Regular(transaction) => transaction.hash,
            Transaction::System(transaction) => transaction.hash,
        }
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        match self {
            Transaction::Regular(transaction) => transaction.protocol_version,
            Transaction::System(transaction) => transaction.protocol_version,
        }
    }

    pub fn raw(&self) -> &[u8] {
        match self {
            Transaction::Regular(transaction) => &transaction.raw,
            Transaction::System(transaction) => &transaction.raw,
        }
    }
}

impl From<node::Transaction> for Transaction {
    fn from(transaction: node::Transaction) -> Self {
        match transaction {
            node::Transaction::Regular(regular_transaction) => {
                Transaction::Regular(regular_transaction.into())
            }

            node::Transaction::System(system_transaction) => {
                Transaction::System(system_transaction.into())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularTransaction {
    // These fields come from node::Transaction.
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    pub raw: SerializedTransaction,
    pub identifiers: Vec<SerializedTransactionIdentifier>,
    pub contract_actions: Vec<ContractAction>,
    pub paid_fees: u128,
    pub estimated_fees: u128,

    // These fields come from applying the node transactions to the ledger state.
    pub transaction_result: TransactionResult,
    pub merkle_tree_root: SerializedZswapStateRoot,
    pub start_index: u64,
    pub end_index: u64,
    pub created_unshielded_utxos: Vec<UnshieldedUtxo>,
    pub spent_unshielded_utxos: Vec<UnshieldedUtxo>,
}

impl From<node::RegularTransaction> for RegularTransaction {
    fn from(transaction: node::RegularTransaction) -> Self {
        Self {
            hash: transaction.hash,
            protocol_version: transaction.protocol_version,
            identifiers: transaction.identifiers,
            raw: transaction.raw,
            contract_actions: transaction.contract_actions,
            paid_fees: transaction.paid_fees,
            estimated_fees: transaction.estimated_fees,
            transaction_result: Default::default(),
            merkle_tree_root: Default::default(),
            start_index: Default::default(),
            end_index: Default::default(),
            created_unshielded_utxos: Default::default(),
            spent_unshielded_utxos: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTransaction {
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    pub raw: SerializedTransaction,
}

impl From<node::SystemTransaction> for SystemTransaction {
    fn from(transaction: node::SystemTransaction) -> Self {
        Self {
            hash: transaction.hash,
            protocol_version: transaction.protocol_version,
            raw: transaction.raw,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[cfg_attr(feature = "cloud", sqlx(type_name = "TRANSACTION_VARIANT"))]
pub enum TransactionVariant {
    Regular,
    System,
}

/// All serialized transactions from a single block along with metadata needed for ledger state
/// application.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct BlockTransactions {
    pub transactions: Vec<(TransactionVariant, SerializedTransaction)>,

    #[sqlx(try_from = "i64")]
    pub protocol_version: ProtocolVersion,

    pub block_parent_hash: ByteArray<32>,

    #[sqlx(try_from = "i64")]
    pub block_timestamp: u64,
}

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

use crate::domain::{ContractAction, node};
use derive_more::Debug;
use indexer_common::domain::{
    LedgerEvent, ProtocolVersion, SerializedTransaction, SerializedTransactionIdentifier,
    SerializedZswapMerkleTreeRoot, TransactionHash, TransactionResult, TransactionVariant,
    UnshieldedUtxo,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transaction {
    Regular(Box<RegularTransaction>),
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
                Transaction::Regular(Box::new(regular_transaction.into()))
            }

            node::Transaction::System(system_transaction) => {
                Transaction::System(system_transaction.into())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularTransaction {
    // These fields come from the node.
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    #[debug(skip)]
    pub raw: SerializedTransaction,
    #[debug(skip)]
    pub identifiers: Vec<SerializedTransactionIdentifier>,
    #[debug(skip)]
    pub contract_actions: Vec<ContractAction>,
    pub paid_fees: u128,
    pub estimated_fees: u128,

    // These fields are set after applying the transaction to the ledger state.
    pub transaction_result: TransactionResult,
    #[debug(skip)]
    pub zswap_merkle_tree_root: SerializedZswapMerkleTreeRoot,
    pub zswap_start_index: u64,
    pub zswap_end_index: u64, // Exclusive, i.e. the next free index.
    pub dust_commitment_start_index: u64,
    pub dust_commitment_end_index: u64,
    pub dust_generation_start_index: u64,
    pub dust_generation_end_index: u64,
    #[debug(skip)]
    pub created_unshielded_utxos: Vec<UnshieldedUtxo>,
    #[debug(skip)]
    pub spent_unshielded_utxos: Vec<UnshieldedUtxo>,
    #[debug(skip)]
    pub ledger_events: Vec<LedgerEvent>,
    /// Populated when the underlying transaction is a `ClaimRewards` with
    /// `ClaimKind::CardanoBridge` (a user claiming bridged NIGHT). Set from the indexer-common
    /// apply outcome; `None` for every other transaction.
    #[debug(skip)]
    pub bridge_claim: Option<indexer_common::domain::bridge::BridgeClaim>,
}

impl From<node::RegularTransaction> for RegularTransaction {
    fn from(transaction: node::RegularTransaction) -> Self {
        Self {
            hash: transaction.hash,
            protocol_version: transaction.protocol_version,
            identifiers: transaction.identifiers,
            raw: transaction.raw,
            contract_actions: transaction.contract_actions,
            paid_fees: Default::default(),
            estimated_fees: Default::default(),
            transaction_result: Default::default(),
            zswap_merkle_tree_root: Default::default(),
            zswap_start_index: Default::default(),
            zswap_end_index: Default::default(),
            dust_commitment_start_index: Default::default(),
            dust_commitment_end_index: Default::default(),
            dust_generation_start_index: Default::default(),
            dust_generation_end_index: Default::default(),
            created_unshielded_utxos: Default::default(),
            spent_unshielded_utxos: Default::default(),
            ledger_events: Default::default(),
            // Set after applying the transaction; see `apply_regular_transaction`.
            bridge_claim: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTransaction {
    // These fields come from the node.
    pub hash: TransactionHash,
    pub protocol_version: ProtocolVersion,
    #[debug(skip)]
    pub raw: SerializedTransaction,

    // These fields are set after applying the transaction to the ledger state.
    #[debug(skip)]
    pub created_unshielded_utxos: Vec<UnshieldedUtxo>,
    #[debug(skip)]
    pub ledger_events: Vec<LedgerEvent>,
}

impl From<node::SystemTransaction> for SystemTransaction {
    fn from(transaction: node::SystemTransaction) -> Self {
        Self {
            hash: transaction.hash,
            protocol_version: transaction.protocol_version,
            raw: transaction.raw,
            created_unshielded_utxos: Default::default(),
            ledger_events: Default::default(),
        }
    }
}

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

use crate::domain::bridge::BridgeClaim;
use derive_more::Debug;
use indexer_common::{
    domain::{
        BlockHash, ProtocolVersion, SerializedTransaction, SerializedTransactionIdentifier,
        SerializedZswapMerkleTreeRoot, TransactionHash, TransactionResult,
    },
    infra::sqlx::{SqlxOption, U128BeBytes},
};
use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transaction {
    Regular(RegularTransaction),
    System(SystemTransaction),
}

impl Transaction {
    pub fn id(&self) -> u64 {
        match self {
            Transaction::Regular(t) => t.id,
            Transaction::System(t) => t.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct RegularTransaction {
    #[sqlx(try_from = "i64")]
    pub id: u64,

    pub hash: TransactionHash,

    #[sqlx(try_from = "i64")]
    pub protocol_version: ProtocolVersion,

    #[debug(skip)]
    pub raw: SerializedTransaction,

    pub block_hash: BlockHash,

    #[sqlx(json)]
    pub transaction_result: TransactionResult,

    #[debug(skip)]
    #[cfg_attr(feature = "standalone", sqlx(skip))]
    pub identifiers: Vec<SerializedTransactionIdentifier>,

    #[debug(skip)]
    pub zswap_merkle_tree_root: SerializedZswapMerkleTreeRoot,

    #[sqlx(try_from = "i64")]
    pub zswap_start_index: u64,

    #[sqlx(try_from = "i64")]
    pub zswap_end_index: u64, // Exclusive, i.e. the next free index.

    #[sqlx(try_from = "i64")]
    pub dust_commitment_start_index: u64,

    #[sqlx(try_from = "i64")]
    pub dust_commitment_end_index: u64,

    #[sqlx(try_from = "i64")]
    pub dust_generation_start_index: u64,

    #[sqlx(try_from = "i64")]
    pub dust_generation_end_index: u64,

    #[sqlx(try_from = "SqlxOption<U128BeBytes>")]
    pub paid_fees: Option<u128>,

    #[sqlx(try_from = "SqlxOption<U128BeBytes>")]
    pub estimated_fees: Option<u128>,

    /// Set after the row is read, by looking up `bridge_claims` for this transaction. `Some` when
    /// the transaction is a `ClaimRewards` with `ClaimKind::CardanoBridge`, which the API surfaces
    /// as a `BridgeClaimTransaction` rather than a `RegularTransaction`. Boxed because it is
    /// `None` for all but bridge-claim transactions, keeping the common variant small.
    #[sqlx(skip)]
    pub bridge_claim: Option<Box<BridgeClaim>>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SystemTransaction {
    #[sqlx(try_from = "i64")]
    pub id: u64,

    pub hash: TransactionHash,

    #[sqlx(try_from = "i64")]
    pub protocol_version: ProtocolVersion,

    #[debug(skip)]
    pub raw: SerializedTransaction,

    pub block_hash: BlockHash,
}

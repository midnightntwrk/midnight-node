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

use indexer_common::{
    domain::{IntentHash, Nonce, TokenType, UnshieldedAddress},
    infra::sqlx::{SqlxOption, U128BeBytes},
};
use sqlx::FromRow;

/// An unshielded UTXO.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct UnshieldedUtxo {
    /// Database ID of the transaction that created this UTXO.
    #[sqlx(try_from = "i64")]
    pub creating_transaction_id: u64,

    /// Database ID of the transaction that spent this UTXO, if any.
    #[sqlx(try_from = "SqlxOption<i64>")]
    pub spending_transaction_id: Option<u64>,

    /// The unshielded address that owns this UTXO.
    pub owner: UnshieldedAddress,

    /// Type of token (e.g. NIGHT has all-zero bytes).
    pub token_type: TokenType,

    /// Amount (big-endian bytes in DB -> u128 here).
    #[sqlx(try_from = "U128BeBytes")]
    pub value: u128,

    /// Hash of the intent that created this UTXO.
    pub intent_hash: IntentHash,

    /// Matches ledger's u32 type but stored as BIGINT since u32 max exceeds PostgreSQL INT range.
    #[sqlx(try_from = "i64")]
    pub output_index: u32,

    #[sqlx(try_from = "SqlxOption<i64>")]
    pub ctime: Option<u64>,

    /// Initial nonce for DUST generation tracking.
    pub initial_nonce: Nonce,

    /// Whether this UTXO is registered for DUST generation.
    pub registered_for_dust_generation: bool,
}

/// Token balance held by a contract.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ContractBalance {
    /// Token type identifier.
    pub token_type: TokenType,

    /// Balance amount.
    #[sqlx(try_from = "U128BeBytes")]
    pub amount: u128,
}

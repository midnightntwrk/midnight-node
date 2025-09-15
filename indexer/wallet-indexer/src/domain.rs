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

pub mod storage;

use fastrace::trace;
use indexer_common::domain::{ProtocolVersion, ViewingKey, ledger, ledger::SerializedTransaction};
use sqlx::prelude::FromRow;

/// Relevant data of a wallet from the perspective of the Wallet Indexer.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Wallet {
    pub viewing_key: ViewingKey,
    pub last_indexed_transaction_id: u64,
}

/// Relevant data of a transaction from the perspective of the Wallet Indexer.
#[derive(Debug, Clone, FromRow)]
pub struct Transaction {
    #[sqlx(try_from = "i64")]
    pub id: u64,

    #[sqlx(try_from = "i64")]
    pub protocol_version: ProtocolVersion,

    pub raw: SerializedTransaction,
}

impl Transaction {
    /// Check the relevance of this transaction for the given wallet.
    #[trace]
    pub fn relevant(&self, wallet: &Wallet) -> Result<bool, ledger::Error> {
        let transaction = ledger::Transaction::deserialize(&self.raw, self.protocol_version)?;
        Ok(transaction.relevant(wallet.viewing_key))
    }
}

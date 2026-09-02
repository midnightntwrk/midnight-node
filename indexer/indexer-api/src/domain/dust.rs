// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use indexer_common::{
    domain::{
        ByteVec, CardanoRewardAddress, DustPublicKey, SerializedDustTreeInsertionPath,
        TransactionHash,
    },
    infra::sqlx::U128BeBytes,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// DUST generation status for a specific Cardano reward address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DustGenerationStatus {
    /// Cardano reward address.
    pub cardano_reward_address: CardanoRewardAddress,

    /// Associated DUST address (DUST public key) if registered.
    pub dust_address: Option<DustPublicKey>,

    /// Whether this reward address is registered.
    pub registered: bool,

    /// NIGHT balance backing generation.
    pub night_balance: u128,

    /// Generation rate in Specks per second.
    pub generation_rate: u128,

    /// Maximum DUST capacity in SPECK.
    pub max_capacity: u128,

    /// Current generated DUST capacity in SPECK.
    pub current_capacity: u128,

    /// Cardano UTXO transaction hash for update/unregister operations.
    pub utxo_tx_hash: Option<Vec<u8>>,

    /// Cardano UTXO output index for update/unregister operations.
    pub utxo_output_index: Option<u32>,
}

/// Aggregated dust generations data for a Cardano reward address.
#[derive(Debug, Clone)]
pub struct DustGenerations {
    pub cardano_reward_address: CardanoRewardAddress,
    pub registrations: Vec<DustRegistration>,
}

/// A single dust registration with aggregated generation stats.
#[derive(Debug, Clone)]
pub struct DustRegistration {
    pub dust_address: DustPublicKey,
    pub valid: bool,
    pub night_balance: u128,
    pub generation_rate: u128,
    pub max_capacity: u128,
    pub current_capacity: u128,
    pub utxo_tx_hash: Option<Vec<u8>>,
    pub utxo_output_index: Option<u32>,
}

/// A dust generation entry for the subscription stream.
#[derive(Debug, Clone, FromRow)]
pub struct DustGenerationEntry {
    /// Commitment-tree index from QualifiedDustOutput.mt_index.
    /// (DB column is named `merkle_index`; SELECT uses an alias.)
    #[sqlx(try_from = "i64")]
    pub commitment_mt_index: u64,

    /// Generation-tree index from DustInitialUtxo.generation_index.
    #[sqlx(try_from = "i64")]
    pub generation_mt_index: u64,

    pub owner: ByteVec,

    /// NIGHT amount backing this dust output, in STAR.
    #[sqlx(try_from = "U128BeBytes")]
    pub value: u128,

    /// DUST amount at creation, in SPECK, from QualifiedDustOutput.initial_value.
    #[sqlx(try_from = "U128BeBytes")]
    pub initial_value: u128,

    /// Hash of the NIGHT UTXO that backs this dust output (InitialNonce).
    pub backing_night: ByteVec,

    #[sqlx(try_from = "i64")]
    pub ctime: u64,

    #[sqlx(try_from = "i64")]
    pub transaction_id: u64,

    pub transaction_hash: TransactionHash,
}

/// A dust generation dtime update entry for the subscription stream.
///
/// Built from the row by the storage layer, not directly via `FromRow`,
/// because `tree_insertion_path` is sourced by deserialising
/// `LedgerEventAttributes::DustGenerationDtimeUpdate` from the row's
/// JSONB `attributes` column.
#[derive(Debug, Clone)]
pub struct DustGenerationDtimeUpdateEntry {
    /// Ledger event id of this dtime update. Internal cursor for advancing
    /// between live-tail polls; not exposed on the GraphQL surface.
    pub ledger_event_id: u64,

    /// Generation-tree index of the entry whose dtime changed.
    pub generation_mt_index: u64,

    pub owner: ByteVec,

    /// Hash of the NIGHT UTXO that backs this dust output.
    pub night_utxo_hash: ByteVec,

    /// New decay time as observed in this ledger event.
    pub new_dtime: u64,

    pub transaction_id: u64,

    pub transaction_hash: TransactionHash,

    /// Tagged-serialised `TreeInsertionPath<DustGenerationInfo>` from the
    /// originating ledger event. Surfaced verbatim on the GraphQL API so
    /// wallets can hand it to `generating_tree.update_from_evidence(...)`.
    pub tree_insertion_path: SerializedDustTreeInsertionPath,
}

/// A dust nullifier transaction for the subscription stream.
#[derive(Debug, Clone)]
pub struct DustNullifierTransaction {
    pub nullifier_le_bytes: ByteVec,
    pub commitment_le_bytes: ByteVec,
    pub transaction_id: u64,
    pub transaction_hash: TransactionHash,
    pub block_height: u32,
    pub block_hash: ByteVec,
}

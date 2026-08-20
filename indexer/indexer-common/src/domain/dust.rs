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

use crate::domain::{DustPublicKey, NightUtxoHash, Nonce};
use serde::{Deserialize, Serialize};

/// Qualified DUST output information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedDustOutput {
    /// Initial value of DUST UTXO.
    pub initial_value: u128,

    /// Owner's DUST public key.
    pub owner: DustPublicKey,

    /// Nonce for this DUST UTXO.
    pub nonce: Nonce,

    /// Sequence number.
    pub seq: u32,

    /// Creation time.
    pub ctime: u64,

    /// Backing Night UTXO nonce.
    pub backing_night: Nonce,

    /// Merkle tree index.
    pub mt_index: u64,
}

/// Merkle tree path entry for DUST trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DustMerklePathEntry {
    /// The hash of the sibling at this level (if available).
    pub sibling_hash: Option<Vec<u8>>,
    /// Whether the path goes left at this level.
    pub goes_left: bool,
}

/// DUST generation information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DustGenerationInfo {
    /// Hash of the backing Night UTXO.
    pub night_utxo_hash: NightUtxoHash,

    /// Value of backing Night UTXO.
    pub value: u128,

    /// Owner's DUST public key.
    pub owner: DustPublicKey,

    /// Initial nonce.
    pub nonce: Nonce,

    /// Creation time.
    pub ctime: u64,

    /// Decay time (when Night is spent).
    pub dtime: u64,
}

/// DUST parameters as specified in the ledger specification.
/// These values are defined in midnight-ledger/spec/dust.md and determine the economic
/// properties of DUST generation and decay.
///
/// # Unit Conversions
/// - 1 NIGHT = 10^6 STAR (atomic unit of NIGHT).
/// - 1 DUST = 10^15 SPECK (atomic unit of DUST).
///
/// # Parameter Explanations
///
/// ## night_dust_ratio (SPECK per STAR)
/// Maximum DUST that can be generated per NIGHT (5 DUST per NIGHT).
/// - Target: 5 DUST per NIGHT.
/// - Calculation: (5 DUST × 10^15 SPECK/DUST) / (10^6 STAR/NIGHT) = 5 × 10^9 SPECK/STAR.
///
/// ## generation_decay_rate (SPECK per STAR per second)
/// Rate of DUST generation, producing approximately 1-week generation time to reach max:
/// - Time to max = night_dust_ratio / generation_decay_rate.
/// - ≈ 7 days ≈ 1 week.
///
/// ## dust_grace_period (seconds)
/// Maximum time window allowed for DUST spends (3 hours) to prevent transactions from
/// living indefinitely while still accommodating network congestion.
///
/// # Usage
/// Use [`crate::domain::ledger::dust_parameters`] to get the parameters for a specific
/// protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DustParameters {
    /// NIGHT to DUST ratio (SPECK per STAR).
    pub night_dust_ratio: u64,

    /// Generation decay rate (SPECK per STAR per second).
    pub generation_decay_rate: u32,

    /// DUST grace period in seconds.
    pub dust_grace_period: u64,
}

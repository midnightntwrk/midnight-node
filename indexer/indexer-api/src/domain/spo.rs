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

/// SPO identity information.
#[derive(Debug, Clone)]
pub struct SpoIdentity {
    pub pool_id_hex: String,
    pub mainchain_pubkey_hex: String,
    pub sidechain_pubkey_hex: String,
    pub aura_pubkey_hex: Option<String>,
    pub validator_class: String,
}

/// Pool metadata from Cardano.
#[derive(Debug, Clone)]
pub struct PoolMetadata {
    pub pool_id_hex: String,
    pub hex_id: Option<String>,
    pub name: Option<String>,
    pub ticker: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
}

/// SPO with optional metadata.
#[derive(Debug, Clone)]
pub struct Spo {
    pub pool_id_hex: String,
    pub validator_class: String,
    pub sidechain_pubkey_hex: String,
    pub aura_pubkey_hex: Option<String>,
    pub name: Option<String>,
    pub ticker: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
}

/// Composite SPO data (identity + metadata + performance).
#[derive(Debug, Clone)]
pub struct SpoComposite {
    pub identity: Option<SpoIdentity>,
    pub metadata: Option<PoolMetadata>,
    pub performance: Vec<EpochPerf>,
}

/// SPO performance for an epoch.
#[derive(Debug, Clone)]
pub struct EpochPerf {
    pub epoch_no: i64,
    pub spo_sk_hex: String,
    pub produced: i64,
    pub expected: i64,
    pub identity_label: Option<String>,
    pub stake_snapshot: Option<String>,
    pub pool_id_hex: Option<String>,
    pub validator_class: Option<String>,
}

/// Current epoch information.
#[derive(Debug, Clone)]
pub struct EpochInfo {
    pub epoch_no: i64,
    pub duration_seconds: i64,
    pub elapsed_seconds: i64,
}

/// Committee member for an epoch.
#[derive(Debug, Clone)]
pub struct CommitteeMember {
    pub epoch_no: i64,
    pub position: i32,
    pub sidechain_pubkey_hex: String,
    pub expected_slots: i32,
    pub aura_pubkey_hex: Option<String>,
    pub pool_id_hex: Option<String>,
    pub spo_sk_hex: Option<String>,
}

/// Registration statistics for an epoch.
#[derive(Debug, Clone)]
pub struct RegisteredStat {
    pub epoch_no: i64,
    pub federated_valid_count: i64,
    pub federated_invalid_count: i64,
    pub registered_valid_count: i64,
    pub registered_invalid_count: i64,
    pub dparam: Option<f64>,
}

/// Cumulative registration totals for an epoch.
#[derive(Debug, Clone)]
pub struct RegisteredTotals {
    pub epoch_no: i64,
    pub total_registered: i64,
    pub newly_registered: i64,
}

/// Presence event for an SPO in an epoch.
#[derive(Debug, Clone)]
pub struct PresenceEvent {
    pub epoch_no: i64,
    pub id_key: String,
    pub source: String,
    pub status: Option<String>,
}

/// First valid epoch for an SPO identity.
#[derive(Debug, Clone)]
pub struct FirstValidEpoch {
    pub id_key: String,
    pub first_valid_epoch: i64,
}

/// Stake share information for an SPO.
#[derive(Debug, Clone)]
pub struct StakeShare {
    pub pool_id_hex: String,
    pub name: Option<String>,
    pub ticker: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
    pub live_stake: Option<String>,
    pub active_stake: Option<String>,
    pub live_delegators: Option<i64>,
    pub live_saturation: Option<f64>,
    pub declared_pledge: Option<String>,
    pub live_pledge: Option<String>,
    pub stake_share: Option<f64>,
}

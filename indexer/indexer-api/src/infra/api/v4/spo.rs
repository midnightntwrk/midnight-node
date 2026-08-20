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

use crate::domain::spo::{
    CommitteeMember as DomainCommitteeMember, EpochInfo as DomainEpochInfo,
    EpochPerf as DomainEpochPerf, FirstValidEpoch as DomainFirstValidEpoch,
    PoolMetadata as DomainPoolMetadata, PresenceEvent as DomainPresenceEvent,
    RegisteredStat as DomainRegisteredStat, RegisteredTotals as DomainRegisteredTotals,
    Spo as DomainSpo, SpoComposite as DomainSpoComposite, SpoIdentity as DomainSpoIdentity,
    StakeShare as DomainStakeShare,
};
use async_graphql::SimpleObject;

/// SPO identity information.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct SpoIdentity {
    pub pool_id_hex: String,
    pub mainchain_pubkey_hex: String,
    pub sidechain_pubkey_hex: String,
    pub aura_pubkey_hex: Option<String>,
    pub validator_class: String,
}

impl From<DomainSpoIdentity> for SpoIdentity {
    fn from(d: DomainSpoIdentity) -> Self {
        Self {
            pool_id_hex: d.pool_id_hex,
            mainchain_pubkey_hex: d.mainchain_pubkey_hex,
            sidechain_pubkey_hex: d.sidechain_pubkey_hex,
            aura_pubkey_hex: d.aura_pubkey_hex,
            validator_class: d.validator_class,
        }
    }
}

/// Pool metadata from Cardano.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct PoolMetadata {
    pub pool_id_hex: String,
    pub hex_id: Option<String>,
    pub name: Option<String>,
    pub ticker: Option<String>,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
}

impl From<DomainPoolMetadata> for PoolMetadata {
    fn from(d: DomainPoolMetadata) -> Self {
        Self {
            pool_id_hex: d.pool_id_hex,
            hex_id: d.hex_id,
            name: d.name,
            ticker: d.ticker,
            homepage_url: d.homepage_url,
            logo_url: d.logo_url,
        }
    }
}

/// SPO with optional metadata.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
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

impl From<DomainSpo> for Spo {
    fn from(d: DomainSpo) -> Self {
        Self {
            pool_id_hex: d.pool_id_hex,
            validator_class: d.validator_class,
            sidechain_pubkey_hex: d.sidechain_pubkey_hex,
            aura_pubkey_hex: d.aura_pubkey_hex,
            name: d.name,
            ticker: d.ticker,
            homepage_url: d.homepage_url,
            logo_url: d.logo_url,
        }
    }
}

/// Composite SPO data (identity + metadata + performance).
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct SpoComposite {
    pub identity: Option<SpoIdentity>,
    pub metadata: Option<PoolMetadata>,
    pub performance: Vec<EpochPerf>,
}

impl From<DomainSpoComposite> for SpoComposite {
    fn from(d: DomainSpoComposite) -> Self {
        Self {
            identity: d.identity.map(Into::into),
            metadata: d.metadata.map(Into::into),
            performance: d.performance.into_iter().map(Into::into).collect(),
        }
    }
}

/// SPO performance for an epoch.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
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

impl From<DomainEpochPerf> for EpochPerf {
    fn from(d: DomainEpochPerf) -> Self {
        Self {
            epoch_no: d.epoch_no,
            spo_sk_hex: d.spo_sk_hex,
            produced: d.produced,
            expected: d.expected,
            identity_label: d.identity_label,
            stake_snapshot: d.stake_snapshot,
            pool_id_hex: d.pool_id_hex,
            validator_class: d.validator_class,
        }
    }
}

/// Current epoch information.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct EpochInfo {
    pub epoch_no: i64,
    pub duration_seconds: i64,
    pub elapsed_seconds: i64,
}

impl From<DomainEpochInfo> for EpochInfo {
    fn from(d: DomainEpochInfo) -> Self {
        Self {
            epoch_no: d.epoch_no,
            duration_seconds: d.duration_seconds,
            elapsed_seconds: d.elapsed_seconds,
        }
    }
}

/// Committee member for an epoch.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct CommitteeMember {
    pub epoch_no: i64,
    pub position: i32,
    pub sidechain_pubkey_hex: String,
    pub expected_slots: i32,
    pub aura_pubkey_hex: Option<String>,
    pub pool_id_hex: Option<String>,
    pub spo_sk_hex: Option<String>,
}

impl From<DomainCommitteeMember> for CommitteeMember {
    fn from(d: DomainCommitteeMember) -> Self {
        Self {
            epoch_no: d.epoch_no,
            position: d.position,
            sidechain_pubkey_hex: d.sidechain_pubkey_hex,
            expected_slots: d.expected_slots,
            aura_pubkey_hex: d.aura_pubkey_hex,
            pool_id_hex: d.pool_id_hex,
            spo_sk_hex: d.spo_sk_hex,
        }
    }
}

/// Registration statistics for an epoch.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct RegisteredStat {
    pub epoch_no: i64,
    pub federated_valid_count: i64,
    pub federated_invalid_count: i64,
    pub registered_valid_count: i64,
    pub registered_invalid_count: i64,
    pub dparam: Option<f64>,
}

impl From<DomainRegisteredStat> for RegisteredStat {
    fn from(d: DomainRegisteredStat) -> Self {
        Self {
            epoch_no: d.epoch_no,
            federated_valid_count: d.federated_valid_count,
            federated_invalid_count: d.federated_invalid_count,
            registered_valid_count: d.registered_valid_count,
            registered_invalid_count: d.registered_invalid_count,
            dparam: d.dparam,
        }
    }
}

/// Cumulative registration totals for an epoch.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct RegisteredTotals {
    pub epoch_no: i64,
    pub total_registered: i64,
    pub newly_registered: i64,
}

impl From<DomainRegisteredTotals> for RegisteredTotals {
    fn from(d: DomainRegisteredTotals) -> Self {
        Self {
            epoch_no: d.epoch_no,
            total_registered: d.total_registered,
            newly_registered: d.newly_registered,
        }
    }
}

/// Presence event for an SPO in an epoch.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct PresenceEvent {
    pub epoch_no: i64,
    pub id_key: String,
    pub source: String,
    pub status: Option<String>,
}

impl From<DomainPresenceEvent> for PresenceEvent {
    fn from(d: DomainPresenceEvent) -> Self {
        Self {
            epoch_no: d.epoch_no,
            id_key: d.id_key,
            source: d.source,
            status: d.status,
        }
    }
}

/// First valid epoch for an SPO identity.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct FirstValidEpoch {
    pub id_key: String,
    pub first_valid_epoch: i64,
}

impl From<DomainFirstValidEpoch> for FirstValidEpoch {
    fn from(d: DomainFirstValidEpoch) -> Self {
        Self {
            id_key: d.id_key,
            first_valid_epoch: d.first_valid_epoch,
        }
    }
}

/// Stake share information for an SPO.
///
/// Values are sourced from mainchain pool data (e.g., Blockfrost) and keyed by Cardano pool_id.
#[derive(SimpleObject)]
#[graphql(rename_fields = "camelCase")]
pub struct StakeShare {
    /// Cardano pool ID (56-character hex string).
    pub pool_id_hex: String,
    /// Pool name from metadata.
    pub name: Option<String>,
    /// Pool ticker from metadata.
    pub ticker: Option<String>,
    /// Pool homepage URL from metadata.
    pub homepage_url: Option<String>,
    /// Pool logo URL from metadata.
    pub logo_url: Option<String>,
    /// Current live stake in lovelace.
    pub live_stake: Option<String>,
    /// Current active stake in lovelace.
    pub active_stake: Option<String>,
    /// Number of live delegators.
    pub live_delegators: Option<i64>,
    /// Saturation ratio (0.0 to 1.0+).
    pub live_saturation: Option<f64>,
    /// Declared pledge in lovelace.
    pub declared_pledge: Option<String>,
    /// Current live pledge in lovelace.
    pub live_pledge: Option<String>,
    /// Stake share as a fraction of total stake.
    pub stake_share: Option<f64>,
}

impl From<DomainStakeShare> for StakeShare {
    fn from(d: DomainStakeShare) -> Self {
        Self {
            pool_id_hex: d.pool_id_hex,
            name: d.name,
            ticker: d.ticker,
            homepage_url: d.homepage_url,
            logo_url: d.logo_url,
            live_stake: d.live_stake,
            active_stake: d.active_stake,
            live_delegators: d.live_delegators,
            live_saturation: d.live_saturation,
            declared_pledge: d.declared_pledge,
            live_pledge: d.live_pledge,
            stake_share: d.stake_share,
        }
    }
}

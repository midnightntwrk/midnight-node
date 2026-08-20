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

use crate::{
    domain::dust::DustGenerations as DomainDustGenerations,
    infra::api::v4::{
        AddressType, CardanoNetworkId, CardanoRewardAddress, HexEncodable, HexEncoded,
        dust::DustAddress, encode_address, encode_cardano_reward_address,
    },
};
use async_graphql::SimpleObject;
use indexer_common::domain::NetworkId;

/// Dust generations for a Cardano reward address.
#[derive(Debug, Clone, SimpleObject)]
pub struct DustGenerations {
    /// The Bech32-encoded Cardano reward address.
    pub cardano_reward_address: CardanoRewardAddress,

    /// All active registrations with aggregated generation stats.
    pub registrations: Vec<DustRegistration>,
}

/// A single dust registration with aggregated generation stats.
#[derive(Debug, Clone, SimpleObject)]
pub struct DustRegistration {
    /// The Bech32m-encoded DUST address.
    pub dust_address: DustAddress,

    /// Whether this registration is valid.
    pub valid: bool,

    /// NIGHT balance backing generation in STAR.
    pub night_balance: String,

    /// DUST generation rate in SPECK per second.
    pub generation_rate: String,

    /// Maximum DUST capacity in SPECK.
    pub max_capacity: String,

    /// Current generated DUST capacity in SPECK.
    pub current_capacity: String,

    /// Cardano UTXO transaction hash.
    pub utxo_tx_hash: Option<HexEncoded>,

    /// Cardano UTXO output index.
    pub utxo_output_index: Option<u32>,
}

impl DustGenerations {
    pub fn from_domain(dust_generations: DomainDustGenerations, network_id: &NetworkId) -> Self {
        let cardano_network_id = CardanoNetworkId::from(network_id);
        let cardano_reward_address = CardanoRewardAddress(encode_cardano_reward_address(
            dust_generations.cardano_reward_address,
            cardano_network_id,
        ));

        let registrations = dust_generations
            .registrations
            .into_iter()
            .map(|r| {
                let dust_address = DustAddress(encode_address(
                    r.dust_address,
                    AddressType::Dust,
                    network_id,
                ));

                DustRegistration {
                    dust_address,
                    valid: r.valid,
                    night_balance: r.night_balance.to_string(),
                    generation_rate: r.generation_rate.to_string(),
                    max_capacity: r.max_capacity.to_string(),
                    current_capacity: r.current_capacity.to_string(),
                    utxo_tx_hash: r.utxo_tx_hash.map(|h| h.hex_encode()),
                    utxo_output_index: r.utxo_output_index,
                }
            })
            .collect();

        Self {
            cardano_reward_address,
            registrations,
        }
    }
}

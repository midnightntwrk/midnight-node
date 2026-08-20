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

//! GraphQL API types for DUST operations. Currently only supports the dustGenerationStatus query.

use crate::{
    domain,
    infra::api::v4::{
        AddressType, CardanoNetworkId, CardanoRewardAddress, DecodeAddressError, HexEncodable,
        HexEncoded, decode_address, encode_address, encode_cardano_reward_address,
    },
};
use async_graphql::{SimpleObject, scalar};
use indexer_common::domain::{DustPublicKey, NetworkId};
use serde::{Deserialize, Serialize};

/// Bech32m-encoded DUST address.
/// The format depends on the network ID:
/// - Mainnet: `mn_dust` + bech32m data (no network ID suffix)
/// - Other networks: `mn_dust_` + network-id + bech32m data
///
/// DUST addresses are variable length (up to 33 bytes) as they encode a
/// Scale-encoded compact bigint representing the DUST public key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DustAddress(pub String);

scalar!(DustAddress);

impl DustAddress {
    /// Bech32m-decode this dust address into its raw bytes, validating the
    /// network-aware HRP (`mn_dust` on mainnet, `mn_dust_<network>` elsewhere).
    pub fn try_into_domain(
        &self,
        network_id: &NetworkId,
    ) -> Result<DustPublicKey, DecodeAddressError> {
        decode_address(&self.0, AddressType::Dust, network_id)
    }
}

/// DUST generation status for a specific Cardano reward address.
#[derive(Debug, Clone, SimpleObject)]
pub struct DustGenerationStatus {
    /// The Bech32-encoded Cardano reward address (e.g., stake_test1... or stake1...).
    pub cardano_reward_address: CardanoRewardAddress,

    /// The Bech32m-encoded associated DUST address if registered.
    pub dust_address: Option<DustAddress>,

    /// Whether this reward address is registered.
    pub registered: bool,

    /// NIGHT balance backing generation in STAR.
    pub night_balance: String,

    /// DUST generation rate in SPECK per second.
    pub generation_rate: String,

    /// Maximum DUST capacity in SPECK.
    pub max_capacity: String,

    /// Current generated DUST capacity in SPECK.
    pub current_capacity: String,

    /// Cardano UTXO transaction hash for update/unregister operations.
    pub utxo_tx_hash: Option<HexEncoded>,

    /// Cardano UTXO output index for update/unregister operations.
    pub utxo_output_index: Option<u32>,
}

impl From<(domain::DustGenerationStatus, &NetworkId)> for DustGenerationStatus {
    fn from((status, network_id): (domain::DustGenerationStatus, &NetworkId)) -> Self {
        let cardano_network_id = CardanoNetworkId::from(network_id);
        let cardano_reward_address = CardanoRewardAddress(encode_cardano_reward_address(
            status.cardano_reward_address,
            cardano_network_id,
        ));
        let dust_address = status
            .dust_address
            .map(|addr| DustAddress(encode_address(addr, AddressType::Dust, network_id)));

        Self {
            cardano_reward_address,
            dust_address,
            registered: status.registered,
            night_balance: status.night_balance.to_string(),
            generation_rate: status.generation_rate.to_string(),
            max_capacity: status.max_capacity.to_string(),
            current_capacity: status.current_capacity.to_string(),
            utxo_tx_hash: status.utxo_tx_hash.map(|h| h.hex_encode()),
            utxo_output_index: status.utxo_output_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::api::v4::{AddressType, dust::DustAddress, encode_address};
    use indexer_common::domain::ByteVec;

    #[test]
    fn test_try_into_domain_round_trip() {
        let network_id = "undeployed".try_into().unwrap();
        let bytes = ByteVec::from(vec![1, 2, 3, 4, 5]);
        let dust_address = DustAddress(encode_address(&bytes, AddressType::Dust, &network_id));

        let decoded = dust_address.try_into_domain(&network_id).unwrap();
        assert_eq!(decoded.as_ref(), bytes.as_ref());
    }

    #[test]
    fn test_try_into_domain_rejects_hex() {
        let network_id = "undeployed".try_into().unwrap();
        let dust_address = DustAddress("1234567890abcdef".to_string());
        assert!(dust_address.try_into_domain(&network_id).is_err());
    }

    #[test]
    fn test_try_into_domain_rejects_wrong_network_hrp() {
        let undeployed = "undeployed".try_into().unwrap();
        let preview = "preview".try_into().unwrap();
        let bytes = ByteVec::from(vec![1, 2, 3, 4, 5]);
        let dust_address = DustAddress(encode_address(&bytes, AddressType::Dust, &undeployed));

        assert!(dust_address.try_into_domain(&preview).is_err());
    }
}

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

use async_graphql::scalar;
use derive_more::{Display, derive::From};
use fastrace::trace;
use indexer_common::domain::{
    AddressType, DecodeAddressError, NetworkId, ProtocolVersion, decode_address, ledger,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Bech32m-encoded viewing key.
#[derive(Debug, Display, Clone, PartialEq, Eq, Serialize, Deserialize, From)]
#[from(&str)]
pub struct ViewingKey(pub String);

scalar!(ViewingKey);

impl ViewingKey {
    /// Converts this API viewing key into a domain viewing key, validating the bech32m format and
    /// network ID and deserializing the bech32m data.
    ///
    /// Format expectations:
    /// - For mainnet: "mn_shield-esk" + bech32m data
    /// - For other networks: "mn_shield-esk_" + network-id + bech32m data where network-id is one
    ///   of: "dev", "test", "undeployed"
    #[trace(properties = {
        "network_id": "{network_id}",
        "protocol_version": "{protocol_version}"
    })]
    pub fn try_into_domain(
        self,
        network_id: NetworkId,
        protocol_version: ProtocolVersion,
    ) -> Result<indexer_common::domain::ViewingKey, ViewingKeyFormatError> {
        let bytes = decode_address(&self.0, AddressType::SecretEncryptionKey, network_id)?;
        let secret_key = ledger::SecretKey::deserialize(bytes, protocol_version)?;
        let viewing_key = secret_key.expose_secret().into();

        Ok(viewing_key)
    }
}

#[derive(Debug, Error)]
pub enum ViewingKeyFormatError {
    #[error("cannot bech32m-decode viewing key")]
    Decode(#[from] DecodeAddressError),

    #[error(transparent)]
    Ledger(#[from] ledger::Error),
}

#[cfg(test)]
mod tests {
    use crate::infra::api::v1::viewing_key::ViewingKey;
    use indexer_common::domain::{NetworkId, PROTOCOL_VERSION_000_016_000};

    #[test]
    fn test_try_into_domain() {
        let viewing_key = ViewingKey::from(
            "mn_shield-esk_undeployed1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcsmxqc0u", /* "mn_shield-esk_undeployed1dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcs9ete5h", */
        );
        let domain_viewing_key =
            viewing_key.try_into_domain(NetworkId::Undeployed, PROTOCOL_VERSION_000_016_000);
        println!("{domain_viewing_key:?}");
        assert!(domain_viewing_key.is_ok());
    }
}

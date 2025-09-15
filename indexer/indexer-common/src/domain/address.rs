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

use crate::domain::{ByteVec, NetworkId};
use bech32::{Bech32m, Hrp};
use thiserror::Error;

/// A Midnight address type.
pub enum AddressType {
    Unshielded,
    SecretEncryptionKey,
}

impl AddressType {
    fn hrp(&self, network_id: NetworkId) -> String {
        let suffix = match network_id {
            NetworkId::Undeployed => "_undeployed",
            NetworkId::DevNet => "_dev",
            NetworkId::TestNet => "_test",
            NetworkId::MainNet => "",
        };
        format!("{}{suffix}", self.hrp_prefix())
    }

    fn hrp_prefix(&self) -> &'static str {
        match self {
            AddressType::Unshielded => "mn_addr",
            AddressType::SecretEncryptionKey => "mn_shield-esk",
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeAddressError {
    #[error("cannot bech32m-decode address")]
    Decode(#[from] bech32::DecodeError),

    #[error("expected HRP {expected_hrp}, but was {hrp}")]
    InvalidHrp { expected_hrp: String, hrp: String },
}

#[derive(Debug, Error)]
pub enum EncodeAddressError {
    #[error("cannot bech32m-encode address")]
    Encode(#[from] bech32::EncodeError),

    #[error("expected HRP {expected_hrp}, but was {hrp}")]
    InvalidHrp { expected_hrp: String, hrp: String },
}

pub fn decode_address(
    address: impl AsRef<str>,
    address_type: AddressType,
    network_id: NetworkId,
) -> Result<ByteVec, DecodeAddressError> {
    let (hrp, bytes) = bech32::decode(address.as_ref())?;

    let expected_hrp = address_type.hrp(network_id);
    if hrp.as_str() != expected_hrp {
        let hrp = hrp.to_string();
        return Err(DecodeAddressError::InvalidHrp { expected_hrp, hrp });
    }

    Ok(bytes.into())
}

pub fn encode_address(
    address: impl AsRef<[u8]>,
    address_type: AddressType,
    network_id: NetworkId,
) -> String {
    let hrp = Hrp::parse(&address_type.hrp(network_id)).expect("HRP for address can be parsed");
    bech32::encode::<Bech32m>(hrp, address.as_ref())
        .expect("bytes for unshielded address can be Bech32m-encoded")
}

#[cfg(test)]
mod tests {
    use crate::domain::{AddressType, ByteVec, NetworkId, decode_address, encode_address};
    use assert_matches::assert_matches;

    #[test]
    fn test_encode_decode_address() {
        let address = ByteVec::from(vec![0, 1, 2, 3]);
        let encoded = encode_address(
            &address,
            AddressType::SecretEncryptionKey,
            NetworkId::Undeployed,
        );
        let decoded = decode_address(
            encoded,
            AddressType::SecretEncryptionKey,
            NetworkId::Undeployed,
        );
        assert_matches!(decoded, Ok(a) if a == address);

        let address = ByteVec::from(vec![0, 1, 2, 3]);
        let encoded = encode_address(&address, AddressType::Unshielded, NetworkId::MainNet);
        let decoded = decode_address(encoded, AddressType::Unshielded, NetworkId::MainNet);
        assert_matches!(decoded, Ok(a) if a == address);
    }
}

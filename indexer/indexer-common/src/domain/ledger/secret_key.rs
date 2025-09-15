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

use crate::domain::{
    ByteArray, PROTOCOL_VERSION_000_016_000, ProtocolVersion, VIEWING_KEY_LEN, ledger::Error,
};
use fastrace::trace;
use midnight_serialize_v6::tagged_deserialize as tagged_deserialize_v6;
use midnight_transient_crypto_v6::encryption::SecretKey as SecretKeyV6;

/// Facade for `SecretKey` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum SecretKey {
    V6(SecretKeyV6),
}

impl SecretKey {
    /// Deserialize the given serialized secret key using the given protocol version.
    #[trace(properties = { "protocol_version": "{protocol_version}" })]
    pub fn deserialize(
        secret_key: impl AsRef<[u8]>,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, Error> {
        if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
            let secret_key = tagged_deserialize_v6(&mut secret_key.as_ref())
                .map_err(|error| Error::Io("cannot deserialize SecretKeyV6", error))?;
            Ok(Self::V6(secret_key))
        } else {
            Err(Error::InvalidProtocolVersion(protocol_version))
        }
    }

    /// Get the repr of this secret key.
    pub fn expose_secret(&self) -> ByteArray<VIEWING_KEY_LEN> {
        match self {
            Self::V6(secret_key) => secret_key.repr().into(),
        }
    }
}

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

use crate::domain::{ByteArray, VIEWING_KEY_LEN, ledger::Error};
use fastrace::trace;
use midnight_serialize_v1::Deserializable;

#[derive(Debug, Clone)]
pub struct SecretKey(midnight_transient_crypto_v2::encryption::SecretKey);

impl SecretKey {
    #[trace]
    pub fn deserialize(secret_key: impl AsRef<[u8]>) -> Result<Self, Error> {
        let inner = midnight_transient_crypto_v2::encryption::SecretKey::deserialize(
            &mut secret_key.as_ref(),
            0,
        )
        .map_err(|error| Error::Deserialize("SecretKey", error))?;
        Ok(Self(inner))
    }

    /// Get the repr of this secret key.
    pub fn expose_secret(&self) -> ByteArray<VIEWING_KEY_LEN> {
        self.0.repr().into()
    }
}

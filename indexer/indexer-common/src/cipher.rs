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

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

/// Make a `ChaCha20Poly1305` cipher from the given hex-encoded secret key. Only the first 32
/// bytes are considered.
pub fn make_cipher(secret: SecretString) -> Result<ChaCha20Poly1305, Error> {
    let secret = const_hex::decode(secret.expose_secret())?;
    if secret.len() < 32 {
        return Err(Error::InvalidLen(secret.len()));
    }

    let key = Key::clone_from_slice(&secret[0..32]);
    let cipher = ChaCha20Poly1305::new(&key);

    Ok(cipher)
}

/// Error possibly returned by [make_cipher].
#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot hex-decode secret")]
    Decode(#[from] const_hex::FromHexError),

    #[error("secret must be at least 32 bytes long, but was {0}")]
    InvalidLen(usize),
}

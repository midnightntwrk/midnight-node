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

use crate::domain::{ByteArray, ByteArrayLenError, ByteVec};
use chacha20poly1305::{
    AeadCore, ChaCha20Poly1305,
    aead::{Aead, OsRng, Payload},
};
use derive_more::From;
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;
use std::fmt::{self, Debug, Display};
use thiserror::Error;

pub type SessionId = ByteArray<32>;

pub const VIEWING_KEY_LEN: usize = 32;

/// A secret key that is encrypted at rest.
/// Attention: Do not accidentally leak the secret!
#[derive(Clone, Copy, PartialEq, Eq, Hash, From)]
#[from(forward)]
pub struct ViewingKey(ByteArray<VIEWING_KEY_LEN>);

impl ViewingKey {
    /// Expose the sercret.
    pub fn expose_secret(&self) -> ByteArray<VIEWING_KEY_LEN> {
        self.0
    }

    /// Try to decrypt the given bytes as viewing key using ChaCha20Poly1305 AEAD with the given
    /// nonce and ciphertext and the given wallet ID.
    pub fn decrypt(
        nonce_and_ciphertext: impl AsRef<[u8]>,
        wallet_id: Uuid,
        cipher: &ChaCha20Poly1305,
    ) -> Result<Self, DecryptViewingKeyError> {
        let nonce_and_ciphertext = nonce_and_ciphertext.as_ref();

        let nonce = &nonce_and_ciphertext[0..12];
        let ciphertext = &nonce_and_ciphertext[12..];

        let payload = Payload {
            msg: ciphertext,
            aad: wallet_id.as_bytes(),
        };
        let bytes = cipher.decrypt(nonce.into(), payload)?.try_into()?;

        Ok(Self(bytes))
    }

    /// Encrypt this viewing key using ChaCha20Poly1305 AEAD.
    pub fn encrypt(
        &self,
        id: Uuid,
        cipher: &ChaCha20Poly1305,
    ) -> Result<ByteVec, chacha20poly1305::Error> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

        let payload = Payload {
            msg: &self.0.0,
            aad: id.as_bytes(),
        };
        let mut ciphertext = cipher.encrypt(&nonce, payload)?;

        let mut nonce_and_ciphertext = nonce.to_vec();
        nonce_and_ciphertext.append(&mut ciphertext);

        Ok(nonce_and_ciphertext.into())
    }

    /// Return the session ID (Sha256 hash) for this viewing key.
    pub fn to_session_id(&self) -> SessionId {
        let mut hasher = Sha256::new();
        hasher.update(self.0);
        let session_id = hasher.finalize();

        <[u8; 32]>::from(session_id).into()
    }
}

impl Debug for ViewingKey {
    /// Attention: Do not leak the secret!
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ViewingKey(REDACTED)")
    }
}

impl Display for ViewingKey {
    /// Attention: Do not leak the secret!
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "REDACTED")
    }
}

#[derive(Debug, Error)]
pub enum DecryptViewingKeyError {
    #[error("cannot decrypt secret")]
    DecryptViewingKeyError(#[from] chacha20poly1305::Error),

    #[error("cannot convert into viewing key")]
    ByteArrayLen(#[from] ByteArrayLenError),
}

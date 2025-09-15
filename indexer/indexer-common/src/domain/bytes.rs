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

use derive_more::{AsRef, Deref, From, Into};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::fmt::{self, Debug, Display};
use thiserror::Error;

/// A newtype for a byte vector implementing various traits, amongst others `Debug` and `Display`
/// returning a hex-encoded string, the former no longer than nine characters.
#[derive(
    Default, Clone, PartialEq, Eq, Hash, AsRef, Deref, From, Into, Serialize, Deserialize, Type,
)]
#[as_ref([u8])]
#[from(Vec<u8>, &[u8])]
#[sqlx(transparent)]
pub struct ByteVec(#[serde(with = "const_hex")] pub Vec<u8>);

impl Debug for ByteVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug(self, f)
    }
}

impl Display for ByteVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display(self, f)
    }
}

/// A newtype for a byte array implementing various traits, amongst others `Debug` and `Display`
/// returning a hex-encoded string, the former no longer than nine characters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, AsRef, Deref, From, Into, Serialize, Deserialize)]
#[as_ref([u8])]
#[into([u8; N], Vec<u8>)]
pub struct ByteArray<const N: usize>(#[serde(with = "const_hex")] pub [u8; N]);

impl<const N: usize> Default for ByteArray<N> {
    /// A byte array of length N filled with `0`s.
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> TryFrom<&[u8]> for ByteArray<N> {
    type Error = ByteArrayLenError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        bytes
            .try_into()
            .map_err(|_| ByteArrayLenError(N, bytes.len()))
            .map(Self)
    }
}

impl<const N: usize> TryFrom<Vec<u8>> for ByteArray<N> {
    type Error = ByteArrayLenError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        bytes.as_slice().try_into()
    }
}

impl<const N: usize> TryFrom<ByteVec> for ByteArray<N> {
    type Error = ByteArrayLenError;

    fn try_from(bytes: ByteVec) -> Result<Self, Self::Error> {
        bytes.as_ref().try_into()
    }
}

impl<const N: usize> Debug for ByteArray<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug(self, f)
    }
}

impl<const N: usize> Display for ByteArray<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display(self, f)
    }
}

#[cfg(feature = "cloud")]
impl<const N: usize> Type<sqlx::Postgres> for ByteArray<N> {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <[u8; N] as Type<sqlx::Postgres>>::type_info()
    }
}

#[cfg(feature = "standalone")]
impl<const N: usize> Type<sqlx::Sqlite> for ByteArray<N> {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Sqlite>>::type_info()
    }
}

#[cfg(feature = "cloud")]
impl<'r, const N: usize> sqlx::Decode<'r, sqlx::Postgres> for ByteArray<N> {
    fn decode(
        value: <sqlx::Postgres as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        <[u8; N] as sqlx::Decode<'r, sqlx::Postgres>>::decode(value).map(Self)
    }
}

#[cfg(feature = "standalone")]
impl<'r, const N: usize> sqlx::Decode<'r, sqlx::Sqlite> for ByteArray<N> {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        <&[u8] as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)
            .and_then(|bytes| ByteArray::try_from(bytes).map_err(|error| error.into()))
    }
}

#[derive(Debug, Error)]
#[error("cannot create byte array of len {0} from input of len {1}")]
pub struct ByteArrayLenError(usize, usize);

fn debug<T>(bytes: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result
where
    T: AsRef<[u8]>,
{
    let hex_encoded = const_hex::encode(bytes);

    if hex_encoded.len() <= 8 {
        write!(f, "{hex_encoded}")
    } else {
        write!(f, "{}…", &hex_encoded[0..8])
    }
}

fn display<T>(bytes: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result
where
    T: AsRef<[u8]>,
{
    let hex_encoded = const_hex::encode(bytes);
    write!(f, "{hex_encoded}")
}

#[cfg(test)]
mod tests {
    use crate::domain::{ByteArray, ByteVec};

    #[test]
    fn test_byte_vec() {
        let bytes = ByteVec::default();
        assert_eq!(format!("{bytes:?}"), "");
        assert_eq!(format!("{bytes}"), "");

        let bytes = ByteVec::from([0, 1, 2, 3].as_slice());
        assert_eq!(format!("{bytes:?}"), "00010203");
        assert_eq!(format!("{bytes}"), "00010203");

        let bytes = ByteVec::from(vec![0, 1, 2, 3, 4]);
        assert_eq!(format!("{bytes:?}"), "00010203…");
        assert_eq!(format!("{bytes}"), "0001020304");
    }

    #[test]
    fn test_byte_array() {
        let bytes = ByteArray::from([]);
        assert_eq!(format!("{bytes:?}"), "");
        assert_eq!(format!("{bytes}"), "");

        let bytes = ByteArray::from([0, 1, 2, 3]);
        assert_eq!(format!("{bytes:?}"), "00010203");
        assert_eq!(format!("{bytes}"), "00010203");

        let bytes = ByteArray::from([0, 1, 2, 3, 4]);
        assert_eq!(format!("{bytes:?}"), "00010203…");
        assert_eq!(format!("{bytes}"), "0001020304");
    }
}

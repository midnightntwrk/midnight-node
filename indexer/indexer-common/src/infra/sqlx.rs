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

#[cfg_attr(docsrs, doc(cfg(feature = "cloud")))]
#[cfg(feature = "cloud")]
pub mod postgres;
#[cfg_attr(docsrs, doc(cfg(feature = "standalone")))]
#[cfg(feature = "standalone")]
mod sqlite;

use crate::domain::{ByteArray, ByteArrayLenError};
use serde::{Deserialize, Serialize};
use sqlx::{Database, Decode, Type, error::BoxDynError};

/// A helper to use `Option<T>` where T does not implement `sqlx::Type` but a `TryFrom` into a
/// supported type with `sqlx::FromRow` like this:
/// Newtype for a byte array representing the big-endian encoded bytes of a `u128`. This can be used
/// as a helper to use `u128` with `sqlx::FromRow` like this:
/// ```
/// #[sqlx(try_from = "U128BeBytes")]
/// ```
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct U128BeBytes(pub [u8; 16]);

impl From<u128> for U128BeBytes {
    fn from(value: u128) -> Self {
        Self(value.to_be_bytes())
    }
}

impl From<U128BeBytes> for u128 {
    fn from(value: U128BeBytes) -> Self {
        u128::from_be_bytes(value.0)
    }
}

/// A helper to use `Option<u64>` with `sqlx::FromRow` like this:
/// ```ignore
/// #[sqlx(try_from = "SqlxOption<&'a [u8]>")]
/// pub author: Option<ByteArray>,
/// ```
pub struct SqlxOption<T>(Option<T>);

impl<T> From<SqlxOption<T>> for Option<T> {
    fn from(value: SqlxOption<T>) -> Self {
        value.0
    }
}

impl<T, D> Type<D> for SqlxOption<T>
where
    T: Type<D>,
    D: Database,
{
    fn type_info() -> D::TypeInfo {
        Option::<T>::type_info()
    }
}

impl<'r, T, D> Decode<'r, D> for SqlxOption<T>
where
    T: Decode<'r, D>,
    D: Database,
{
    fn decode(value: D::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let option = Option::<T>::decode(value)?;
        Ok(Self(option))
    }
}

impl TryFrom<SqlxOption<i64>> for Option<u64> {
    type Error = BoxDynError;

    fn try_from(value: SqlxOption<i64>) -> Result<Self, Self::Error> {
        let value = value.0.map(TryInto::try_into).transpose()?;
        Ok(value)
    }
}

impl TryFrom<SqlxOption<U128BeBytes>> for Option<u128> {
    type Error = BoxDynError;

    fn try_from(value: SqlxOption<U128BeBytes>) -> Result<Self, Self::Error> {
        let value = value.0.map(Into::into);
        Ok(value)
    }
}

impl<const N: usize> TryFrom<SqlxOption<&[u8]>> for Option<ByteArray<N>> {
    type Error = ByteArrayLenError;

    fn try_from(value: SqlxOption<&[u8]>) -> Result<Self, Self::Error> {
        let value = value.0.map(TryInto::try_into).transpose()?;
        Ok(value)
    }
}

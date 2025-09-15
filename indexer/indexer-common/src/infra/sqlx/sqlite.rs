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

use crate::infra::sqlx::U128BeBytes;
use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};

impl Type<Sqlite> for U128BeBytes {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <&[u8] as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for U128BeBytes {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Sqlite>>::decode(value)?;
        let bytes = bytes
            .try_into()
            .map_err(|_| format!("expected 16 bytes, but was {}", bytes.len()))?;
        Ok(Self(bytes))
    }
}

impl<'q> Encode<'q, Sqlite> for U128BeBytes {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = Box::from(self.0);
        <Box<[u8]> as Encode<'q, Sqlite>>::encode(bytes, buf)
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::{
        pool::sqlite::{Config, SqlitePool},
        sqlx::U128BeBytes,
    };
    use anyhow::Context;
    use sqlx::FromRow;
    use std::error::Error as StdError;

    #[tokio::test]
    async fn test_pool() -> Result<(), Box<dyn StdError>> {
        let pool = SqlitePool::new(Config::default())
            .await
            .context("create pool")?;

        sqlx::query("CREATE TABLE test (id BYTEA PRIMARY KEY)")
            .execute(&*pool)
            .await
            .context("create table")?;

        sqlx::query("INSERT INTO test (id) VALUES ($1)")
            .bind(U128BeBytes::from(42))
            .execute(&*pool)
            .await
            .context("insert into test table")?;

        let test = sqlx::query_as::<_, Test>("SELECT * FROM test LIMIT 1")
            .fetch_one(&*pool)
            .await
            .context("query test table")?;
        assert_eq!(test.id, 42);

        Ok(())
    }

    #[derive(Debug, FromRow)]
    struct Test {
        #[sqlx(try_from = "U128BeBytes")]
        id: u128,
    }
}

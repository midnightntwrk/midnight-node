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
use sqlx::{Database, Decode, Encode, Postgres, Type, encode::IsNull, error::BoxDynError};

impl Type<Postgres> for U128BeBytes {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <[u8; 16] as Type<Postgres>>::type_info()
    }
}

impl<'r> Decode<'r, Postgres> for U128BeBytes {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <[u8; 16] as Decode<Postgres>>::decode(value)?;
        Ok(Self(bytes))
    }
}

impl<'q> Encode<'q, Postgres> for U128BeBytes {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        self.0.encode_by_ref(buf)
    }
}

/// Maps a "deadlock_detected" PosgtreSQL error to the result of the given function wrapped in `Ok`
/// and passes all other errors along.
/// For "40P01" see <https://www.postgresql.org/docs/current/errcodes-appendix.html>.
pub fn ignore_deadlock_detected<F, T>(error: sqlx::Error, on_deadlock: F) -> Result<T, sqlx::Error>
where
    F: FnOnce() -> T,
{
    match error {
        sqlx::Error::Database(e) if e.code().as_deref() == Some("40P01") => Ok(on_deadlock()),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::{
        pool::postgres::{Config, PostgresPool},
        sqlx::U128BeBytes,
    };
    use anyhow::Context;
    use sqlx::{FromRow, postgres::PgSslMode};
    use std::{error::Error as StdError, time::Duration};
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::postgres::Postgres;

    #[tokio::test]
    async fn test_pool() -> Result<(), Box<dyn StdError>> {
        let postgres_container = Postgres::default()
            .with_db_name("indexer")
            .with_user("indexer")
            .with_password(env!("APP__INFRA__STORAGE__PASSWORD"))
            .with_tag("17.1-alpine")
            .start()
            .await
            .context("start Postgres container")?;
        let postgres_port = postgres_container
            .get_host_port_ipv4(5432)
            .await
            .context("get Postgres port")?;

        let config = Config {
            host: "localhost".to_string(),
            port: postgres_port,
            dbname: "indexer".to_string(),
            user: "indexer".to_string(),
            password: env!("APP__INFRA__STORAGE__PASSWORD").into(),
            sslmode: PgSslMode::Prefer,
            max_connections: 10,
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(5 * 60),
        };
        let pool = PostgresPool::new(config).await.context("create pool")?;

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

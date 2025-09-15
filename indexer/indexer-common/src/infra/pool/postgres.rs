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

use derive_more::Into;
use log::debug;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use std::{ops::Deref, time::Duration};
use thiserror::Error;

/// New type for `sqlx::PgPool`, allowing for some custom extensions as well as security.
///
/// To use as `&sqlx::PgPool` in `Query::execute`, use its `Deref` implementation: `&*pool` or
/// `pool.deref()`. If an owned `sqlx::PgPool` is needed, use `Into::into`.
#[derive(Debug, Clone, Into)]
pub struct PostgresPool(sqlx::PgPool);

impl PostgresPool {
    /// Try to create a new [PostgresPool] with the given config.
    pub async fn new(config: Config) -> Result<Self, Error> {
        let Config {
            host,
            port,
            dbname,
            user,
            password,
            sslmode,
            max_connections,
            idle_timeout,
            max_lifetime,
        } = config;

        let connect_options = PgConnectOptions::new()
            .host(&host)
            .database(&dbname)
            .username(&user)
            .password(password.expose_secret())
            .port(port)
            .ssl_mode(sslmode);

        let inner = PgPoolOptions::new()
            .max_connections(max_connections)
            .idle_timeout(Some(idle_timeout))
            .max_lifetime(max_lifetime)
            .connect_with(connect_options)
            .await?;
        let pool = PostgresPool(inner);
        debug!(pool:?; "created pool");

        Ok(pool)
    }
}

impl Deref for PostgresPool {
    type Target = sqlx::PgPool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Error possibly returned by [PostgresPool::new].
#[derive(Debug, Error)]
#[error("cannot create Postgres connection pool")]
pub struct Error(#[from] sqlx::Error);

/// Configuration for [PostgresPool].
#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub user: String,
    pub password: SecretString,
    #[serde_as(as = "DisplayFromStr")]
    pub sslmode: PgSslMode,
    pub max_connections: u32,
    #[serde(with = "humantime_serde")]
    pub idle_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub max_lifetime: Duration,
}

#[cfg(test)]
mod tests {
    use crate::infra::pool::postgres::{Config, PostgresPool};
    use anyhow::Context;
    use sqlx::postgres::PgSslMode;
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

        let pool = PostgresPool::new(config).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        let result = sqlx::query("CREATE TABLE test (id integer PRIMARY KEY)")
            .execute(&*pool)
            .await;
        assert!(result.is_ok());

        Ok(())
    }
}

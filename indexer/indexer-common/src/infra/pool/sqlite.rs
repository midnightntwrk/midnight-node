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
use serde::Deserialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::ops::Deref;
use thiserror::Error;

/// New type for `sqlx::SqlitePool`, allowing for some custom extensions as well as security.
///
/// To use as `&sqlx::SqlitePool` in `Query::execute`, use its `Deref` implementation: `&*pool` or
/// `pool.deref()`. If an owned `sqlx::SqlitePool` is needed, use `Into::into`.
#[derive(Debug, Clone, Into)]
pub struct SqlitePool(sqlx::SqlitePool);

impl SqlitePool {
    /// Try to create a new [SqlitePool] with the given config.
    pub async fn new(config: Config) -> Result<Self, Error> {
        let connect_options = config.try_into().map_err(Error::ConvertConfig)?;
        let inner = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(connect_options)
            .await?;
        let pool = SqlitePool(inner);
        debug!(pool:?; "created pool");

        Ok(pool)
    }
}

impl Deref for SqlitePool {
    type Target = sqlx::SqlitePool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Error possibly returned by [SqlitePool::new].
#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot convert config into sqlite connect options")]
    ConvertConfig(#[source] sqlx::Error),

    #[error("cannot create sqlite connection pool")]
    CreatePool(#[from] sqlx::Error),
}

/// Configuration for [SqlitePool].
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub cnn_url: String,
}

impl TryFrom<Config> for SqliteConnectOptions {
    type Error = sqlx::Error;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        let mut options = config.cnn_url.parse::<SqliteConnectOptions>()?;
        options = options.create_if_missing(true);
        Ok(options)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cnn_url: "sqlite::memory:".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::pool::sqlite::{Config, SqlitePool};
    use std::{ops::Deref, path::Path};
    use tokio::fs;

    #[tokio::test]
    async fn test_sqlite_pool_file_creation() {
        let db_path = "test_indexer.sqlite";

        if Path::new(db_path).exists() {
            fs::remove_file(db_path)
                .await
                .expect("Failed to remove existing test database file");
        }
        assert!(!Path::new(db_path).exists());

        let pool = SqlitePool::new(Config {
            cnn_url: format!("sqlite://{db_path}"),
        })
        .await;

        assert!(pool.is_ok());
        assert!(Path::new(db_path).exists());
        fs::remove_file(db_path)
            .await
            .expect("Failed to remove test database file");
    }

    #[tokio::test]
    async fn test_pool() {
        let pool = SqlitePool::new(Config::default()).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();

        let result = sqlx::query("CREATE TABLE test (id integer PRIMARY KEY)")
            .execute(pool.deref())
            .await;
        assert!(result.is_ok());
    }
}

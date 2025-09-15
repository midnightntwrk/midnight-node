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

use crate::infra::pool::postgres::PostgresPool;
use sqlx::migrate::MigrateError;
use thiserror::Error;

/// Run the database migrations for Postgres.
pub async fn run(pool: &PostgresPool) -> Result<(), Error> {
    sqlx::migrate!("migrations/postgres").run(&**pool).await?;
    Ok(())
}

/// Error possibly returned by [run].
#[derive(Debug, Error)]
#[error("cannot run migrations for postgres")]
pub struct Error(#[from] MigrateError);

#[cfg(test)]
mod tests {
    use crate::infra::{
        migrations::postgres::run,
        pool::{self, postgres::PostgresPool},
    };
    use anyhow::Context;
    use sqlx::{Row, postgres::PgSslMode};
    use std::{collections::HashSet, error::Error as StdError, time::Duration};
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::postgres::Postgres;

    #[tokio::test]
    async fn test_run() -> Result<(), Box<dyn StdError>> {
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

        let config = pool::postgres::Config {
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
        let pool = PostgresPool::new(config).await?;

        let result = run(&pool).await;
        assert!(result.is_ok());

        let table_names = sqlx::query(
            "SELECT tablename
             FROM pg_catalog.pg_tables
             WHERE schemaname = 'public'",
        )
        .fetch_all(&*pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>(0))
        .collect::<HashSet<_>>();

        assert!(table_names.contains("_sqlx_migrations"));

        Ok(())
    }
}

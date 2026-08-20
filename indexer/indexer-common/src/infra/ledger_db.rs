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

#[cfg_attr(docsrs, doc(cfg(any(feature = "cloud", feature = "standalone"))))]
#[cfg(any(feature = "cloud", feature = "standalone"))]
pub mod v1_1;

use serde::Deserialize;

#[cfg(feature = "cloud")]
pub fn init(config: Config, pool: crate::infra::pool::postgres::PostgresPool) {
    let Config { cache_max_nodes } = config;

    let db = v1_1::LedgerDb::new(pool);
    let _ = midnight_storage_core_v1::storage::set_default_storage(|| {
        midnight_storage_core_v1::Storage::new(cache_max_nodes, db)
    });
}

#[cfg(feature = "standalone")]
pub async fn init(config: Config) -> Result<(), Error> {
    use crate::infra::{migrations, pool::sqlite};

    let Config {
        cache_max_nodes,
        cnn_url,
    } = config;

    // storage-core assumes a single writer: `flush_*` reads root counts and
    // then writes new ones, and that read-then-write must observe its own
    // in-progress state. With max_connections > 1, sqlx can route the read to
    // a different connection whose WAL snapshot predates the writer, breaking
    // the invariant and producing "roots counts can't be negative" panics.
    //
    // `synchronous_full`: chain-indexer commits the ledger state BEFORE the block row in the
    // main DB and the resume path relies on the on-disk ledger DB being at least as new as
    // the main DB. With NORMAL both files fsync independently at checkpoints, so a power
    // loss could keep block N in the main DB while dropping N's ledger state here - the
    // startup filter would then silently seed from an older state and diverge. FULL keeps
    // the cross-file ordering: a ledger state is durable before its block row can be.
    let pool = sqlite::SqlitePool::new(sqlite::Config {
        cnn_url,
        max_connections: 1,
        synchronous_full: true,
    })
    .await?;
    migrations::sqlite::run_for_ledger_db(&pool).await?;

    let db = v1_1::LedgerDb::new(pool);
    let _ = midnight_storage_core_v1::storage::set_default_storage(|| {
        midnight_storage_core_v1::Storage::new(cache_max_nodes, db)
    });

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Maximum number of arena nodes held in the storage-core caches. This is a node *count*, not
    /// a byte size: storage-core's read cache is strictly bounded by it and its write cache is
    /// truncated to it on flush (see `midnight_storage_core_v1::Storage::new`, whose own default
    /// is `DEFAULT_CACHE_SIZE = 10_000`). `0` means unbounded.
    pub cache_max_nodes: usize,

    #[cfg(feature = "standalone")]
    pub cnn_url: String,
}

#[cfg(feature = "standalone")]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot create DB pool for SQLite")]
    CreatePool(#[from] crate::infra::pool::sqlite::Error),

    #[error("cannot run migrations for SQLite")]
    RunMigrations(#[from] crate::infra::migrations::sqlite::Error),
}

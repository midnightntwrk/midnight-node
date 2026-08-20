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

use crate::error::StdErrorExt;
use derive_more::Debug;
use futures::TryStreamExt;
use indoc::indoc;
use midnight_serialize_v1::{Deserializable, Serializable};
use midnight_storage_core_v1::{
    DefaultHasher, WellBehavedHasher,
    arena::ArenaHash,
    backend::OnDiskObject,
    db::{DB, DummyArbitrary, Update},
};
use sqlx::QueryBuilder;
use std::{collections::HashMap, future::ready};
use tokio::{runtime::Handle, task::block_in_place};

const BATCH_INSERT_SIZE: usize = 1_000;

#[cfg(feature = "cloud")]
type SqlxTransaction = sqlx::Transaction<'static, sqlx::Postgres>;

#[cfg(feature = "standalone")]
type SqlxTransaction = sqlx::Transaction<'static, sqlx::Sqlite>;

#[derive(Debug)]
pub struct LedgerDb {
    #[cfg(feature = "cloud")]
    pool: crate::infra::pool::postgres::PostgresPool,

    #[cfg(feature = "standalone")]
    pool: crate::infra::pool::sqlite::SqlitePool,
}

impl LedgerDb {
    #[cfg(feature = "cloud")]
    pub fn new(pool: crate::infra::pool::postgres::PostgresPool) -> Self {
        Self { pool }
    }

    #[cfg(feature = "standalone")]
    pub fn new(pool: crate::infra::pool::sqlite::SqlitePool) -> Self {
        Self { pool }
    }
}

impl DB for LedgerDb {
    type Hasher = DefaultHasher;
    type ScanResumeHandle = ArenaHash<DefaultHasher>;

    fn get_node(&self, key: &ArenaHash<Self::Hasher>) -> Option<OnDiskObject<Self::Hasher>> {
        block_in_place(|| {
            Handle::current().block_on(async {
                let query = indoc! {"
                    SELECT object
                    FROM ledger_db_nodes
                    WHERE key = $1
                "};

                sqlx::query_as::<_, (Vec<u8>,)>(query)
                    .bind(key.0.as_slice())
                    .fetch_optional(&*self.pool)
                    .await
                    .unwrap_or_panic("cannot get node")
                    .map(|(object,)| {
                        OnDiskObject::<Self::Hasher>::deserialize(&mut object.as_slice(), 0)
                    })
                    .transpose()
                    .unwrap_or_panic("cannot deserialize node as OnDiskObject")
            })
        })
    }

    fn insert_node(&mut self, key: ArenaHash<Self::Hasher>, object: OnDiskObject<Self::Hasher>) {
        block_in_place(|| {
            Handle::current().block_on(async {
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .unwrap_or_panic("begin transaction for insert node");

                let mut ser_object = Vec::with_capacity(object.serialized_size());
                Serializable::serialize(&object, &mut ser_object)
                    .unwrap_or_panic("cannot serialize object");

                let query = indoc! {"
                    INSERT INTO ledger_db_nodes (
                        key,
                        object
                    )
                    VALUES (
                        $1,
                        $2
                    )
                    ON CONFLICT (key) DO UPDATE
                    SET object = EXCLUDED.object
                "};

                sqlx::query(query)
                    .bind(key.0.as_slice())
                    .bind(ser_object.as_slice())
                    .execute(&mut *tx)
                    .await
                    .unwrap_or_panic("cannot insert node");

                tx.commit()
                    .await
                    .unwrap_or_panic("commit transaction for insert node");
            })
        })
    }

    fn delete_node(&mut self, key: &ArenaHash<Self::Hasher>) {
        block_in_place(|| {
            Handle::current().block_on(async {
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .unwrap_or_panic("begin transaction for delete node");

                delete_node(&mut tx, key).await;

                tx.commit()
                    .await
                    .unwrap_or_panic("commit transaction for delete node");
            })
        })
    }

    fn batch_update<I>(&mut self, updates: I)
    where
        I: Iterator<Item = (ArenaHash<Self::Hasher>, Update<Self::Hasher>)>,
    {
        block_in_place(|| {
            Handle::current().block_on(async {
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .unwrap_or_panic("begin transaction for batch update");

                let mut inserts = Vec::new();

                for (key, update) in updates {
                    match update {
                        Update::InsertNode(object) => inserts.push((key, object)),
                        Update::DeleteNode => {
                            flush_inserts(&mut tx, &mut inserts).await;
                            delete_node(&mut tx, &key).await;
                        }
                        Update::SetRootCount(count) => {
                            flush_inserts(&mut tx, &mut inserts).await;
                            set_root_count(&mut tx, key, count).await;
                        }
                    }
                }

                flush_inserts(&mut tx, &mut inserts).await;

                tx.commit()
                    .await
                    .unwrap_or_panic("commit transaction for batch update");
            })
        })
    }

    fn batch_get_nodes<I>(
        &self,
        keys: I,
    ) -> Vec<(ArenaHash<Self::Hasher>, Option<OnDiskObject<Self::Hasher>>)>
    where
        I: Iterator<Item = ArenaHash<Self::Hasher>>,
    {
        block_in_place(|| {
            Handle::current().block_on(async {
                let keys = keys.collect::<Vec<_>>();

                if keys.is_empty() {
                    return vec![];
                }

                #[cfg(feature = "cloud")]
                {
                    let query = indoc! {"
                        SELECT key, object
                        FROM ledger_db_nodes
                        WHERE key = ANY($1::bytea[])
                    "};

                    let mut nodes = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(query)
                        .bind(keys.iter().map(|key| key.0.as_slice()).collect::<Vec<_>>())
                        .fetch(&*self.pool)
                        .and_then(|(key, object)| {
                            let key_and_object =
                                ArenaHash::<Self::Hasher>::deserialize(&mut key.as_slice(), 0)
                                    .map_err(|error| sqlx::Error::Decode(error.into()))
                                    .and_then(|key| {
                                        let object = OnDiskObject::<Self::Hasher>::deserialize(
                                            &mut object.as_slice(),
                                            0,
                                        )
                                        .map_err(|error| sqlx::Error::Decode(error.into()));
                                        object.map(|object| (key, object))
                                    });
                            ready(key_and_object)
                        })
                        .try_collect::<HashMap<_, _>>()
                        .await
                        .unwrap_or_panic("cannot batch get nodes");

                    keys.into_iter()
                        .map(|key| {
                            let node = nodes.remove(&key);
                            (key, node)
                        })
                        .collect()
                }

                #[cfg(feature = "standalone")]
                {
                    use sqlx::QueryBuilder;

                    let query = indoc! {"
                        SELECT key, object
                        FROM ledger_db_nodes
                        WHERE key IN (
                    "};

                    let mut query = QueryBuilder::new(query);
                    let mut bindings = query.separated(", ");
                    for key in &keys {
                        bindings.push_bind(key.0.as_slice());
                    }
                    query.push(")");

                    let mut nodes = query
                        .build_query_as::<(Vec<u8>, Vec<u8>)>()
                        .fetch(&*self.pool)
                        .and_then(|(key, object)| {
                            let key_and_object =
                                ArenaHash::<Self::Hasher>::deserialize(&mut key.as_slice(), 0)
                                    .map_err(|error| sqlx::Error::Decode(error.into()))
                                    .and_then(|key| {
                                        let object = OnDiskObject::<Self::Hasher>::deserialize(
                                            &mut object.as_slice(),
                                            0,
                                        )
                                        .map_err(|error| sqlx::Error::Decode(error.into()));
                                        object.map(|object| (key, object))
                                    });
                            ready(key_and_object)
                        })
                        .try_collect::<HashMap<_, _>>()
                        .await
                        .unwrap_or_panic("cannot batch get nodes");

                    keys.into_iter()
                        .map(|key| {
                            let node = nodes.remove(&key);
                            (key, node)
                        })
                        .collect()
                }
            })
        })
    }

    fn get_root_count(&self, key: &ArenaHash<Self::Hasher>) -> u32 {
        block_in_place(|| {
            Handle::current().block_on(async {
                // Read the stored `count` column, not `count(1)`: the row-count aggregate
                // is 0/1 (key is the primary key) and under-reports true counts >= 2, which
                // storage-core's flush then drives negative. A missing row means count 0.
                let query = indoc! {"
                    SELECT count
                    FROM ledger_db_roots
                    WHERE key = $1
                "};

                sqlx::query_as::<_, (i64,)>(query)
                    .bind(key.0.as_slice())
                    .fetch_optional(&*self.pool)
                    .await
                    .unwrap_or_panic("cannot get root count")
                    .map_or(0, |(count,)| count as u32)
            })
        })
    }

    fn set_root_count(&mut self, key: ArenaHash<Self::Hasher>, count: u32) {
        block_in_place(|| {
            Handle::current().block_on(async {
                let mut tx = self
                    .pool
                    .begin()
                    .await
                    .unwrap_or_panic("begin transaction for set root count");

                set_root_count(&mut tx, key, count).await;

                tx.commit()
                    .await
                    .unwrap_or_panic("commit transaction for set root count");
            })
        })
    }

    fn get_roots(&self) -> HashMap<ArenaHash<Self::Hasher>, u32> {
        block_in_place(|| {
            Handle::current().block_on(async {
                let query = indoc! {"
                    SELECT key, count
                    FROM ledger_db_roots
                "};

                sqlx::query_as::<_, (Vec<u8>, i64)>(query)
                    .fetch(&*self.pool)
                    .and_then(|(key, count)| {
                        let key = ArenaHash::<Self::Hasher>::deserialize(&mut key.as_slice(), 0)
                            .map_err(|error| sqlx::Error::Decode(error.into()));
                        ready(key.map(|key| (key, count as u32)))
                    })
                    .try_collect::<HashMap<_, _>>()
                    .await
                    .unwrap_or_panic("cannot get roots")
            })
        })
    }

    fn size(&self) -> usize {
        block_in_place(|| {
            Handle::current().block_on(async {
                let query = indoc! {"
                    SELECT count(1)
                    FROM ledger_db_nodes
                "};

                let (count,) = sqlx::query_as::<_, (i64,)>(query)
                    .fetch_one(&*self.pool)
                    .await
                    .unwrap_or_panic("cannot get size");

                count as usize
            })
        })
    }

    /// Called by storage-core's gc-v1 sweep phase (`gc.rs`): after marking
    /// reachable nodes, `gc()` scans the DB in batches, culls every scanned
    /// node not in the mark set, and uses the returned resume handle to pick
    /// up where a time-bounded pass left off. Not called by indexer code.
    fn scan(
        &self,
        resume_from: Option<Self::ScanResumeHandle>,
        batch_size: usize,
    ) -> (
        Vec<(ArenaHash<Self::Hasher>, OnDiskObject<Self::Hasher>)>,
        Option<Self::ScanResumeHandle>,
    ) {
        block_in_place(|| {
            Handle::current().block_on(async {
                let mut query = QueryBuilder::new("SELECT key, object FROM ledger_db_nodes");
                if let Some(handle) = resume_from.as_ref() {
                    query.push(" WHERE key > ");
                    query.push_bind(handle.0.as_slice());
                }
                query.push(" ORDER BY key LIMIT ");
                query.push_bind(batch_size as i64);

                let raw = query
                    .build_query_as::<(Vec<u8>, Vec<u8>)>()
                    .fetch_all(&*self.pool)
                    .await
                    .unwrap_or_panic("cannot scan ledger_db_nodes");

                let batch = raw
                    .into_iter()
                    .map(|(key, object)| {
                        let key = ArenaHash::<Self::Hasher>::deserialize(&mut key.as_slice(), 0)
                            .unwrap_or_panic("cannot deserialize key as ArenaHash");
                        let object =
                            OnDiskObject::<Self::Hasher>::deserialize(&mut object.as_slice(), 0)
                                .unwrap_or_panic("cannot deserialize node as OnDiskObject");
                        (key, object)
                    })
                    .collect::<Vec<_>>();

                let next_handle = batch.last().map(|(k, _)| k.clone());

                (batch, next_handle)
            })
        })
    }
}

impl Default for LedgerDb {
    fn default() -> Self {
        panic!("LedgerDb cannot be constructed by default");
    }
}

impl DummyArbitrary for LedgerDb {}

trait ResultExt<T> {
    fn unwrap_or_panic(self, msg: &'static str) -> T;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn unwrap_or_panic(self, msg: &'static str) -> T {
        self.unwrap_or_else(|error| panic!("{msg}: {}", error.as_chain()))
    }
}

async fn flush_inserts<H>(
    tx: &mut SqlxTransaction,
    inserts: &mut Vec<(ArenaHash<H>, OnDiskObject<H>)>,
) where
    H: WellBehavedHasher,
{
    if inserts.is_empty() {
        return;
    }

    for chunk in inserts.chunks(BATCH_INSERT_SIZE) {
        QueryBuilder::new("INSERT INTO ledger_db_nodes (key, object) ")
            .push_values(chunk, |mut q, (key, object)| {
                let mut ser_object = Vec::with_capacity(object.serialized_size());
                Serializable::serialize(object, &mut ser_object)
                    .unwrap_or_panic("cannot serialize object");
                q.push_bind(key.0.as_slice()).push_bind(ser_object);
            })
            .push(" ON CONFLICT (key) DO UPDATE SET object = EXCLUDED.object")
            .build()
            .execute(&mut **tx)
            .await
            .unwrap_or_panic("cannot batch insert nodes");
    }

    inserts.clear();
}

async fn delete_node<H>(tx: &mut SqlxTransaction, key: &ArenaHash<H>)
where
    H: WellBehavedHasher,
{
    let query = indoc! {"
        DELETE FROM ledger_db_nodes
        WHERE key = $1
    "};

    sqlx::query(query)
        .bind(key.0.as_slice())
        .execute(&mut **tx)
        .await
        .unwrap_or_panic("cannot delete node");
}

async fn set_root_count<H>(tx: &mut SqlxTransaction, key: ArenaHash<H>, count: u32)
where
    H: WellBehavedHasher,
{
    if count > 0 {
        let query = indoc! {"
            INSERT INTO ledger_db_roots (key, count)
            VALUES ($1, $2)
            ON CONFLICT (key)
            DO UPDATE SET count = $2
        "};

        sqlx::query(query)
            .bind(key.0.as_slice())
            .bind(count as i64)
            .execute(&mut **tx)
            .await
            .unwrap_or_panic("cannot set root count");
    } else {
        let query = indoc! {"
            DELETE FROM ledger_db_roots
            WHERE key = $1
        "};

        sqlx::query(query)
            .bind(key.0.as_slice())
            .execute(&mut **tx)
            .await
            .unwrap_or_panic("cannot set root count");
    }
}

#[cfg(test)]
pub(crate) fn arena_hash(bytes: [u8; 32]) -> ArenaHash<DefaultHasher> {
    ArenaHash::deserialize(&mut bytes.as_slice(), 0).expect("32 bytes are an ArenaHash")
}

/// `get_root_count` must return the stored count, not the number of matching rows: `key` is the
/// primary key, so a row-count aggregate can only ever be 0 or 1.
///
/// Shared by the sqlite and postgres suites rather than written twice: `LedgerDb` is one type under
/// both features - only its pool field is feature-gated - so the assertions need neither generics
/// nor a macro. Only acquiring the pool differs, and that stays in each suite.
#[cfg(test)]
fn assert_get_root_count_reads_stored_count(db: &mut LedgerDb) {
    let key = arena_hash([0xaa; 32]);
    let other_key = arena_hash([0xbb; 32]);

    assert_eq!(db.get_root_count(&key), 0, "missing key is not a root");

    db.set_root_count(key.clone(), 7);
    db.set_root_count(other_key.clone(), 1);
    assert_eq!(db.get_root_count(&key), 7);
    assert_eq!(db.get_root_count(&other_key), 1);

    db.set_root_count(key.clone(), 0);
    assert_eq!(db.get_root_count(&key), 0, "count 0 deletes the row");
    assert_eq!(db.get_root_count(&other_key), 1);
}

#[cfg(all(test, feature = "standalone"))]
pub(crate) mod sqlite_tests {
    use super::assert_get_root_count_reads_stored_count;
    use crate::infra::{
        ledger_db::v1_1::LedgerDb,
        migrations::sqlite::run_for_ledger_db,
        pool::sqlite::{Config, SqlitePool},
    };
    use anyhow::Context;
    use indoc::indoc;
    use midnight_storage_core_v1::{DefaultHasher, Storage, arena::ArenaHash, db::DB};
    use std::error::Error as StdError;

    /// Number of times a root is persisted before it is unpersisted again; stands in for the
    /// retention window (a ledger state root is persisted once per block and unpersisted once
    /// the block leaves the window). Any value >= 3 exposes the read bug.
    const PERSISTS: u32 = 5;

    pub(crate) async fn setup_pool() -> Result<SqlitePool, Box<dyn StdError>> {
        let pool = SqlitePool::new(Config::default())
            .await
            .context("create sqlite pool")?;
        run_for_ledger_db(&pool)
            .await
            .context("run ledger_db migrations")?;

        Ok(pool)
    }

    /// The stored root count for `key`, read with plain SQL so assertions do not go through the
    /// code under test. `None` means there is no row, i.e. the key is not a root.
    pub(crate) async fn stored_root_count(
        pool: &SqlitePool,
        key: &ArenaHash<DefaultHasher>,
    ) -> Result<Option<i64>, Box<dyn StdError>> {
        let query = indoc! {"
            SELECT count
            FROM ledger_db_roots
            WHERE key = $1
        "};

        let count = sqlx::query_as::<_, (i64,)>(query)
            .bind(key.0.as_slice())
            .fetch_optional(&**pool)
            .await
            .context("select stored root count")?
            .map(|(count,)| count);

        Ok(count)
    }

    #[tokio::test]
    async fn scan_empty_db_returns_no_rows_and_no_handle() -> Result<(), Box<dyn StdError>> {
        let pool = SqlitePool::new(Config::default())
            .await
            .context("create sqlite pool")?;
        run_for_ledger_db(&pool)
            .await
            .context("run ledger_db migrations")?;

        let db = LedgerDb::new(pool);

        let (batch, handle) = tokio::task::spawn_blocking(move || {
            // scan() uses block_in_place internally, which requires being on a
            // multi-thread runtime; spawn_blocking is a clean way to satisfy that.
            db.scan(None, 100)
        })
        .await
        .context("await scan")?;

        assert!(batch.is_empty());
        assert!(handle.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn scan_with_resume_on_empty_db_still_returns_no_rows() -> Result<(), Box<dyn StdError>> {
        let pool = SqlitePool::new(Config::default())
            .await
            .context("create sqlite pool")?;
        run_for_ledger_db(&pool)
            .await
            .context("run ledger_db migrations")?;

        let db = LedgerDb::new(pool);
        let fake_resume: ArenaHash<DefaultHasher> = ArenaHash::default();

        let (batch, handle) = tokio::task::spawn_blocking(move || db.scan(Some(fake_resume), 50))
            .await
            .context("await scan")?;

        assert!(batch.is_empty());
        assert!(handle.is_none());

        Ok(())
    }

    /// `get_root_count` must return the stored count, not the number of matching rows: `key` is
    /// the primary key, so a row-count aggregate can only ever be 0 or 1.
    #[tokio::test(flavor = "multi_thread")]
    async fn get_root_count_returns_stored_count_not_row_count() -> Result<(), Box<dyn StdError>> {
        let pool = setup_pool().await?;
        let mut db = LedgerDb::new(pool);

        assert_get_root_count_reads_stored_count(&mut db);

        Ok(())
    }

    /// Root counts must accumulate across persists: storage-core's flush computes
    /// `stored count + delta`, so an under-reading `get_root_count` writes back a count that is
    /// too low. With a read saturating at 1 every persist stores `1 + 1`, and the count caps at
    /// 2 however many blocks reference the root.
    #[tokio::test(flavor = "multi_thread")]
    async fn repeated_persist_accumulates_stored_root_count() -> Result<(), Box<dyn StdError>> {
        let pool = setup_pool().await?;
        let storage = Storage::new(16, LedgerDb::new(pool.clone()));

        let mut root = storage.alloc(42u32);
        for _ in 0..PERSISTS {
            root.persist();
            storage.with_backend(|backend| backend.flush_all_changes_to_db());
        }

        assert_eq!(
            stored_root_count(&pool, &root.hash()).await?,
            Some(PERSISTS.into())
        );

        Ok(())
    }

    /// Balanced persist/unpersist must bring the root count back to zero. Under-read counts make
    /// storage-core's flush compute a negative count and panic ("roots counts can't be negative"),
    /// which is what takes the chain-indexer down once a root leaves the retention window.
    #[tokio::test(flavor = "multi_thread")]
    async fn balanced_unpersist_returns_root_count_to_zero() -> Result<(), Box<dyn StdError>> {
        let pool = setup_pool().await?;
        let storage = Storage::new(16, LedgerDb::new(pool.clone()));

        let mut root = storage.alloc(42u32);
        for _ in 0..PERSISTS {
            root.persist();
            storage.with_backend(|backend| backend.flush_all_changes_to_db());
        }

        for _ in 0..PERSISTS {
            root.unpersist();
            storage.with_backend(|backend| backend.flush_all_changes_to_db());
        }

        assert_eq!(
            stored_root_count(&pool, &root.hash()).await?,
            None,
            "no longer a root"
        );

        Ok(())
    }
}

#[cfg(all(test, feature = "cloud"))]
mod postgres_tests {
    use super::assert_get_root_count_reads_stored_count;
    use crate::infra::{
        ledger_db::v1_1::LedgerDb,
        migrations::postgres::run as run_migrations,
        pool::{self, postgres::PostgresPool},
    };
    use anyhow::Context;
    use midnight_storage_core_v1::{DefaultHasher, arena::ArenaHash, db::DB};
    use sqlx::postgres::PgSslMode;
    use std::{error::Error as StdError, time::Duration};
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::postgres::Postgres;

    async fn setup_pool()
    -> Result<(testcontainers::ContainerAsync<Postgres>, PostgresPool), Box<dyn StdError>> {
        let container = Postgres::default()
            .with_db_name("indexer")
            .with_user("indexer")
            .with_password(env!("APP__INFRA__STORAGE__PASSWORD"))
            .with_tag("17.1-alpine")
            .start()
            .await
            .context("start Postgres container")?;
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .context("get Postgres port")?;

        let config = pool::postgres::Config {
            host: "localhost".to_string(),
            port,
            dbname: "indexer".to_string(),
            user: "indexer".to_string(),
            password: env!("APP__INFRA__STORAGE__PASSWORD").into(),
            sslmode: PgSslMode::Prefer,
            max_connections: 10,
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(5 * 60),
        };
        let pool = PostgresPool::new(config).await?;
        run_migrations(&pool).await.context("run migrations")?;

        Ok((container, pool))
    }

    #[tokio::test]
    async fn scan_empty_db_returns_no_rows_and_no_handle() -> Result<(), Box<dyn StdError>> {
        let (_container, pool) = setup_pool().await?;
        let db = LedgerDb::new(pool);

        let (batch, handle) = tokio::task::spawn_blocking(move || db.scan(None, 100))
            .await
            .context("await scan")?;

        assert!(batch.is_empty());
        assert!(handle.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn scan_with_resume_on_empty_db_still_returns_no_rows() -> Result<(), Box<dyn StdError>> {
        let (_container, pool) = setup_pool().await?;
        let db = LedgerDb::new(pool);
        let fake_resume: ArenaHash<DefaultHasher> = ArenaHash::default();

        let (batch, handle) = tokio::task::spawn_blocking(move || db.scan(Some(fake_resume), 50))
            .await
            .context("await scan")?;

        assert!(batch.is_empty());
        assert!(handle.is_none());

        Ok(())
    }

    /// See the standalone counterpart: `key` is the primary key, so a row-count aggregate can
    /// only ever be 0 or 1, never the stored count.
    #[tokio::test(flavor = "multi_thread")]
    async fn get_root_count_reads_the_stored_count() -> Result<(), Box<dyn StdError>> {
        let (_container, pool) = setup_pool().await?;
        let mut db = LedgerDb::new(pool);

        assert_get_root_count_reads_stored_count(&mut db);

        Ok(())
    }
}

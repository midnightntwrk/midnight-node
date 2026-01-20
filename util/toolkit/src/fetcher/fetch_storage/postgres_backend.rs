// This file is part of midnight-node.
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

use async_trait::async_trait;
use core::fmt::Debug;
use midnight_node_ledger_helpers::{DB, ProofKind, SignatureKind, Tagged};
use serde::{Deserialize, Serialize};
use sqlx::{
	PgPool, Row,
	postgres::{PgPoolOptions, PgRow},
};
use subxt::utils::H256;

use super::{BlockData, FetchStorage};

/// Persistent [`FetchStorage`] backend using PostgreSQL.
///
/// Data is serialized as BSON. Uses sqlx connection pooling.
#[derive(Clone)]
pub struct PostgresBackend<S: SignatureKind<D> + Tagged, P: ProofKind<D> + Debug, D: DB> {
	pool: PgPool,
	_marker: std::marker::PhantomData<(S, P, D)>,
}

impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D> + Debug>
	PostgresBackend<S, P, D>
{
	/// Creates a new backend and initializes tables. Panics on connection failure.
	pub async fn new(database_url: &str) -> Self {
		let pool = PgPoolOptions::new()
			.max_connections(10)
			.connect(database_url)
			.await
			.expect("failed to create database pool");

		let backend = Self { pool, _marker: std::marker::PhantomData };

		backend.init_tables().await;
		backend
	}

	/// Creates a new backend with an existing connection pool.
	pub async fn with_pool(pool: PgPool) -> Self {
		let backend = Self { pool, _marker: std::marker::PhantomData };

		backend.init_tables().await;
		backend
	}

	/// Creates required tables if they don't exist.
	async fn init_tables(&self) {
		sqlx::query(
			r#"
            CREATE TABLE IF NOT EXISTS block_data (
                chain_id BYTEA NOT NULL,
                block_number BIGINT NOT NULL,
                data BYTEA NOT NULL,
                PRIMARY KEY (chain_id, block_number)
            )
            "#,
		)
		.execute(&self.pool)
		.await
		.expect("failed to create block_data table");

		sqlx::query(
			r#"
            CREATE TABLE IF NOT EXISTS highest_verified (
                chain_id BYTEA PRIMARY KEY,
                height BIGINT NOT NULL
            )
            "#,
		)
		.execute(&self.pool)
		.await
		.expect("failed to create highest_verified table");

		// Create index for efficient range queries
		sqlx::query(
			r#"
            CREATE INDEX IF NOT EXISTS idx_block_data_chain_number 
            ON block_data (chain_id, block_number)
            "#,
		)
		.execute(&self.pool)
		.await
		.expect("failed to create index");
	}

	fn serialize_block_data(block: &BlockData<S, P, D>) -> Vec<u8>
	where
		BlockData<S, P, D>: Serialize,
	{
		bson::serialize_to_vec(block).expect("failed to serialize block data")
	}

	fn deserialize_block_data(data: &[u8]) -> BlockData<S, P, D>
	where
		for<'a> BlockData<S, P, D>: Deserialize<'a>,
	{
		bson::deserialize_from_slice(data).expect("failed to deserialize block data")
	}
}

#[async_trait]
impl<D: DB + Clone, S: SignatureKind<D> + Tagged, P: ProofKind<D> + Debug> FetchStorage<S, P, D>
	for PostgresBackend<S, P, D>
where
	BlockData<S, P, D>: Serialize,
	for<'a> BlockData<S, P, D>: Deserialize<'a>,
{
	async fn get_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
	) -> Option<BlockData<S, P, D>> {
		let result: Option<PgRow> = sqlx::query(
			r#"
            SELECT data FROM block_data 
            WHERE chain_id = $1 AND block_number = $2
            "#,
		)
		.bind(chain_id.0.as_slice())
		.bind(block_number as i64)
		.fetch_optional(&self.pool)
		.await
		.expect("failed to query block data");

		result.map(|row| {
			let data: Vec<u8> = row.get("data");
			Self::deserialize_block_data(&data)
		})
	}

	async fn get_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = u64> + Send,
	) -> Vec<Option<BlockData<S, P, D>>> {
		let block_numbers: Vec<u64> = range.collect();

		if block_numbers.is_empty() {
			return Vec::new();
		}

		let block_numbers_i64: Vec<i64> = block_numbers.iter().map(|&n| n as i64).collect();

		// Create a table with the block numbers, then left-join to create nulls if missing
		let rows: Vec<PgRow> = sqlx::query(
			r#"
            SELECT bd.data
            FROM UNNEST($2::BIGINT[]) WITH ORDINALITY AS bn(block_number, ord)
            LEFT JOIN block_data bd ON bd.chain_id = $1 AND bd.block_number = bn.block_number
            ORDER BY bn.ord
            "#,
		)
		.bind(chain_id.0.as_slice())
		.bind(&block_numbers_i64)
		.fetch_all(&self.pool)
		.await
		.expect("failed to query block data range");

		rows.into_iter()
			.map(|row| {
				let data: Option<Vec<u8>> = row.get("data");
				data.map(|d| Self::deserialize_block_data(&d))
			})
			.collect()
	}

	async fn insert_block_data(
		&self,
		chain_id: H256,
		block_number: u64,
		block: BlockData<S, P, D>,
	) {
		let data = Self::serialize_block_data(&block);

		sqlx::query(
			r#"
            INSERT INTO block_data (chain_id, block_number, data)
            VALUES ($1, $2, $3)
            ON CONFLICT (chain_id, block_number) 
            DO UPDATE SET data = EXCLUDED.data
            "#,
		)
		.bind(chain_id.0.as_slice())
		.bind(block_number as i64)
		.bind(&data)
		.execute(&self.pool)
		.await
		.expect("failed to insert block data");
	}

	async fn insert_block_data_range(
		&self,
		chain_id: H256,
		range: impl Iterator<Item = (u64, BlockData<S, P, D>)> + Send,
	) {
		let blocks: Vec<(u64, BlockData<S, P, D>)> = range.collect();

		if blocks.is_empty() {
			return;
		}

		// Use a transaction for batch insert
		let mut tx = self.pool.begin().await.expect("failed to begin transaction");

		for (block_number, block) in blocks {
			let data = Self::serialize_block_data(&block);

			sqlx::query(
				r#"
                INSERT INTO block_data (chain_id, block_number, data)
                VALUES ($1, $2, $3)
                ON CONFLICT (chain_id, block_number) 
                DO UPDATE SET data = EXCLUDED.data
                "#,
			)
			.bind(chain_id.0.as_slice())
			.bind(block_number as i64)
			.bind(&data)
			.execute(&mut *tx)
			.await
			.expect("failed to insert block data");
		}

		tx.commit().await.expect("failed to commit transaction");
	}

	async fn get_highest_verified_block(&self, chain_id: H256) -> Option<u64> {
		let result: Option<PgRow> = sqlx::query(
			r#"
            SELECT height FROM highest_verified 
            WHERE chain_id = $1
            "#,
		)
		.bind(chain_id.0.as_slice())
		.fetch_optional(&self.pool)
		.await
		.expect("failed to query highest verified block");

		result.map(|row| {
			let height: i64 = row.get("height");
			height as u64
		})
	}

	async fn set_highest_verified_block(&self, chain_id: H256, height: u64) {
		sqlx::query(
			r#"
            INSERT INTO highest_verified (chain_id, height)
            VALUES ($1, $2)
            ON CONFLICT (chain_id) 
            DO UPDATE SET height = EXCLUDED.height
            "#,
		)
		.bind(chain_id.0.as_slice())
		.bind(height as i64)
		.execute(&self.pool)
		.await
		.expect("failed to set highest verified block");
	}
}

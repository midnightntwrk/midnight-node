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

//! PostgreSQL-based storage backend for wallet state caching.
//!
//! This backend stores wallet state in a PostgreSQL database, enabling
//! multi-instance toolkit deployments to share cached wallet state.

use async_trait::async_trait;
use sqlx::{
	PgPool, Row,
	postgres::{PgPoolOptions, PgRow},
};
use subxt::utils::H256;

use super::{WalletStateCache, WalletStateStorage};

/// Persistent [`WalletStateStorage`] backend using PostgreSQL.
///
/// Data is serialized as BSON. Uses sqlx connection pooling.
/// This backend enables multi-instance toolkit deployments to share cached wallet state.
#[derive(Clone)]
pub struct PostgresBackend {
	pool: PgPool,
}

impl PostgresBackend {
	/// Creates a new backend and initializes tables.
	///
	/// # Panics
	///
	/// Panics if the database connection fails.
	pub async fn new(database_url: &str) -> Self {
		let pool = PgPoolOptions::new()
			.max_connections(10)
			.connect(database_url)
			.await
			.expect("failed to create database pool");

		let backend = Self { pool };
		backend.init_tables().await;
		backend
	}

	/// Creates a new backend with an existing connection pool.
	///
	/// Useful for sharing a connection pool with `FetchStorage::PostgresBackend`.
	pub async fn with_pool(pool: PgPool) -> Self {
		let backend = Self { pool };
		backend.init_tables().await;
		backend
	}

	/// Creates required tables if they don't exist.
	async fn init_tables(&self) {
		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS wallet_state_cache (
				chain_id BYTEA NOT NULL,
				wallet_id BYTEA NOT NULL,
				block_height BIGINT NOT NULL,
				data BYTEA NOT NULL,
				created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
				updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
				PRIMARY KEY (chain_id, wallet_id)
			)
			"#,
		)
		.execute(&self.pool)
		.await
		.expect("failed to create wallet_state_cache table");

		// Create index for efficient queries by chain_id
		sqlx::query(
			r#"
			CREATE INDEX IF NOT EXISTS idx_wallet_state_cache_chain
			ON wallet_state_cache (chain_id)
			"#,
		)
		.execute(&self.pool)
		.await
		.expect("failed to create chain_id index");
	}

	fn serialize(cache: &WalletStateCache) -> Vec<u8> {
		bson::serialize_to_vec(cache).expect("failed to serialize wallet state cache")
	}

	fn deserialize(data: &[u8]) -> WalletStateCache {
		bson::deserialize_from_slice(data).expect("failed to deserialize wallet state cache")
	}
}

#[async_trait]
impl WalletStateStorage for PostgresBackend {
	async fn get_wallet_state(&self, chain_id: H256, wallet_id: H256) -> Option<WalletStateCache> {
		let result: Option<PgRow> = sqlx::query(
			r#"
			SELECT data FROM wallet_state_cache
			WHERE chain_id = $1 AND wallet_id = $2
			"#,
		)
		.bind(chain_id.0.as_slice())
		.bind(wallet_id.0.as_slice())
		.fetch_optional(&self.pool)
		.await
		.expect("failed to query wallet state cache");

		result.map(|row| {
			let data: Vec<u8> = row.get("data");
			Self::deserialize(&data)
		})
	}

	async fn set_wallet_state(&self, chain_id: H256, wallet_id: H256, cache: WalletStateCache) {
		let data = Self::serialize(&cache);

		sqlx::query(
			r#"
			INSERT INTO wallet_state_cache (chain_id, wallet_id, block_height, data, updated_at)
			VALUES ($1, $2, $3, $4, NOW())
			ON CONFLICT (chain_id, wallet_id)
			DO UPDATE SET block_height = EXCLUDED.block_height, data = EXCLUDED.data, updated_at = NOW()
			"#,
		)
		.bind(chain_id.0.as_slice())
		.bind(wallet_id.0.as_slice())
		.bind(cache.block_height as i64)
		.bind(&data)
		.execute(&self.pool)
		.await
		.expect("failed to insert wallet state cache");
	}

	async fn get_cached_block_height(&self, chain_id: H256, wallet_id: H256) -> Option<u64> {
		// Query only the block_height column for efficiency
		let result: Option<PgRow> = sqlx::query(
			r#"
			SELECT block_height FROM wallet_state_cache
			WHERE chain_id = $1 AND wallet_id = $2
			"#,
		)
		.bind(chain_id.0.as_slice())
		.bind(wallet_id.0.as_slice())
		.fetch_optional(&self.pool)
		.await
		.expect("failed to query cached block height");

		result.map(|row| {
			let height: i64 = row.get("block_height");
			height as u64
		})
	}

	async fn delete_wallet_state(&self, chain_id: H256, wallet_id: H256) {
		sqlx::query(
			r#"
			DELETE FROM wallet_state_cache
			WHERE chain_id = $1 AND wallet_id = $2
			"#,
		)
		.bind(chain_id.0.as_slice())
		.bind(wallet_id.0.as_slice())
		.execute(&self.pool)
		.await
		.expect("failed to delete wallet state cache");
	}
}

// Note: Integration tests for PostgresBackend require a running PostgreSQL instance.
// They should be added to the integration test suite rather than unit tests.
// Example test setup would use testcontainers or a dedicated test database.

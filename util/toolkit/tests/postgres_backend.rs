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

mod common;

use common::test_image;
use midnight_node_ledger_helpers::DefaultDB;
use midnight_node_toolkit::{
	ProofType, SignatureType,
	fetcher::{
		fetch_storage::{WalletStateCaching, postgres_backend::PostgresBackend},
		wallet_state_cache::{SerializableBlockContext, WalletSnapshot, WalletStateCache},
	},
};
use subxt::utils::H256;
use testcontainers::{GenericImage, ImageExt, core::WaitFor, runners::AsyncRunner};
use tokio::sync::OnceCell;

type TestPostgresBackend = PostgresBackend<SignatureType, ProofType, DefaultDB>;

struct SharedPostgres {
	_container: testcontainers::ContainerAsync<GenericImage>,
	url: String,
}

static POSTGRES: OnceCell<SharedPostgres> = OnceCell::const_new();

async fn postgres_url() -> &'static str {
	&POSTGRES
		.get_or_init(|| async {
			let (name, tag) = test_image("postgres");
			let password: String =
				(0..32).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
			let container = GenericImage::new(name, tag)
				.with_wait_for(WaitFor::message_on_stderr(
					"database system is ready to accept connections",
				))
				.with_env_var("POSTGRES_PASSWORD", &password)
				.with_env_var("POSTGRES_USER", "postgres")
				.with_env_var("POSTGRES_DB", "toolkit_test")
				.start()
				.await
				.expect("failed to start postgres container");

			let port =
				container.get_host_port_ipv4(5432).await.expect("failed to get postgres port");
			let url = format!("postgres://postgres:{password}@localhost:{port}/toolkit_test");
			SharedPostgres { _container: container, url }
		})
		.await
		.url
}

fn create_test_cache(block_height: u64, wallet_id: H256) -> WalletStateCache {
	WalletStateCache {
		chain_id: H256::from([1u8; 32]),
		wallet_id,
		block_height,
		ledger_state_bytes: vec![0u8; 1000],
		wallet_snapshots: vec![WalletSnapshot {
			seed_hash: H256::from([3u8; 32]),
			shielded_state_bytes: vec![],
			dust_local_state_bytes: None,
		}],
		latest_block_context: SerializableBlockContext {
			tblock_secs: 1234567890,
			tblock_err: 0,
			parent_block_hash: [4u8; 32],
		},
		state_root: Some(vec![5u8; 32]),
		version: "wallet-state-cache-v1".to_string(),
	}
}

#[tokio::test]
async fn test_postgres_wallet_state_roundtrip() {
	let backend: TestPostgresBackend = PostgresBackend::new(postgres_url().await).await;

	let chain_id = H256::from([100u8; 32]);
	let wallet_id = H256::from([2u8; 32]);

	let cache = create_test_cache(100, wallet_id);

	// Initially no cache
	assert!(
		WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id)
			.await
			.is_none()
	);

	// Save cache
	WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache.clone()).await;

	// Retrieve cache
	let retrieved = WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id).await;
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();

	assert_eq!(retrieved.chain_id, cache.chain_id);
	assert_eq!(retrieved.wallet_id, cache.wallet_id);
	assert_eq!(retrieved.block_height, cache.block_height);
	assert_eq!(retrieved.ledger_state_bytes, cache.ledger_state_bytes);
	assert_eq!(retrieved.version, cache.version);
}

#[tokio::test]
async fn test_postgres_evict_stale_entries() {
	let backend: TestPostgresBackend = PostgresBackend::new(postgres_url().await).await;

	let chain_id = H256::from([101u8; 32]);
	let wallet_id = H256::from([2u8; 32]);

	// Save a cache entry
	let cache = create_test_cache(100, wallet_id);
	WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache).await;

	// Evict entries older than 30 days (should not evict our fresh entry)
	let evicted = backend.evict_stale_wallet_cache(30).await;
	assert_eq!(evicted, 0);

	// Entry should still exist
	assert!(
		WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id)
			.await
			.is_some()
	);

	// Evict entries older than 0 days (should evict everything)
	let evicted = backend.evict_stale_wallet_cache(0).await;
	assert!(evicted >= 1);

	// Entry should be gone
	assert!(
		WalletStateCaching::get_wallet_state(&backend, chain_id, wallet_id)
			.await
			.is_none()
	);
}

#[tokio::test]
async fn test_postgres_evict_oldest_entries() {
	let backend: TestPostgresBackend = PostgresBackend::new(postgres_url().await).await;

	let chain_id = H256::from([102u8; 32]);

	// Create 5 cache entries with different wallet IDs
	for i in 0..5u8 {
		let wallet_id = H256::from([i + 10; 32]);
		let cache = create_test_cache(i as u64 * 100, wallet_id);
		WalletStateCaching::set_wallet_state(&backend, chain_id, wallet_id, cache).await;
		// Small delay to ensure different timestamps
		tokio::time::sleep(std::time::Duration::from_millis(10)).await;
	}

	// Count should be at least 5
	let count = backend.wallet_cache_count().await;
	assert!(count >= 5, "Expected at least 5 entries, got {}", count);

	// Keep only 2 entries (evict at least 3)
	let evicted = backend.evict_oldest_wallet_cache(2).await;
	assert!(evicted >= 3, "Expected to evict at least 3, evicted {}", evicted);
}

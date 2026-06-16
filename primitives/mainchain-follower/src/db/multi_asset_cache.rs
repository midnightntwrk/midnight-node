// This file is part of midnight-node.
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

use log::info;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use tokio::sync::Mutex;

type PolicyNameKey = (Vec<u8>, Vec<u8>);

/// Caches `multi_asset.id` (db-sync surrogate key) lookups to avoid repeated joins on the
/// `multi_asset` table. The cached IDs are stable for the lifetime of the process because any
/// scenario that reassigns surrogate keys (db-sync reset, resync, or deep rollback past the
/// asset's minting block) requires restarting the node, which clears this in-memory cache.
pub struct MultiAssetCache {
	pool: Pool<Postgres>,
	cache: Mutex<HashMap<PolicyNameKey, i64>>,
}

impl MultiAssetCache {
	pub fn new(pool: Pool<Postgres>) -> Self {
		Self { pool, cache: Mutex::new(HashMap::new()) }
	}

	/// Resolves the `multi_asset.id` for a given (policy, name) pair, caching the result.
	/// Returns `None` if no matching multi_asset entry exists in db-sync.
	pub async fn resolve_ident(
		&self,
		policy: &[u8],
		name: &[u8],
	) -> Result<Option<i64>, sqlx::Error> {
		let key = (policy.to_vec(), name.to_vec());
		{
			let cache = self.cache.lock().await;
			if let Some(&id) = cache.get(&key) {
				return Ok(Some(id));
			}
		}

		let id_opt = resolve_multi_asset_id(&self.pool, policy, name).await?;

		if let Some(id) = id_opt {
			info!("Cached multi_asset.id = {} for policy/name pair", id);
			let mut cache = self.cache.lock().await;
			cache.insert(key, id);
		}

		Ok(id_opt)
	}
}

/// Resolve `multi_asset.id` (db-sync surrogate key) for a `(policy, name)` pair
/// with a single, uncached db-sync lookup. [`MultiAssetCache::resolve_ident`]
/// wraps this with a process-lifetime cache; callers issuing a one-shot query
/// (e.g. a bulk pull) can use this directly to avoid allocating a cache.
pub async fn resolve_multi_asset_id(
	pool: &Pool<Postgres>,
	policy: &[u8],
	name: &[u8],
) -> Result<Option<i64>, sqlx::Error> {
	sqlx::query_scalar("SELECT id FROM multi_asset WHERE policy = $1 AND name = $2")
		.bind(policy)
		.bind(name)
		.fetch_optional(pool)
		.await
}

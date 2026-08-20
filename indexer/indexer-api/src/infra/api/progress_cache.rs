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

//! Cross-subscriber cache for shielded and unshielded progress polling. Concurrent subscribers
//! for the same wallet or address collapse into a single database hit instead of each polling.
//! Each entry is served for a short time-to-live, after which the next poll re-queries, so a
//! shared value is at most one time-to-live stale.

use crate::infra::api::ApiResult;
use indexer_common::domain::UnshieldedAddress;
use moka::future::Cache;
use serde::Deserialize;
use std::{future::Future, time::Duration};
use uuid::Uuid;

/// The cached shielded progress: the highest, highest checked and highest relevant zswap
/// end indices.
type ShieldedIndices = (Option<u64>, Option<u64>, Option<u64>);

/// Configuration for the [`ProgressCache`].
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ProgressCacheConfig {
    /// Maximum number of entries, applied independently to the shielded and unshielded caches.
    max_capacity: u64,

    /// How long a cached progress value is served before the next poll re-queries. Kept short
    /// (around the block interval) so a shared value is at most this stale.
    #[serde(with = "humantime_serde")]
    time_to_live: Duration,
}

/// Per-process cache of the most recent progress query result, keyed by wallet ID for
/// shielded subscriptions and by address for unshielded ones.
#[derive(Clone)]
pub struct ProgressCache {
    shielded: Cache<Uuid, ShieldedIndices>,

    /// `UnshieldedAddress` => highest transaction ID for that address.
    unshielded: Cache<UnshieldedAddress, Option<u64>>,
}

impl ProgressCache {
    pub fn new(config: ProgressCacheConfig) -> Self {
        let shielded = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(config.time_to_live)
            .build();
        let unshielded = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(config.time_to_live)
            .build();

        Self {
            shielded,
            unshielded,
        }
    }

    /// Return the cached shielded indices for `wallet_id`, else run `query`, cache and return
    /// its result. Concurrent subscribers for the same wallet collapse into a single database
    /// hit via `try_get_with`; the value is served for at most the configured time-to-live.
    pub async fn shielded_indices(
        &self,
        wallet_id: Uuid,
        query: impl Future<Output = ApiResult<ShieldedIndices>>,
    ) -> ApiResult<ShieldedIndices> {
        self.shielded
            .try_get_with(wallet_id, query)
            .await
            .map_err(|error| (*error).clone())
    }

    /// Return the cached highest transaction ID for `address`, else run `query`, cache and
    /// return its result. Concurrent subscribers for the same address collapse into a single
    /// database hit via `try_get_with`; the value is served for at most the configured
    /// time-to-live.
    pub async fn unshielded_highest_transaction_id(
        &self,
        address: UnshieldedAddress,
        query: impl Future<Output = ApiResult<Option<u64>>>,
    ) -> ApiResult<Option<u64>> {
        self.unshielded
            .try_get_with(address, query)
            .await
            .map_err(|error| (*error).clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::infra::api::{
        ApiError,
        progress_cache::{ProgressCache, ProgressCacheConfig},
    };
    use futures::future::join_all;
    use indexer_common::domain::UnshieldedAddress;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use uuid::Uuid;

    fn config() -> ProgressCacheConfig {
        ProgressCacheConfig {
            max_capacity: 100,
            time_to_live: Duration::from_secs(60),
        }
    }

    #[tokio::test]
    async fn shielded_dedups_within_ttl() {
        let cache = ProgressCache::new(config());
        let wallet_id = Uuid::from_u128(1);
        let calls = AtomicUsize::new(0);
        let query = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ApiError>((Some(1u64), Some(2u64), Some(3u64)))
        };

        let first = cache.shielded_indices(wallet_id, query()).await.unwrap();
        let second = cache.shielded_indices(wallet_id, query()).await.unwrap();

        assert_eq!(first, (Some(1), Some(2), Some(3)));
        assert_eq!(second, first);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second read should hit the cache"
        );
    }

    #[tokio::test]
    async fn shielded_coalesces_concurrent_misses() {
        let cache = ProgressCache::new(config());
        let wallet_id = Uuid::from_u128(1);
        let calls = AtomicUsize::new(0);

        // The slow query keeps the load in flight so all callers overlap on the same cold key.
        let reads = (0..16).map(|_| {
            cache.shielded_indices(wallet_id, async {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok::<_, ApiError>((Some(1u64), Some(2u64), Some(3u64)))
            })
        });

        for result in join_all(reads).await {
            assert_eq!(result.unwrap(), (Some(1), Some(2), Some(3)));
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "concurrent cold misses should coalesce into one query"
        );
    }

    #[tokio::test]
    async fn unshielded_dedups_within_ttl() {
        let cache = ProgressCache::new(config());
        let address = UnshieldedAddress::from([1u8; 32]);
        let calls = AtomicUsize::new(0);
        let query = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ApiError>(Some(7u64))
        };

        let first = cache
            .unshielded_highest_transaction_id(address, query())
            .await
            .unwrap();
        let second = cache
            .unshielded_highest_transaction_id(address, query())
            .await
            .unwrap();

        assert_eq!(first, Some(7));
        assert_eq!(second, first);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second read should hit the cache"
        );
    }

    #[tokio::test]
    async fn unshielded_coalesces_concurrent_misses() {
        let cache = ProgressCache::new(config());
        let address = UnshieldedAddress::from([1u8; 32]);
        let calls = AtomicUsize::new(0);

        let reads = (0..16).map(|_| {
            cache.unshielded_highest_transaction_id(address, async {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok::<_, ApiError>(Some(7u64))
            })
        });

        for result in join_all(reads).await {
            assert_eq!(result.unwrap(), Some(7));
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "concurrent cold misses should coalesce into one query"
        );
    }
}

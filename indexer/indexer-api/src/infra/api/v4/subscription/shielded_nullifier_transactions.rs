// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    domain::storage::Storage,
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{HexEncodable, HexEncoded, directives::beta, transaction::Transaction},
    },
};
use async_graphql::{ComplexObject, Context, SimpleObject, Subscription};
use async_stream::try_stream;
use futures::{Stream, TryStreamExt};
use indexer_common::domain::{BlockIndexed, Subscriber};
use log::{debug, warn};
use std::{marker::PhantomData, pin::pin};

pub struct ShieldedNullifierTransactionsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for ShieldedNullifierTransactionsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

/// A transaction containing a shielded (zswap) nullifier match with block context.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct ShieldedNullifierTransaction<S>
where
    S: Storage,
{
    /// The transaction ID (indexer-internal BIGSERIAL, use as resumption cursor).
    pub transaction_id: u64,
    /// The hex-encoded transaction hash (32-byte chain identifier).
    pub transaction_hash: HexEncoded,
    /// The hex-encoded block hash (use to query block with ledger parameters).
    pub block_hash: HexEncoded,
    /// The block height containing this transaction.
    pub block_height: u32,
    /// The hex-encoded matched nullifier.
    pub nullifier: HexEncoded,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

impl<S> From<crate::domain::ShieldedNullifierTransaction> for ShieldedNullifierTransaction<S>
where
    S: Storage,
{
    fn from(t: crate::domain::ShieldedNullifierTransaction) -> Self {
        Self {
            transaction_id: t.transaction_id,
            transaction_hash: t.transaction_hash.hex_encode(),
            block_hash: t.block_hash.hex_encode(),
            block_height: t.block_height,
            nullifier: t.nullifier.hex_encode(),
            _s: PhantomData,
        }
    }
}

#[ComplexObject]
impl<S> ShieldedNullifierTransaction<S>
where
    S: Storage,
{
    /// The transaction containing this nullifier match.
    #[graphql(directive = beta::apply())]
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        let id = self.transaction_id;
        let transaction = cx
            .get_transaction_by_id_loader::<S>()
            .load_one(id)
            .await
            .map_err_into_server_error(|| format!("get transaction by id {id}"))?
            .some_or_server_error(|| format!("transaction with id {id} not found"))?;

        Ok(transaction.into())
    }
}

#[Subscription]
impl<S, B> ShieldedNullifierTransactionsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to transactions containing shielded (zswap) nullifiers matching the provided
    /// prefixes. Returns transaction and block references for wallet to fetch full data.
    /// If `toBlock` is specified, the subscription finishes after reaching that block.
    async fn shielded_nullifier_transactions<'a>(
        &self,
        cx: &'a Context<'a>,
        nullifier_prefixes: Vec<HexEncoded>,
        from_block: Option<u64>,
        to_block: Option<u64>,
    ) -> impl Stream<Item = ApiResult<ShieldedNullifierTransaction<S>>> {
        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();
        let batch_size = cx
            .get_subscription_config()
            .shielded_nullifier_transactions
            .batch_size;
        let quotas = cx.get_subscription_quotas();
        let per_connection_counter = cx.get_per_connection_counter();

        let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();

        try_stream! {
            let _quota_guard = quotas
                .try_acquire(per_connection_counter, None)
                .map_err_into_client_error(|| "subscription limit exceeded")?;

            (!nullifier_prefixes.is_empty())
                .then_some(())
                .some_or_client_error(|| "nullifierPrefixes must not be empty")?;

            (nullifier_prefixes.len() <= 10)
                .then_some(())
                .some_or_client_error(|| "maximum of ten nullifier prefixes allowed")?;

            let prefix_bytes = nullifier_prefixes
                .iter()
                .map(|p| const_hex::decode(p.as_ref()))
                .collect::<Result<Vec<_>, _>>()
                .map_err_into_client_error(|| "invalid hex-encoded nullifier prefix")?;

            prefix_bytes
                .iter()
                .all(|b| !b.is_empty())
                .then_some(())
                .some_or_client_error(|| "nullifierPrefixes elements must not be empty")?;

            let from = from_block.unwrap_or(0);
            let to = to_block.unwrap_or(u64::MAX);

            validate_block_range(from, to)?;

            debug!("streaming existing shielded nullifier transactions");

            let transactions = storage
                .get_shielded_nullifier_transactions(&prefix_bytes, from, to, batch_size)
                .await;
            let mut transactions = pin!(transactions);
            while let Some(transaction) = transactions
                .try_next()
                .await
                .map_err_into_server_error(|| "get next shielded nullifier transaction")?
            {
                yield transaction.into();
            }

            if let Some(to) = to_block {
                let latest_block = storage
                    .get_latest_block()
                    .await
                    .map_err_into_server_error(|| "get latest block")?;

                if let Some(block) = latest_block
                    && block.height as u64 >= to
                {
                    return;
                }
            }

            debug!("streaming live shielded nullifier transactions");
            let mut block_indexed_stream = pin!(block_indexed_stream);
            while block_indexed_stream
                .try_next()
                .await
                .map_err_into_server_error(|| "get next BlockIndexed event")?
                .is_some()
            {
                let transactions = storage
                    .get_shielded_nullifier_transactions(&prefix_bytes, from, to, batch_size)
                    .await;
                let mut transactions = pin!(transactions);
                while let Some(transaction) = transactions
                    .try_next()
                    .await
                    .map_err_into_server_error(|| "get next shielded nullifier transaction")?
                {
                    yield transaction.into();
                }

                if let Some(to) = to_block {
                    let latest_block = storage
                        .get_latest_block()
                        .await
                        .map_err_into_server_error(|| "get latest block")?;

                    if let Some(block) = latest_block
                        && block.height as u64 >= to
                    {
                        return;
                    }
                }
            }

            warn!("stream of BlockIndexed events completed unexpectedly");
        }
    }
}

/// Reject `fromBlock > toBlock` with a client error so callers get a clear
/// signal that they likely have a bug in their request rather than thinking
/// their query just happens to match nothing (#1095).
fn validate_block_range(from: u64, to: u64) -> ApiResult<()> {
    (from <= to)
        .then_some(())
        .some_or_client_error(|| "fromBlock must not exceed toBlock")
}

#[cfg(test)]
mod tests {
    use super::validate_block_range;

    #[test]
    fn test_validate_block_range_accepts_valid_ranges() {
        assert!(validate_block_range(0, 0).is_ok());
        assert!(validate_block_range(5, 10).is_ok());
        assert!(validate_block_range(0, u64::MAX).is_ok());
    }

    #[test]
    fn test_validate_block_range_rejects_from_greater_than_to() {
        assert!(validate_block_range(1, 0).is_err());
        assert!(validate_block_range(10, 5).is_err());
        assert!(validate_block_range(u64::MAX, 0).is_err());
    }
}

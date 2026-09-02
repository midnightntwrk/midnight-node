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

use super::polling::{jittered, jittered_interval, next_poll_interval};
use crate::{
    domain::{self, LedgerStateCache, storage::Storage},
    infra::api::{
        ApiError, ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{
            HexEncoded, decode_session_id,
            merkle_tree_collapsed_update::{CollapsedMerkleTree, MerkleTreeCollapsedUpdate},
            transaction::RegularTransaction,
        },
    },
};
use async_graphql::{Context, SimpleObject, Subscription, Union};
use async_stream::try_stream;
use derive_more::Debug;

use drop_stream::DropStreamExt;
use fastrace::{Span, future::FutureExt, prelude::SpanContext, trace};
use futures::{
    Stream, StreamExt,
    future::ok,
    stream::{self, TryStreamExt},
};
use indexer_common::domain::{Subscriber, WalletIndexed};
use log::{debug, warn};
use std::{future::ready, marker::PhantomData, pin::pin};
use stream_cancel::{StreamExt as _, Trigger, Tripwire};
use tokio::time::sleep;
use uuid::Uuid;

/// An event of the shielded transactions subscription.
#[derive(Debug, Union)]
enum ShieldedTransactionsEvent<S: Storage> {
    // Boxing RelevantTransaction to reduce variant size (clippy warning).
    RelevantTransaction(Box<RelevantTransaction<S>>),
    ShieldedTransactionsProgress(ShieldedTransactionsProgress),
}

/// A transaction relevant for the subscribing wallet and an optional zswap state Merkle tree
/// collapsed update.
#[derive(Debug, SimpleObject)]
struct RelevantTransaction<S>
where
    S: Storage,
{
    /// A transaction relevant for the subscribing wallet.
    transaction: RegularTransaction<S>,

    /// Only include a zswap state Merkle tree collapsed update if there is a gap between the
    /// current zswap index "driving" the subscription and the zswap start index of the
    /// transaction.
    zswap_collapsed_update: Option<MerkleTreeCollapsedUpdate>,

    /// An optional collapsed Merkle tree.
    #[graphql(deprecation = "Use zswapCollapsedUpdate instead")]
    collapsed_merkle_tree: Option<CollapsedMerkleTree>,
}

/// Information about the shielded transactions indexing progress.
#[derive(Debug, SimpleObject)]
struct ShieldedTransactionsProgress {
    /// The highest end index into the zswap state for all transactions. It represents the known
    /// state of the blockchain. A value of zero (completely unlikely) means that no shielded
    /// transactions have been indexed yet.
    highest_zswap_end_index: u64,

    /// The highest end index into the zswap state for all transactions. It represents the known
    /// state of the blockchain. A value of zero (completely unlikely) means that no shielded
    /// transactions have been indexed yet.
    #[graphql(deprecation = "Use highestZswapEndIndex instead")]
    highest_end_index: u64,

    /// The highest highest end index into the zswap state for all transactions checked for
    /// relevance. Initially less than and eventually (when some wallet has been fully indexed)
    /// equal to `highest_end_index`. A value of zero (very unlikely) means that no wallet
    /// has subscribed before and indexing for the subscribing wallet has not yet started.
    highest_checked_zswap_end_index: u64,

    /// The highest highest end index into the zswap state for all transactions checked for
    /// relevance. Initially less than and eventually (when some wallet has been fully indexed)
    /// equal to `highest_end_index`. A value of zero (very unlikely) means that no wallet
    /// has subscribed before and indexing for the subscribing wallet has not yet started.
    #[graphql(deprecation = "Use highestCheckedZswapEndIndex instead")]
    highest_checked_end_index: u64,

    /// The highest highest end index into the zswap state for all relevant transactions for the
    /// subscribing wallet. Usually less than `highest_checked_end_index` unless the latest
    /// checked transaction is relevant for the subscribing wallet. A value of zero means that
    /// no relevant transactions have been indexed for the subscribing wallet.
    highest_relevant_zswap_end_index: u64,

    /// The highest highest end index into the zswap state for all relevant transactions for the
    /// subscribing wallet. Usually less than `highest_checked_end_index` unless the latest
    /// checked transaction is relevant for the subscribing wallet. A value of zero means that
    /// no relevant transactions have been indexed for the subscribing wallet.
    #[graphql(deprecation = "Use highestRelevantZswapEndIndex instead")]
    highest_relevant_end_index: u64,
}

pub struct ShieldedTransactionsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for ShieldedTransactionsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> ShieldedTransactionsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to shielded transaction events for the given session ID starting at the given
    /// index or at zero if omitted.
    async fn shielded_transactions<'a>(
        &self,
        cx: &'a Context<'a>,
        session_id: HexEncoded,
        index: Option<u64>,
    ) -> Result<impl Stream<Item = ApiResult<ShieldedTransactionsEvent<S>>> + use<'a, S, B>, ApiError>
    {
        cx.get_metrics().wallets_connected.increment(1);

        let session_id =
            decode_session_id(session_id).map_err_into_client_error(|| "invalid session ID")?;

        let quota_guard = cx
            .get_subscription_quotas()
            .try_acquire(cx.get_per_connection_counter(), Some(session_id))
            .map_err_into_client_error(|| "subscription limit exceeded")?;

        let wallet_id = cx
            .get_storage::<S>()
            .resolve_session_id(session_id)
            .await
            .map_err_into_server_error(|| "resolve session ID")?
            .some_or_client_error(|| "unknown or expired session ID")?;
        let index = index.unwrap_or_default();

        // Build a stream of shielded transaction events by merging relevant transactions and
        // progress items. The relevant transactions stream should be infinite by definition (see
        // the trait). However, if it nevertheless completes, we use a tripwire to ensure the
        // progress stream also completes, preventing the merged stream from hanging indefinitely
        // waiting for both streams to complete.
        let (trigger, tripwire) = Tripwire::new();

        let relevant_transactions = make_relevant_transactions::<S, B>(
            cx, wallet_id, index, trigger,
        )
        .map_ok(|relevant_transaction| {
            ShieldedTransactionsEvent::RelevantTransaction(relevant_transaction.into())
        });

        let progress = make_progress::<S>(cx, wallet_id)
            .take_until_if(tripwire)
            .map_ok(ShieldedTransactionsEvent::ShieldedTransactionsProgress)
            .boxed();

        let events = tokio_stream::StreamExt::merge(relevant_transactions, progress);

        // As long as the subscription is alive, the wallet is periodically kept active, even if
        // there are no new transactions.
        let storage = cx.get_storage::<S>();
        let keep_wallet_alive_interval = cx
            .get_subscription_config()
            .shielded_transactions
            .keep_wallet_alive_interval;
        let keep_wallet_active = jittered_interval(keep_wallet_alive_interval)
            .then(move |_| async move { storage.keep_wallet_active(wallet_id).await })
            .map_err(|error| ApiError::server("keep wallet active", error));
        let events = stream::select(events.map_ok(Some), keep_wallet_active.map_ok(|_| None))
            .try_filter_map(ok)
            .on_drop(move || {
                cx.get_metrics().wallets_connected.decrement(1);
                drop(quota_guard);
                debug!(wallet_id:%; "shielded transaction subscription ended");
            });

        Ok(events)
    }
}

fn make_relevant_transactions<'a, S, B>(
    cx: &'a Context<'a>,
    wallet_id: Uuid,
    mut index: u64,
    trigger: Trigger,
) -> impl Stream<Item = ApiResult<RelevantTransaction<S>>> + use<'a, S, B>
where
    S: Storage,
    B: Subscriber,
{
    let storage = cx.get_storage::<S>();
    let subscriber = cx.get_subscriber::<B>();
    let ledger_state_cache = cx.get_ledger_state_cache();
    let batch_size = cx
        .get_subscription_config()
        .shielded_transactions
        .batch_size;

    let wallet_indexed_events = subscriber
        .subscribe::<WalletIndexed>()
        .try_filter(move |wallet_indexed| ready(wallet_indexed.wallet_id == wallet_id));

    try_stream! {
        // Stream exiting transactions.
        debug!(wallet_id:%, index; "streaming existing transactions");

        let transactions = storage.get_relevant_transactions(wallet_id, index, batch_size);
        let mut transactions = pin!(transactions);
        while let Some(transaction) = get_next_transaction(&mut transactions)
            .await
            .map_err_into_server_error(|| "get next transaction")?
        {
            let end_index = transaction.zswap_end_index;

            yield make_relevant_transaction(
                index,
                transaction,
                storage,
                ledger_state_cache,
            )
            .await?;

            // The end index is "exclusive", i.e. the next free index; hence no +1 here!
            index = end_index;
        }

        // Stream live transactions.
        debug!(wallet_id:%, index; "streaming live transactions");
        let mut wallet_indexed_events = pin!(wallet_indexed_events);
        while wallet_indexed_events
            .try_next()
            .await
            .map_err_into_server_error(|| "get next WalletIndexed event")?
            .is_some()
        {
            debug!(index; "streaming next live transactions");

            let transactions =
                storage.get_relevant_transactions(wallet_id, index, batch_size);
            let mut transactions = pin!(transactions);
            while let Some(transaction) =  get_next_transaction(&mut transactions)
                .await
                .map_err_into_server_error(|| "get next transaction")?
            {
                let end_index = transaction.zswap_end_index;

                yield make_relevant_transaction(
                    index,
                    transaction,
                    storage,
                    ledger_state_cache,
                )
                .await?;

                // The end index is "exclusive", i.e. the next free index; hence no +1 here!
                index = end_index;
            }
        }

        warn!("stream of WalletIndexed events completed unexpectedly");
        trigger.cancel();
    }
}

#[trace(properties = { "index": "{index:?}" })]
async fn make_relevant_transaction<S>(
    index: u64,
    transaction: domain::RegularTransaction,
    storage: &S,
    ledger_state_cache: &LedgerStateCache,
) -> ApiResult<RelevantTransaction<S>>
where
    S: Storage,
{
    debug!(index, transaction:?; "making relevant transaction");

    // Only include a zswap state Merkle tree collapsed update if there is a gap between the queried
    // index and the start index of the transaction.
    let zswap_collapsed_update = if index < transaction.zswap_start_index {
        let zswap_collapsed_update = ledger_state_cache
            .make_zswap_collapsed_update(
                index,
                transaction.zswap_start_index - 1,
                storage,
                transaction.protocol_version,
            )
            .await
            .map_err_into_server_error(|| "create zswap state Merkle tree collapsed update")?;
        Some(MerkleTreeCollapsedUpdate::from(zswap_collapsed_update))
    } else {
        None
    };

    let collapsed_merkle_tree = zswap_collapsed_update.as_ref().map(|u| u.to_owned().into());

    let relevant_transaction = RelevantTransaction {
        transaction: transaction.into(),
        zswap_collapsed_update,
        collapsed_merkle_tree,
    };

    debug!(relevant_transaction:?; "made relevant transaction");

    Ok(relevant_transaction)
}

fn make_progress<'a, S>(
    cx: &'a Context<'a>,
    wallet_id: Uuid,
) -> impl Stream<Item = ApiResult<ShieldedTransactionsProgress>> + use<'a, S>
where
    S: Storage,
{
    let storage = cx.get_storage::<S>();
    let progress_cache = cx.get_progress_cache();
    let base = cx
        .get_subscription_config()
        .shielded_transactions
        .progress_update_interval;

    // Emit progress immediately, then re-poll after a jittered interval that
    // backs off while the indices are unchanged (idle) and resets when they move.
    // The cache lets concurrent subscribers for the same wallet share one query.
    let mut current_interval = base;
    let mut last_indices = None;
    try_stream! {
        loop {
            let indices = progress_cache
                .shielded_indices(wallet_id, async {
                    storage
                        .get_highest_zswap_end_indices(wallet_id)
                        .await
                        .map_err_into_server_error(|| "get highest indices")
                })
                .await?;
            current_interval =
                next_poll_interval(current_interval, base, last_indices.as_ref() != Some(&indices));
            last_indices = Some(indices);
            yield to_progress(indices);
            sleep(jittered(current_interval)).await;
        }
    }
}

fn to_progress(
    (highest, highest_checked, highest_relevant): (Option<u64>, Option<u64>, Option<u64>),
) -> ShieldedTransactionsProgress {
    let highest_zswap_end_index = highest.unwrap_or_default();
    let highest_checked_zswap_end_index = highest_checked.unwrap_or_default();
    let highest_relevant_zswap_end_index = highest_relevant.unwrap_or_default();

    ShieldedTransactionsProgress {
        highest_zswap_end_index,
        highest_end_index: highest_zswap_end_index,
        highest_checked_zswap_end_index,
        highest_checked_end_index: highest_checked_zswap_end_index,
        highest_relevant_zswap_end_index,
        highest_relevant_end_index: highest_relevant_zswap_end_index,
    }
}

async fn get_next_transaction<E>(
    transactions: &mut (impl Stream<Item = Result<domain::RegularTransaction, E>> + Unpin),
) -> Result<Option<domain::RegularTransaction>, E> {
    transactions
        .try_next()
        .in_span(Span::root(
            "subscription.shielded-transactions.get-next-transaction",
            SpanContext::random(),
        ))
        .await
}

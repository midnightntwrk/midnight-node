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

use crate::{
    domain::{self, LedgerStateCache, storage::Storage},
    infra::api::{
        ApiError, ApiResult, ContextExt, InnerApiError, ResultExt,
        v1::{
            AsBytesExt, HexEncoded, decode_session_id, subscription::get_next_transaction,
            transaction::Transaction,
        },
    },
};
use async_graphql::{Context, SimpleObject, Subscription, Union, async_stream::try_stream};
use derive_more::Debug;
use drop_stream::DropStreamExt;
use fastrace::trace;
use futures::{
    Stream, StreamExt,
    future::ok,
    stream::{self, TryStreamExt},
};
use indexer_common::domain::{LedgerStateStorage, SessionId, Subscriber, WalletIndexed};
use log::{debug, warn};
use std::{
    future::ready, marker::PhantomData, num::NonZeroU32, pin::pin, sync::Arc, time::Duration,
};
use stream_cancel::{StreamExt as _, Trigger, Tripwire};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

// TODO: Make configurable!
const BATCH_SIZE: NonZeroU32 = NonZeroU32::new(100).unwrap();

// TODO: Make configurable!
const PROGRESS_UPDATES_INTERVAL: Duration = Duration::from_secs(3);

// TODO: Make configurable!
const ACTIVATE_WALLET_INTERVAL: Duration = Duration::from_secs(60);

/// An event of the shielded transactions subscription.
#[derive(Debug, Union)]
pub enum ShieldedTransactionsEvent<S: Storage> {
    // Boxing RelevantTransaction to reduce variant size (clippy warning).
    RelevantTransaction(Box<RelevantTransaction<S>>),
    ShieldedTransactionsProgress(ShieldedTransactionsProgress),
}

/// A transaction relevant for the subscribing wallet and an optional collapsed merkle tree.
#[derive(Debug, SimpleObject)]
pub struct RelevantTransaction<S>
where
    S: Storage,
{
    /// A transaction relevant for the subscribing wallet.
    pub transaction: Transaction<S>,

    /// An optional collapsed merkle tree.
    pub collapsed_merkle_tree: Option<CollapsedMerkleTree>,
}

/// Information about the shielded transactions indexing progress.
#[derive(Debug, SimpleObject)]
pub struct ShieldedTransactionsProgress {
    /// The highest zswap state end index (see `endIndex` of `Transaction`) of all transactions. It
    /// represents the known state of the blockchain. A value of zero (completely unlikely) means
    /// that no shielded transactions have been indexed yet.
    pub highest_end_index: u64,

    /// The highest zswap state end index (see `endIndex` of `Transaction`) of all transactions
    /// checked for relevance. Initially less than and eventually (when some wallet has been fully
    /// indexed) equal to `highest_end_index`. A value of zero (very unlikely) means that no wallet
    /// has subscribed before and indexing for the subscribing wallet has not yet started.
    pub highest_checked_end_index: u64,

    /// The highest zswap state end index (see `endIndex` of `Transaction`) of all relevant
    /// transactions for the subscribing wallet. Usually less than `highest_checked_end_index`
    /// unless the latest checked transaction is relevant for the subscribing wallet. A value of
    /// zero means that no relevant transactions have been indexed for the subscribing wallet.
    pub highest_relevant_end_index: u64,
}

#[derive(Debug, SimpleObject)]
pub struct CollapsedMerkleTree {
    /// The zswap state start index.
    start_index: u64,

    /// The zswap state end index.
    end_index: u64,

    /// The hex-encoded value.
    #[debug(skip)]
    update: HexEncoded,

    /// The protocol version.
    protocol_version: u32,
}

impl From<domain::MerkleTreeCollapsedUpdate> for CollapsedMerkleTree {
    fn from(value: domain::MerkleTreeCollapsedUpdate) -> Self {
        let domain::MerkleTreeCollapsedUpdate {
            start_index,
            end_index,
            update,
            protocol_version,
        } = value;

        Self {
            start_index,
            end_index,
            update: update.hex_encode(),
            protocol_version: protocol_version.0,
        }
    }
}

pub struct ShieldedTransactionsSubscription<S, B, Z> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
    _z: PhantomData<Z>,
}

impl<S, B, Z> Default for ShieldedTransactionsSubscription<S, B, Z> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
            _z: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B, Z> ShieldedTransactionsSubscription<S, B, Z>
where
    S: Storage,
    B: Subscriber,
    Z: LedgerStateStorage,
{
    /// Subscribe shielded transaction events for the given session ID starting at the given index
    /// or at zero if omitted.
    pub async fn shielded_transactions<'a>(
        &self,
        cx: &'a Context<'a>,
        session_id: HexEncoded,
        index: Option<u64>,
    ) -> Result<
        impl Stream<Item = ApiResult<ShieldedTransactionsEvent<S>>> + use<'a, S, B, Z>,
        ApiError,
    > {
        cx.get_metrics().wallets_connected.increment(1);

        let session_id =
            decode_session_id(session_id).map_err_into_client_error(|| "invalid session ID")?;
        let index = index.unwrap_or_default();

        // Build a stream of shielded transaction events by merging relevant transactions and
        // progress items. The relevant transactions stream should be infinite by definition (see
        // the trait). However, if it nevertheless completes, we use a tripwire to ensure the
        // progress stream also completes, preventing the merged stream from hanging indefinitely
        // waiting for both streams to complete.
        let (trigger, tripwire) = Tripwire::new();

        let relevant_transactions = make_relevant_transactions::<S, B, Z>(
            cx, session_id, index, trigger,
        )
        .map_ok(|relevant_transaction| {
            ShieldedTransactionsEvent::RelevantTransaction(relevant_transaction.into())
        });

        let progress = make_progress::<S>(cx, session_id)
            .take_until_if(tripwire)
            .map_ok(ShieldedTransactionsEvent::ShieldedTransactionsProgress)
            .boxed();

        let events = tokio_stream::StreamExt::merge(relevant_transactions, progress);

        // As long as the subscription is alive, the wallet is periodically set active, even if
        // there are no new transactions.
        let storage = cx.get_storage::<S>();
        let set_wallet_active = IntervalStream::new(interval(ACTIVATE_WALLET_INTERVAL))
            .then(move |_| async move { storage.set_wallet_active(session_id).await })
            .map_err(|error| {
                ApiError::Server(InnerApiError(
                    "set wallet active".to_string(),
                    Some(Arc::new(error)),
                ))
            });
        let events = stream::select(events.map_ok(Some), set_wallet_active.map_ok(|_| None))
            .try_filter_map(ok)
            .on_drop(move || {
                cx.get_metrics().wallets_connected.decrement(1);
                debug!(session_id:%; "shielded transaction subscription ended");
            });

        Ok(events)
    }
}

fn make_relevant_transactions<'a, S, B, Z>(
    cx: &'a Context<'a>,
    session_id: SessionId,
    mut index: u64,
    trigger: Trigger,
) -> impl Stream<Item = ApiResult<RelevantTransaction<S>>> + use<'a, S, B, Z>
where
    S: Storage,
    B: Subscriber,
    Z: LedgerStateStorage,
{
    let storage = cx.get_storage::<S>();
    let subscriber = cx.get_subscriber::<B>();
    let ledger_state_storage = cx.get_ledger_state_storage::<Z>();
    let zswap_state_cache = cx.get_ledger_state_cache();

    let wallet_indexed_events = subscriber
        .subscribe::<WalletIndexed>()
        .try_filter(move |wallet_indexed| ready(wallet_indexed.session_id == session_id));

    try_stream! {
        // Stream exiting transactions.
        debug!(session_id:%, index; "streaming existing transactions");

        let transactions = storage.get_relevant_transactions(session_id, index, BATCH_SIZE);
        let mut transactions = pin!(transactions);
        while let Some(transaction) = get_next_transaction(&mut transactions)
            .await
            .map_err_into_server_error(|| "get next transaction")?
        {
            let relevant_transaction = make_relevant_transaction(
                index,
                transaction,
                ledger_state_storage,
                zswap_state_cache,
            )
            .await?;

            index = relevant_transaction.transaction.end_index;

            yield relevant_transaction;
        }

        // Stream live transactions.
        debug!(session_id:%, index; "streaming live transactions");
        let mut wallet_indexed_events = pin!(wallet_indexed_events);
        while wallet_indexed_events
            .try_next()
            .await
            .map_err_into_server_error(|| "get next WalletIndexed event")?
            .is_some()
        {
            debug!(index; "streaming next live transactions");

            let transactions =
                storage.get_relevant_transactions(session_id, index, BATCH_SIZE);
            let mut transactions = pin!(transactions);
            while let Some(transaction) =  get_next_transaction(&mut transactions)
                .await
                .map_err_into_server_error(|| "get next transaction")?
            {
                let relevant_transaction = make_relevant_transaction(
                    index,
                    transaction,
                    ledger_state_storage,
                    zswap_state_cache,
                )
                .await?;

                index = relevant_transaction.transaction.end_index;

                yield relevant_transaction;
            }
        }

        warn!("stream of WalletIndexed events completed unexpectedly");
        trigger.cancel();
    }
}

#[trace(properties = { "from": "{from:?}" })]
async fn make_relevant_transaction<S, Z>(
    from: u64,
    transaction: domain::Transaction,
    ledger_state_storage: &Z,
    zswap_state_cache: &LedgerStateCache,
) -> ApiResult<RelevantTransaction<S>>
where
    S: Storage,
    Z: LedgerStateStorage,
{
    debug!(from, transaction:?; "making relevant transaction");

    let collapsed_merkle_tree = if from == transaction.start_index || transaction.start_index == 0 {
        None
    } else {
        let collapsed_merkle_tree = zswap_state_cache
            .collapsed_update(
                from,
                transaction.start_index - 1,
                ledger_state_storage,
                transaction.protocol_version,
            )
            .await
            .map_err_into_server_error(|| "create collapsed update")?
            .into();
        Some(collapsed_merkle_tree)
    };

    let relevant_transaction = RelevantTransaction {
        transaction: transaction.into(),
        collapsed_merkle_tree,
    };
    debug!(relevant_transaction:?; "made relevant transaction");

    Ok(relevant_transaction)
}

fn make_progress<'a, S>(
    cx: &'a Context<'a>,
    session_id: SessionId,
) -> impl Stream<Item = ApiResult<ShieldedTransactionsProgress>> + use<'a, S>
where
    S: Storage,
{
    let intervals = IntervalStream::new(interval(PROGRESS_UPDATES_INTERVAL));
    intervals.then(move |_| make_progress_update(session_id, cx.get_storage::<S>()))
}

async fn make_progress_update<S>(
    session_id: SessionId,
    storage: &S,
) -> ApiResult<ShieldedTransactionsProgress>
where
    S: Storage,
{
    let (highest_end_index, highest_checked_end_index, highest_relevant_end_index) = storage
        .get_highest_end_indices(session_id)
        .await
        .map_err_into_server_error(|| "get highest indices")?;

    Ok(ShieldedTransactionsProgress {
        highest_end_index: highest_end_index.unwrap_or_default(),
        highest_checked_end_index: highest_checked_end_index.unwrap_or_default(),
        highest_relevant_end_index: highest_relevant_end_index.unwrap_or_default(),
    })
}

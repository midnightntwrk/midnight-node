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

use super::polling::{jittered, next_poll_interval};
use crate::{
    domain::{self, storage::Storage},
    infra::api::{
        ApiError, ApiResult, ContextExt, ResultExt,
        v4::{
            transaction::Transaction,
            unshielded::{UnshieldedAddress, UnshieldedUtxo},
        },
    },
};
use async_graphql::{Context, SimpleObject, Subscription, Union};
use async_stream::try_stream;
use derive_more::Debug;
use drop_stream::DropStreamExt;
use fastrace::{Span, future::FutureExt, prelude::SpanContext, trace};
use futures::{Stream, TryStreamExt};
use indexer_common::domain::{NetworkId, Subscriber, UnshieldedUtxoIndexed};
use log::{debug, warn};
use std::{future::ready, marker::PhantomData, pin::pin};
use stream_cancel::{StreamExt as _, Trigger, Tripwire};
use tokio::time::sleep;

/// An event of the unshielded transactions subscription.
#[derive(Debug, Union)]
enum UnshieldedTransactionsEvent<S: Storage> {
    /// A transaction that created and/or spent UTXOs alongside these and other information.
    // Boxing UnshieldedTransaction to reduce variant size (clippy warning).
    UnshieldedTransaction(Box<UnshieldedTransaction<S>>),

    /// Information about the unshielded indexing progress.
    UnshieldedTransactionsProgress(UnshieldedTransactionsProgress),
}

/// A transaction that created and/or spent UTXOs alongside these and other information.
#[derive(Debug, SimpleObject)]
struct UnshieldedTransaction<S>
where
    S: Storage,
{
    /// The transaction that created and/or spent UTXOs.
    transaction: Transaction<S>,

    /// UTXOs created in the above transaction, possibly empty.
    created_utxos: Vec<UnshieldedUtxo<S>>,

    /// UTXOs spent in the above transaction, possibly empty.
    spent_utxos: Vec<UnshieldedUtxo<S>>,
}

/// Information about the unshielded indexing progress.
#[derive(Debug, SimpleObject)]
struct UnshieldedTransactionsProgress {
    /// The highest transaction ID of all currently known transactions for a subscribed address.
    highest_transaction_id: u64,
}

pub struct UnshieldedTransactionsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for UnshieldedTransactionsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> UnshieldedTransactionsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe unshielded transaction events for the given address and the given transaction ID
    /// or zero if omitted.
    async fn unshielded_transactions<'a>(
        &self,
        cx: &'a Context<'a>,
        address: UnshieldedAddress,
        transaction_id: Option<u64>,
    ) -> Result<
        impl Stream<Item = ApiResult<UnshieldedTransactionsEvent<S>>> + use<'a, S, B>,
        ApiError,
    > {
        let address = address
            .try_into_domain(cx.get_network_id())
            .map_err_into_client_error(|| "invalid address")?;

        let quota_guard = cx
            .get_subscription_quotas()
            .try_acquire(cx.get_per_connection_counter(), None)
            .map_err_into_client_error(|| "subscription limit exceeded")?;

        // Build a stream of unshielded transaction events by merging ViewingUpdates and
        // ProgressUpdates. The ViewingUpdates stream should be infinite by definition (see
        // the trait). However, if it nevertheless completes, we use a Tripwire to ensure
        // the ProgressUpdates stream also completes, preventing the merged stream from
        // hanging indefinitely waiting for both streams to complete.
        let (trigger, tripwire) = Tripwire::new();

        let unshielded_transactions =
            make_unshielded_transactions::<S, B>(cx, address, transaction_id.unwrap_or(0), trigger)
                .map_ok(|update| UnshieldedTransactionsEvent::UnshieldedTransaction(update.into()));

        let progress_updates = progress_updates::<S>(cx, address)
            .take_until_if(tripwire)
            .map_ok(UnshieldedTransactionsEvent::UnshieldedTransactionsProgress);

        let events = tokio_stream::StreamExt::merge(unshielded_transactions, progress_updates)
            .on_drop(move || {
                drop(quota_guard);
            });

        Ok(events)
    }
}

fn make_unshielded_transactions<'a, S, B>(
    cx: &'a Context<'a>,
    address: indexer_common::domain::UnshieldedAddress,
    mut transaction_id: u64,
    trigger: Trigger,
) -> impl Stream<Item = ApiResult<UnshieldedTransaction<S>>> + use<'a, S, B>
where
    S: Storage,
    B: Subscriber,
{
    let network_id = cx.get_network_id();
    let storage = cx.get_storage::<S>();
    let subscriber = cx.get_subscriber::<B>();
    let batch_size = cx
        .get_subscription_config()
        .unshielded_transactions
        .batch_size;

    let utxo_indexed_events = subscriber
        .subscribe::<UnshieldedUtxoIndexed>()
        .try_filter(move |event| ready(event.address == address));

    try_stream! {
        // Stream UTXO events for existing transactions.
        debug!(address:?, transaction_id; "streaming events for existing transactions");

        let transactions =
            storage.get_transactions_by_unshielded_address(address, transaction_id, batch_size);
        let mut transactions = pin!(transactions);
        while let Some(transaction) = get_next_transaction(&mut transactions)
            .await
            .map_err_into_server_error(|| format!("get next transaction for address {address}"))?
        {
            if let Some(unshielded_transaction) = make_unshielded_transaction(
                &mut transaction_id,
                storage,
                address,
                transaction,
                network_id,
            )
            .await?
            {
                yield unshielded_transaction;
            }
        }

        // Stream UTXO events for live transactions.
        debug!(address:?, transaction_id; "streaming events for live transactions");
        let mut utxo_indexed_events = pin!(utxo_indexed_events);
        while utxo_indexed_events
            .try_next()
            .await
            .map_err_into_server_error(|| "get next UnshieldedUtxoIndexed event")?
            .is_some()
        {
            let transactions =
                storage.get_transactions_by_unshielded_address(address, transaction_id, batch_size);
            let mut transactions = pin!(transactions);
            while let Some(transaction) =
                get_next_transaction(&mut transactions)
                    .await
                    .map_err_into_server_error(|| {
                        format!("get next transaction for address {address}")
                    })?
            {
                if let Some(unshielded_transaction) = make_unshielded_transaction(
                    &mut transaction_id,
                    storage,
                    address,
                    transaction,
                    network_id,
                )
                .await?
                {
                    yield unshielded_transaction;
                }
            }
        }

        warn!("stream of UnshieldedUtxoIndexed events completed unexpectedly");
        trigger.cancel();
    }
}

#[trace(properties = { "transaction_id": "{transaction_id}", "address": "{address:?}" })]
async fn make_unshielded_transaction<S>(
    transaction_id: &mut u64,
    storage: &S,
    address: indexer_common::domain::UnshieldedAddress,
    transaction: domain::Transaction,
    network_id: &NetworkId,
) -> ApiResult<Option<UnshieldedTransaction<S>>>
where
    S: Storage,
{
    let id = transaction.id();
    *transaction_id = id + 1;

    let created = storage
        .get_unshielded_utxos_by_address_created_by_transaction(address, id)
        .await
        .map_err_into_server_error(|| format!("get created UTXOs for transaction with ID {id}"))?;

    let spent = storage
        .get_unshielded_utxos_by_address_spent_by_transaction(address, id)
        .await
        .map_err_into_server_error(|| format!("get spent UTXOs for transaction with ID {id}"))?;

    // Only emit an event for transactions that have UTXOs for this address.
    let unshielded_utxo_update = (!created.is_empty() || !spent.is_empty()).then(|| {
        let created_utxos = created
            .into_iter()
            .map(|utxo| UnshieldedUtxo::<S>::from((utxo, network_id)))
            .collect();

        let spent_utxos = spent
            .into_iter()
            .map(|utxo| UnshieldedUtxo::<S>::from((utxo, network_id)))
            .collect();

        UnshieldedTransaction {
            transaction: transaction.into(),
            created_utxos,
            spent_utxos,
        }
    });

    Ok(unshielded_utxo_update)
}

fn progress_updates<'a, S>(
    cx: &'a Context<'a>,
    address: indexer_common::domain::UnshieldedAddress,
) -> impl Stream<Item = ApiResult<UnshieldedTransactionsProgress>> + use<'a, S>
where
    S: Storage,
{
    let storage = cx.get_storage::<S>();
    let progress_cache = cx.get_progress_cache();
    let base = cx
        .get_subscription_config()
        .unshielded_transactions
        .progress_update_interval;

    // Emit progress immediately, then re-poll after a jittered interval that
    // backs off while the highest transaction ID is unchanged (idle) and resets
    // when it moves. The cache lets concurrent subscribers for the same address
    // share one query.
    let mut current_interval = base;
    let mut last_highest_transaction_id = None;
    try_stream! {
        loop {
            let highest_transaction_id = progress_cache
                .unshielded_highest_transaction_id(address, async {
                    storage
                        .get_highest_transaction_id_for_unshielded_address(address)
                        .await
                        .map_err_into_server_error(|| "get highest transaction ID for address")
                })
                .await?
                .unwrap_or(0);
            current_interval = next_poll_interval(
                current_interval,
                base,
                last_highest_transaction_id != Some(highest_transaction_id),
            );
            last_highest_transaction_id = Some(highest_transaction_id);
            yield UnshieldedTransactionsProgress {
                highest_transaction_id,
            };
            sleep(jittered(current_interval)).await;
        }
    }
}

async fn get_next_transaction<E>(
    transactions: &mut (impl Stream<Item = Result<domain::Transaction, E>> + Unpin),
) -> Result<Option<domain::Transaction>, E> {
    transactions
        .try_next()
        .in_span(Span::root(
            "subscription.unshielded-transactions.get-next-transaction",
            SpanContext::random(),
        ))
        .await
}

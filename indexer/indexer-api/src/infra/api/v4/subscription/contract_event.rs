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

//! Subscription resolver for `contractEvents` (ticket #1161).
//!
//! Starting cursor: either the `id` argument (resume from a previous event) or
//! `filter.fromBlock` (start from the first event in that block); if both are set, the
//! effective cursor is the later of the two. Pattern follows `dustLedgerEvents` /
//! `zswapLedgerEvents` for the id-cursor path.
//!
//! `filter.toBlock`, if set, completes the stream once the chain has reached that block,
//! matching the bounded-subscription pattern of `dust_nullifier_transactions` /
//! `shielded_nullifier_transactions`.

use crate::{
    domain::{ContractEventRow, storage::Storage},
    infra::api::{
        ApiError, ApiResult, ContextExt, ResultExt,
        v4::{
            contract_event::{ContractEvent, ContractEventFilter},
            directives::beta,
        },
    },
};
use async_graphql::{Context, Subscription};
use async_stream::try_stream;
use fastrace::{Span, future::FutureExt, prelude::SpanContext};
use futures::{Stream, TryStreamExt};
use indexer_common::domain::{BlockIndexed, Subscriber};
use log::{debug, warn};
use std::{marker::PhantomData, pin::pin};

pub struct ContractEventsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for ContractEventsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> ContractEventsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to contract events matching the given filter, starting at the given event ID
    /// (inclusive) and returning events in monotonic ID order; completes once the chain has
    /// reached `filter.toBlock` if that is set.
    #[graphql(directive = beta::apply())]
    async fn contract_events<'a>(
        &self,
        cx: &'a Context<'a>,
        filter: ContractEventFilter,
        id: Option<u64>,
    ) -> Result<impl Stream<Item = ApiResult<ContractEvent<S>>> + use<'a, S, B>, ApiError> {
        let filter = filter
            .into_domain()
            .map_err(|error| ApiError::client("invalid contract event filter", error))?;

        let id = id.unwrap_or(0);
        // Event IDs are i64 in the database; reject IDs beyond that range up front instead of
        // letting the SQL bind wrap negative and replay the full history.
        i64::try_from(id).map_err_into_client_error(|| "id out of range")?;

        let quota_guard = cx
            .get_subscription_quotas()
            .try_acquire(cx.get_per_connection_counter(), None)
            .map_err_into_client_error(|| "subscription limit exceeded")?;

        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();
        let batch_size = cx.get_subscription_config().contract_events.batch_size;
        let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();

        // Bounded subscription: when `filter.toBlock` is set, evaluate the terminator BEFORE
        // each drain — the drain's DB snapshot then covers everything the check saw, so events
        // committed between snapshot and check cannot be lost — and complete after the final
        // drain.
        let to_block = filter.to_block;

        let contract_events = try_stream! {
            let _hold = quota_guard;
            let mut id = id;

            // Stream existing contract events.
            let last_round = reached_to_block(storage, to_block).await?;
            debug!(id; "streaming existing contract events");
            let rows = storage.get_contract_events_from_id(&filter, id, batch_size).await;
            let mut rows = pin!(rows);
            while let Some(row) = get_next_row(&mut rows)
                .await
                .map_err_into_server_error(|| format!("get next contract event at id {id}"))?
            {
                let event_id = row.id;
                id = event_id + 1;
                yield ContractEvent::try_from(row).map_err_into_server_error(|| {
                    format!("unexpected contract event row at id {event_id}")
                })?;
            }
            if last_round {
                debug!(to_block:?; "contractEvents subscription completed at toBlock");
                return;
            }

            // Stream live contract events.
            debug!(id; "streaming live contract events");
            let mut block_indexed_stream = pin!(block_indexed_stream);
            while let Some(BlockIndexed { height, .. }) = block_indexed_stream
                .try_next()
                .await
                .map_err_into_server_error(|| "get next BlockIndexed event")?
            {
                let last_round =
                    to_block.is_some_and(|to_block| height >= u64::from(to_block));

                let rows = storage.get_contract_events_from_id(&filter, id, batch_size).await;
                let mut rows = pin!(rows);
                while let Some(row) = get_next_row(&mut rows)
                    .await
                    .map_err_into_server_error(|| {
                        format!("get next contract event at id {id}")
                    })?
                {
                    let event_id = row.id;
                    id = event_id + 1;
                    yield ContractEvent::try_from(row).map_err_into_server_error(|| {
                        format!("unexpected contract event row at id {event_id}")
                    })?;
                }

                if last_round {
                    debug!(to_block:?; "contractEvents subscription completed at toBlock");
                    return;
                }
            }

            warn!("stream of BlockIndexed events completed unexpectedly");
        };

        Ok(contract_events)
    }
}

async fn get_next_row<E>(
    rows: &mut (impl Stream<Item = Result<ContractEventRow, E>> + Unpin),
) -> Result<Option<ContractEventRow>, E> {
    rows.try_next()
        .in_span(Span::root(
            "subscription.contract-events.get-next-row",
            SpanContext::random(),
        ))
        .await
}

/// Whether the chain has already reached `to_block`; used before the initial drain so the
/// bounded subscription can complete after it.
async fn reached_to_block<S: Storage>(storage: &S, to_block: Option<u32>) -> ApiResult<bool> {
    let Some(to_block) = to_block else {
        return Ok(false);
    };

    let latest = storage
        .get_latest_block()
        .await
        .map_err_into_server_error(|| "get latest block to evaluate toBlock terminator")?;

    Ok(latest.is_some_and(|block| block.height >= to_block))
}

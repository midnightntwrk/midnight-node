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

use crate::{
    domain::{LedgerEvent, storage::Storage},
    infra::api::{ApiResult, ContextExt, ResultExt, v4::ledger_events::DustLedgerEvent},
};
use async_graphql::{Context, Subscription};
use async_stream::try_stream;
use fastrace::{Span, future::FutureExt, prelude::SpanContext};
use futures::{Stream, TryStreamExt};
use indexer_common::domain::{BlockIndexed, LedgerEventGrouping, Subscriber};
use log::{debug, warn};
use std::{marker::PhantomData, pin::pin};

pub struct DustLedgerEventsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for DustLedgerEventsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> DustLedgerEventsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to dust ledger events starting at the given ID or at the very start if omitted.
    async fn dust_ledger_events<'a>(
        &self,
        cx: &'a Context<'a>,
        id: Option<u64>,
    ) -> impl Stream<Item = ApiResult<DustLedgerEvent>> {
        let mut id = id.unwrap_or(1);
        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();
        let batch_size = cx.get_subscription_config().dust_ledger_events.batch_size;
        let quotas = cx.get_subscription_quotas();
        let per_connection_counter = cx.get_per_connection_counter();

        let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();

        try_stream! {
            let _quota_guard = quotas
                .try_acquire(per_connection_counter, None)
                .map_err_into_client_error(|| "subscription limit exceeded")?;

            debug!(id; "streaming existing events");

            let ledger_events = storage
                .get_ledger_events(LedgerEventGrouping::Dust, id, batch_size)
                .await;
            let mut ledger_events = pin!(ledger_events);
            while let Some(ledger_event) = get_next_ledger_event(&mut ledger_events)
                .await
                .map_err_into_server_error(|| format!("get next ledger event at id {id}"))?
            {
                let ledger_event_id = ledger_event.id;
                id = ledger_event_id + 1;
                yield ledger_event.try_into().map_err_into_server_error(|| {
                    format!("unexpected dust ledger event with ID {ledger_event_id}")
                })?;
            }

            debug!(id; "streaming live events");
            let mut block_indexed_stream = pin!(block_indexed_stream);
            while block_indexed_stream
                .try_next()
                .await
                .map_err_into_server_error(|| "get next BlockIndexed event")?
                .is_some()
            {
                debug!(id; "streaming next events");

                let ledger_events = storage
                    .get_ledger_events(LedgerEventGrouping::Dust, id, batch_size)
                    .await;
                let mut ledger_events = pin!(ledger_events);
                while let Some(ledger_event) = get_next_ledger_event(&mut ledger_events)
                    .await
                    .map_err_into_server_error(|| format!("get next ledger event at id {id}"))?
                {
                    let ledger_event_id = ledger_event.id;
                    id = ledger_event_id + 1;
                    yield ledger_event.try_into().map_err_into_server_error(|| {
                        format!("unexpected dust ledger event with ID {ledger_event_id}")
                    })?;
                }
            }

            warn!("stream of BlockIndexed events completed unexpectedly");
        }
    }
}

async fn get_next_ledger_event<E>(
    ledger_events: &mut (impl Stream<Item = Result<LedgerEvent, E>> + Unpin),
) -> Result<Option<LedgerEvent>, E> {
    ledger_events
        .try_next()
        .in_span(Span::root(
            "subscription.dust-ledger-events.get-next-ledger-event",
            SpanContext::random(),
        ))
        .await
}

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
    domain::{
        bridge as domain_bridge,
        storage::{Storage, bridge::BridgeEventFilter},
    },
    infra::api::{
        ApiError, ApiResult, ContextExt, ResultExt,
        v4::{
            HexEncoded,
            bridge::{BridgeBalance, BridgeEvent, BridgeEventVariant, BridgePoolSummary},
            directives::beta,
        },
    },
};
use async_graphql::{Context, SimpleObject, Subscription};
use async_stream::try_stream;
use futures::{Stream, TryStreamExt, stream};
use indexer_common::domain::{
    BridgeEventIndexed, Subscriber, UnshieldedAddress, UnshieldedUtxoIndexed, bridge,
};
use std::{future::ready, marker::PhantomData, pin::pin};

const BACKFILL_BATCH: u64 = 100;

/// Pair of latest bridge event and refreshed pool summary, emitted by `bridgePoolUpdates`.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct BridgePoolUpdate {
    /// The triggering event, or None for the initial snapshot on subscribe.
    pub new_event: Option<BridgeEvent>,
    pub pool: BridgePoolSummary,
}

/// Synthesise a `domain_bridge::BridgeEvent` from a pub-sub message so the subscription can emit
/// the same `BridgeEvent` interface used elsewhere. The `id` and `transaction_id` fields are
/// zero-valued/absent when sourced from pub-sub since the message carries the event
/// payload and block height but not the persisted-row identifiers; consumers needing them should
/// read from the live tail of `bridgeEvents` instead.
fn synthesise_event(msg: BridgeEventIndexed) -> domain_bridge::BridgeEvent {
    let variant = msg.event.variant();
    let mc_tx_hash = msg.event.mc_tx_hash().cloned();
    let amount = msg.event.amount();
    let recipient = msg.event.recipient().cloned();
    let midnight_tx_hash = *msg.event.midnight_tx_hash();
    let count = match msg.event {
        bridge::BridgeEvent::SubminimalFlushTransfer { count, .. } => Some(count),
        _ => None,
    };
    domain_bridge::BridgeEvent {
        id: 0,
        block_height: msg.block_height,
        transaction_id: None,
        variant,
        mc_tx_hash,
        amount,
        recipient,
        midnight_tx_hash,
        count,
    }
}

pub struct BridgeEventsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for BridgeEventsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> BridgeEventsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to c2m-bridge events.
    ///
    /// Backfills events with id > `from` then live-tails new events. Filters apply across both
    /// phases.
    #[graphql(directive = beta::apply())]
    async fn bridge_events<'a>(
        &self,
        cx: &'a Context<'a>,
        from: Option<u64>,
        recipient: Option<HexEncoded>,
        variant: Option<BridgeEventVariant>,
    ) -> Result<impl Stream<Item = ApiResult<BridgeEvent>> + use<'a, S, B>, ApiError> {
        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();
        let recipient = recipient
            .map(|h| h.hex_decode::<UnshieldedAddress>())
            .transpose()
            .map_err_into_client_error(|| "invalid recipient address")?;
        let domain_variant = variant.map(Into::into);

        let stream = try_stream! {
            let mut last_id = from.unwrap_or(0);

            loop {
                let filter = BridgeEventFilter {
                    variants: domain_variant.into_iter().collect(),
                    recipient,
                    block_height_from: None,
                    block_height_to: None,
                    id_from: Some(last_id),
                };
                let events = storage
                    .get_bridge_events(&filter, 0, BACKFILL_BATCH)
                    .await
                    .map_err_into_server_error(|| "get bridge events")?;

                if events.is_empty() {
                    break;
                }
                for event in events {
                    last_id = last_id.max(event.id);
                    yield BridgeEvent::from(event);
                }
            }

            // Live tail.
            let live = subscriber
                .subscribe::<BridgeEventIndexed>()
                .try_filter(move |evt| {
                    let recipient_match = recipient
                        .map(|r| evt.event.recipient().map(|er| er.as_bytes() == r.as_ref()).unwrap_or(false))
                        .unwrap_or(true);
                    let variant_match = domain_variant
                        .map(|v| evt.event.variant() == v)
                        .unwrap_or(true);
                    ready(recipient_match && variant_match)
                });
            let mut live = pin!(live);
            while let Some(_msg) = live.try_next().await
                .map_err_into_server_error(|| "subscribe BridgeEventIndexed")?
            {
                let filter = BridgeEventFilter {
                    variants: domain_variant.into_iter().collect(),
                    recipient,
                    block_height_from: None,
                    block_height_to: None,
                    id_from: Some(last_id),
                };
                let events = storage
                    .get_bridge_events(&filter, 0, BACKFILL_BATCH)
                    .await
                    .map_err_into_server_error(|| "get bridge events (live)")?;
                for event in events {
                    last_id = last_id.max(event.id);
                    yield BridgeEvent::from(event);
                }
            }
        };

        Ok(stream)
    }

    /// Subscribe to bridge pool updates. Emits a snapshot of the pool summary alongside each
    /// pool-affecting event (Reserve, Invalid, Unapproved, SubminimalFlush). Useful for
    /// observability dashboards.
    #[graphql(directive = beta::apply())]
    async fn bridge_pool_updates<'a>(
        &self,
        cx: &'a Context<'a>,
    ) -> Result<impl Stream<Item = ApiResult<BridgePoolUpdate>> + use<'a, S, B>, ApiError> {
        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();

        let stream = try_stream! {
            // Initial snapshot.
            let initial = storage
                .get_bridge_pool_summary(None)
                .await
                .map_err_into_server_error(|| "get bridge pool summary")?;
            yield BridgePoolUpdate { new_event: None, pool: BridgePoolSummary::from(initial) };

            let live = subscriber
                .subscribe::<BridgeEventIndexed>()
                .try_filter(|evt| {
                    use indexer_common::domain::bridge::BridgeEventVariant::*;
                    let interesting = matches!(
                        evt.event.variant(),
                        ReserveTransfer | InvalidTransfer | UnapprovedTransfer | SubminimalFlushTransfer
                    );
                    ready(interesting)
                });
            let mut live = pin!(live);
            while let Some(msg) = live.try_next().await
                .map_err_into_server_error(|| "subscribe BridgeEventIndexed")?
            {
                let pool = storage
                    .get_bridge_pool_summary(None)
                    .await
                    .map_err_into_server_error(|| "get bridge pool summary (live)")?;
                yield BridgePoolUpdate {
                    new_event: Some(BridgeEvent::from(synthesise_event(msg))),
                    pool: BridgePoolSummary::from(pool),
                };
            }
        };

        Ok(stream)
    }

    /// Subscribe to a recipient's bridge balance. Emits the current balance on subscribe and
    /// re-emits whenever a bridge event for the recipient indexes or the recipient's unshielded
    /// UTXOs change (a claim emits no bridge event, but it creates unshielded UTXOs for
    /// the recipient).
    #[graphql(directive = beta::apply())]
    async fn bridge_balance<'a>(
        &self,
        cx: &'a Context<'a>,
        address: HexEncoded,
    ) -> Result<impl Stream<Item = ApiResult<BridgeBalance>> + use<'a, S, B>, ApiError> {
        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();
        let address = address
            .hex_decode::<UnshieldedAddress>()
            .map_err_into_client_error(|| "invalid recipient address")?;

        let stream = try_stream! {
            // Emit initial balance. `balance` is overridden with the authoritative claimable from
            // the ledger's `bridge_receiving` map (see the `bridge_balance` query for rationale).
            let mut initial = storage
                .get_bridge_balance(address)
                .await
                .map_err_into_server_error(|| "get bridge balance")?;
            initial.balance = match storage
                .get_highest_ledger_state()
                .await
                .map_err_into_server_error(|| "get highest ledger state")?
            {
                Some((protocol_version, ledger_state_key)) => {
                    indexer_common::domain::ledger::LedgerState::load(
                        &ledger_state_key,
                        protocol_version.ledger_version(),
                    )
                    .map_err_into_server_error(|| "load ledger state")?
                    .bridge_receiving(address)
                }
                None => 0,
            };
            yield BridgeBalance::from(initial);

            // Re-emit on each relevant pub-sub event. Bridge pallet events cover the deposit
            // side; a claim is a regular `ClaimRewards` transaction that emits no pallet event,
            // so also wake on unshielded UTXO activity for the address (a claim creates
            // unshielded UTXOs for the recipient).
            let deposits = subscriber
                .subscribe::<BridgeEventIndexed>()
                .try_filter(move |evt| {
                    let matches = evt
                        .event
                        .recipient()
                        .map(|r| r.as_bytes() == address.as_ref())
                        .unwrap_or(false);
                    ready(matches)
                })
                .map_ok(|_| ());
            let claims = subscriber
                .subscribe::<UnshieldedUtxoIndexed>()
                .try_filter(move |utxo| ready(utxo.address == address))
                .map_ok(|_| ());
            let live = stream::select(deposits, claims);
            let mut live = pin!(live);
            while let Some(_msg) = live.try_next().await
                .map_err_into_server_error(|| "subscribe bridge balance updates")?
            {
                let mut updated = storage
                    .get_bridge_balance(address)
                    .await
                    .map_err_into_server_error(|| "get bridge balance (live)")?;
                updated.balance = match storage
                    .get_highest_ledger_state()
                    .await
                    .map_err_into_server_error(|| "get highest ledger state (live)")?
                {
                    Some((protocol_version, ledger_state_key)) => {
                        indexer_common::domain::ledger::LedgerState::load(
                            &ledger_state_key,
                            protocol_version.ledger_version(),
                        )
                        .map_err_into_server_error(|| "load ledger state (live)")?
                        .bridge_receiving(address)
                    }
                    None => 0,
                };
                yield BridgeBalance::from(updated);
            }
        };

        Ok(stream)
    }
}

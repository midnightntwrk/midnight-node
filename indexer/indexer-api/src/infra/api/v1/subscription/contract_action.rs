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
    domain::{self, storage::Storage},
    infra::api::{
        ApiError, ApiResult, ContextExt, ResultExt,
        v1::{HexEncoded, block::BlockOffset, contract_action::ContractAction, resolve_height},
    },
};
use async_graphql::{Context, Subscription, async_stream::try_stream};
use fastrace::{Span, future::FutureExt, prelude::SpanContext};
use futures::{Stream, TryStreamExt};
use indexer_common::domain::{BlockIndexed, Subscriber};
use log::{debug, warn};
use std::{num::NonZeroU32, pin::pin};

// TODO: Make configurable!
const BATCH_SIZE: NonZeroU32 = NonZeroU32::new(100).unwrap();

pub struct ContractActionSubscription<S, B> {
    _storage: std::marker::PhantomData<S>,
    _subscriber: std::marker::PhantomData<B>,
}

impl<S, B> Default for ContractActionSubscription<S, B> {
    fn default() -> Self {
        Self {
            _storage: std::marker::PhantomData,
            _subscriber: std::marker::PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> ContractActionSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to contract actions with the given address starting at the given offset or at the
    /// latest block if the offset is omitted.
    async fn contract_actions<'a>(
        &self,
        cx: &'a Context<'a>,
        address: HexEncoded,
        offset: Option<BlockOffset>,
    ) -> Result<impl Stream<Item = ApiResult<ContractAction<S>>> + use<'a, S, B>, ApiError> {
        let address = address
            .hex_decode()
            .map_err_into_client_error(|| "invalid address")?;

        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();

        let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();
        let height = resolve_height(offset, storage).await?;
        let mut contract_action_id = storage
            .get_contract_action_id_by_block_height(height)
            .await
            .map_err_into_server_error(|| {
                format!("get contract action id by block height {height}")
            })?
            .unwrap_or_default();

        let contract_actions = try_stream! {
            // Stream existing contract actions.
            debug!(contract_action_id; "streaming existing contract actions");

            let contract_actions = storage.get_contract_actions_by_address(
                &address,
                contract_action_id,
                BATCH_SIZE,
            );
            let mut contract_actions = pin!(contract_actions);
            while let Some(contract_action) = get_next_contract_action(&mut contract_actions)
                .await
                .map_err_into_server_error(|| {
                    format!("get next contract action for ID {contract_action_id}")
                })?
            {
                contract_action_id = contract_action.id + 1;

                yield contract_action.into();
            }

            // Stream live contract actions.
            debug!(contract_action_id; "streaming live contract actions");
            let mut block_indexed_stream = pin!(block_indexed_stream);
            while let Some(BlockIndexed { height, .. }) = block_indexed_stream
                .try_next()
                .await
                .map_err_into_server_error(|| "get next BlockIndexed event")?
            {
                debug!(height; "streaming next contract actions");

                let contract_actions = storage.get_contract_actions_by_address(
                    &address,
                    contract_action_id,
                    BATCH_SIZE,
                );
                let mut contract_actions = pin!(contract_actions);

                while let Some(contract_action) = get_next_contract_action(&mut contract_actions)
                    .await
                    .map_err_into_server_error(|| {
                        format!("get next contract action for ID {contract_action_id}")
                    })?
                {
                    contract_action_id = contract_action.id + 1;

                    yield contract_action.into();
                }
            }

            warn!("stream of BlockIndexed events completed unexpectedly");
        };

        Ok(contract_actions)
    }
}

async fn get_next_contract_action<E>(
    contract_actions: &mut (impl Stream<Item = Result<domain::ContractAction, E>> + Unpin),
) -> Result<Option<domain::ContractAction>, E> {
    contract_actions
        .try_next()
        .in_span(Span::root(
            "subscription.contract-actions.get-next-contract-action",
            SpanContext::random(),
        ))
        .await
}

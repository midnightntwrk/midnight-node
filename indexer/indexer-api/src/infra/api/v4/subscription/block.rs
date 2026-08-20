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
    domain::{self, storage::Storage},
    infra::api::{
        ApiError, ApiResult, ContextExt, ResultExt,
        v4::{
            block::{Block, BlockOffset},
            resolve_height,
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

pub struct BlockSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for BlockSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

#[Subscription]
impl<S, B> BlockSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to blocks starting at the given offset or at the latest block if the offset is
    /// omitted.
    async fn blocks<'a>(
        &self,
        cx: &'a Context<'a>,
        offset: Option<BlockOffset>,
    ) -> Result<impl Stream<Item = ApiResult<Block<S>>> + use<'a, S, B>, ApiError> {
        let quota_guard = cx
            .get_subscription_quotas()
            .try_acquire(cx.get_per_connection_counter(), None)
            .map_err_into_client_error(|| "subscription limit exceeded")?;

        let storage = cx.get_storage::<S>();
        let subscriber = cx.get_subscriber::<B>();
        let batch_size = cx.get_subscription_config().blocks.batch_size;

        let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();
        let mut height = resolve_height::<S>(offset, cx).await?;

        let blocks = try_stream! {
            let _hold = quota_guard;

            // Stream existing blocks.
            debug!(height; "streaming existing blocks");

            let blocks = storage.get_blocks(height, batch_size);
            let mut blocks = pin!(blocks);
            while let Some(block) = get_next_block(&mut blocks)
                .await
                .map_err_into_server_error(|| format!("get next block at height {height}"))?
            {
                height = block.height + 1;
                yield block.into();
            }

            // Stream live blocks.
            debug!(height; "streaming live blocks");
            let mut block_indexed_stream = pin!(block_indexed_stream);
            while block_indexed_stream
                .try_next()
                .await
                .map_err_into_server_error(|| "get next BlockIndexed event")?
                .is_some()
            {
                debug!(height; "streaming next blocks");

                let blocks = storage.get_blocks(height, batch_size);
                let mut blocks = pin!(blocks);
                while let Some(block) = get_next_block(&mut blocks)
                    .await
                    .map_err_into_server_error(|| format!("get next block at height {height}"))?
                {
                    height = block.height + 1;
                    yield block.into();
                }
            }

            warn!("stream of BlockIndexed events completed unexpectedly");
        };

        Ok(blocks)
    }
}

async fn get_next_block<E>(
    blocks: &mut (impl Stream<Item = Result<domain::Block, E>> + Unpin),
) -> Result<Option<domain::Block>, E> {
    blocks
        .try_next()
        .in_span(Span::root(
            "subscription.blocks.get-next-block",
            SpanContext::random(),
        ))
        .await
}

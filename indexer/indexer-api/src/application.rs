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

use crate::domain::Api;
use anyhow::Context as AnyhowContext;
use futures::{TryStreamExt, future::ok};
use indexer_common::domain::{BlockIndexed, NetworkId, Subscriber};
use log::{debug, warn};
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{select, signal::unix::Signal, task};

#[serde_as]
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Config {
    #[serde_as(as = "DisplayFromStr")]
    pub network_id: NetworkId,
}

pub async fn run(
    config: Config,
    api: impl Api,
    subscriber: impl Subscriber,
    mut sigterm: Signal,
) -> anyhow::Result<()> {
    let Config { network_id } = config;

    let caught_up = Arc::new(AtomicBool::new(false));

    let block_indexed_task = task::spawn({
        let subscriber = subscriber.clone();
        let caught_up = caught_up.clone();

        async move {
            let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();

            block_indexed_stream
                .try_for_each(|block_indexed| {
                    debug!(caught_up = block_indexed.caught_up; "received block indexed event");
                    caught_up.store(block_indexed.caught_up, Ordering::Release);
                    ok(())
                })
                .await
                .context("cannot get next BlockIndexed event")?;

            Ok::<(), anyhow::Error>(())
        }
    });

    let serve_api_task = {
        task::spawn(async move {
            api.serve(network_id, caught_up)
                .await
                .context("serving API")
        })
    };

    select! {
        result = block_indexed_task => result
            .context("block_indexed_task panicked")
            .and_then(|r| r.context("block_indexed_task failed")),

        result = serve_api_task => result
            .context("serve_api_task panicked")
            .and_then(|r| r.context("serve_api_task failed")),

        _ = sigterm.recv() => {
            warn!("SIGTERM received");
            Ok(())
        }
    }
}

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

use crate::domain::storage::Storage;
use anyhow::Context;
use fastrace::trace;
use futures::{Stream, StreamExt, TryStreamExt, future::ok, stream};
use indexer_common::domain::{BlockIndexed, Publisher, Subscriber, WalletIndexed};
use itertools::Itertools;
use log::warn;
use serde::Deserialize;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{select, signal::unix::Signal, task};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(with = "humantime_serde")]
    pub active_wallets_repeat_delay: Duration,

    #[serde(with = "humantime_serde")]
    pub active_wallets_ttl: Duration,

    pub transaction_batch_size: NonZeroUsize,

    #[serde(default = "parallelism_default")]
    pub parallelism: NonZeroUsize,
}

pub async fn run(
    config: Config,
    storage: impl Storage,
    publisher: impl Publisher,
    subscriber: impl Subscriber,
    mut sigterm: Signal,
) -> anyhow::Result<()> {
    let Config {
        active_wallets_repeat_delay,
        active_wallets_ttl,
        transaction_batch_size,
        parallelism,
    } = config;

    // Shared counter for the maximum transaction ID observed in BlockIndexed events. This allows
    // the Wallet Indexer to not unnecessarily query the database when it is already up-to-date. The
    // initial value is set to the maximum in case initial events are missed during startup.
    let max_transaction_id = Arc::new(AtomicU64::new(u64::MAX));

    let block_indexed_task = task::spawn({
        let subscriber = subscriber.clone();
        let max_transaction_id = max_transaction_id.clone();

        async move {
            let block_indexed_stream = subscriber.subscribe::<BlockIndexed>();

            block_indexed_stream
                .try_for_each(|block_indexed| {
                    if let Some(id) = block_indexed.max_transaction_id {
                        max_transaction_id.store(id, Ordering::Release);
                    }
                    ok(())
                })
                .await
                .context("cannot get next BlockIndexed event")?;

            Ok::<(), anyhow::Error>(())
        }
    });

    let index_wallets_task = {
        task::spawn(async move {
            active_wallets(active_wallets_repeat_delay, active_wallets_ttl, &storage)
                .map(|result| result.context("get next active wallet"))
                .try_for_each_concurrent(Some(parallelism.get()), |wallet_id| {
                    let max_transaction_id = max_transaction_id.clone();
                    let mut publisher = publisher.clone();
                    let mut storage = storage.clone();

                    async move {
                        index_wallet(
                            wallet_id,
                            transaction_batch_size,
                            max_transaction_id,
                            &mut publisher,
                            &mut storage,
                        )
                        .await
                    }
                })
                .await
        })
    };

    select! {
        result = block_indexed_task => result
            .context("block_indexed_task")
            .and_then(|r| r.context("block_indexed_task failed")),

        result = index_wallets_task => result
            .context("index_wallets_task panicked")
            .and_then(|r| r.context("index_wallets_task failed")),

        _ = sigterm.recv() => {
            warn!("SIGTERM received");
            Ok(())
        }
    }
}

fn active_wallets(
    active_wallets_repeat_delay: Duration,
    active_wallets_ttl: Duration,
    storage: &impl Storage,
) -> impl Stream<Item = Result<Uuid, sqlx::Error>> + '_ {
    tokio_stream::StreamExt::throttle(stream::repeat(()), active_wallets_repeat_delay)
        .map(|_| Ok::<_, sqlx::Error>(()))
        .and_then(move |_| storage.active_wallets(active_wallets_ttl))
        .map_ok(|wallets| stream::iter(wallets).map(Ok))
        .try_flatten()
}

#[trace(properties = { "wallet_id": "{wallet_id}" })]
async fn index_wallet(
    wallet_id: Uuid,
    transaction_batch_size: NonZeroUsize,
    max_transaction_id: Arc<AtomicU64>,
    publisher: &mut impl Publisher,
    storage: &mut impl Storage,
) -> anyhow::Result<()> {
    let tx = storage
        .acquire_lock(wallet_id)
        .await
        .with_context(|| format!("acquire lock for wallet ID {wallet_id}"))?;

    let Some(mut tx) = tx else {
        return Ok(());
    };

    let wallet = storage
        .get_wallet_by_id(wallet_id, &mut tx)
        .await
        .with_context(|| format!("get wallet for wallet ID {wallet_id}"))?;

    // Only continue if possibly needed.
    if wallet.last_indexed_transaction_id < max_transaction_id.load(Ordering::Acquire) {
        let from = wallet.last_indexed_transaction_id + 1;
        let transactions = storage
            .get_transactions(from, transaction_batch_size, &mut tx)
            .await
            .context("get transactions")?;

        let last_indexed_transaction_id = if let Some(transaction) = transactions.last() {
            transaction.id
        } else {
            return Ok(());
        };

        let relevant_transactions = transactions
            .into_iter()
            .map(|transaction| {
                transaction
                    .relevant(&wallet)
                    .with_context(|| {
                        format!("check transaction relevance for wallet ID {wallet_id}")
                    })
                    .map(|relevant| (relevant, transaction))
            })
            .filter_map_ok(|(relevant, transaction)| relevant.then_some(transaction))
            .collect::<Result<Vec<_>, _>>()?;

        storage
            .save_relevant_transactions(
                &wallet.viewing_key,
                &relevant_transactions,
                last_indexed_transaction_id,
                &mut tx,
            )
            .await
            .with_context(|| format!("save relevant transactions for wallet ID {wallet_id}"))?;

        tx.commit().await.context("commit database transaction")?;

        if !relevant_transactions.is_empty() {
            let session_id = wallet.viewing_key.to_session_id();
            publisher
                .publish(&WalletIndexed { session_id })
                .await
                .with_context(|| {
                    format!("publish WalletIndexed event for wallet ID {wallet_id}")
                })?;
        }
    }

    Ok(())
}

fn parallelism_default() -> NonZeroUsize {
    std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

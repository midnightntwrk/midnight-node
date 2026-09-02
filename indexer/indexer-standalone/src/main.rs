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

#[cfg(feature = "standalone")]
mod config;

#[cfg(feature = "standalone")]
fn main() {
    use indexer_common::telemetry;
    use log::error;
    use std::panic;

    // Handle `--version` before anything else so it works without a config file.
    indexer_common::handle_version_flag!();

    // Initialize logging.
    telemetry::init_logging();

    // Replace the default panic hook with one that uses structured logging at ERROR level.
    panic::set_hook(Box::new(|panic| error!(panic:%; "process panicked")));

    // Run and log any error.
    if let Err(error) = run() {
        let backtrace = error.backtrace();
        let error = format!("{error:#}");
        error!(error, backtrace:%; "process exited with ERROR");
        std::process::exit(1);
    }
}

#[cfg(feature = "standalone")]
fn run() -> anyhow::Result<()> {
    use crate::config::{Config, InfraConfig};
    use anyhow::Context;
    use chain_indexer::{
        application as chain_app,
        infra::{storage as chain_storage, subxt_node::SubxtNode},
    };
    use indexer_api::{
        application as api_app,
        infra::{api::AxumApi, storage as api_storage},
    };
    use indexer_common::{
        cipher::make_cipher,
        config::ConfigExt,
        infra::{ledger_db, migrations, pool, pub_sub},
        telemetry,
    };
    use log::info;
    use spo_indexer::{
        application as spo_app,
        infra::{spo_client::SPOClient, storage as spo_storage},
    };
    use std::panic;
    use tokio::{
        runtime::Builder,
        select,
        signal::unix::{SignalKind, signal},
        task,
    };
    use wallet_indexer::{application as wallet_app, infra::storage as wallet_storage};

    // Load configuration.
    let Config {
        thread_stack_size,
        application_config,
        spo_config,
        infra_config,
        telemetry_config:
            telemetry::Config {
                tracing_config,
                metrics_config,
            },
    } = Config::load().context("load configuration")?;

    info!(
        application_config:?,
        infra_config:?;
        "starting"
    );

    // Retention/freshness invariant, enforceable only here where both configs share a process
    // (in cloud they live in separate binaries, so there it is an operator responsibility): the
    // dust generations subscription accepts snapshots up to `max_snapshot_age` blocks old, but
    // the chain-indexer only keeps the newest `ledger_state_retention` blocks' ledger states
    // loadable. If retention does not exceed the freshness window, an accepted snapshot can
    // resolve to garbage-collected state and panic inside storage-core on load.
    let ledger_state_retention = application_config.ledger_state_retention.get();
    let max_snapshot_age = infra_config
        .api_config
        .subscription_config
        .dust_generations
        .max_snapshot_age;
    assert!(
        u64::try_from(ledger_state_retention).unwrap_or(u64::MAX) > u64::from(max_snapshot_age),
        "ledger_state_retention ({ledger_state_retention}) must exceed \
         dust_generations.max_snapshot_age ({max_snapshot_age}): otherwise a snapshot can pass \
         the freshness check yet resolve to garbage-collected ledger state and panic on load"
    );

    let InfraConfig {
        run_migrations,
        storage_config,
        ledger_db_config,
        node_config,
        spo_node_config,
        api_config,
        secret,
    } = infra_config;

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(thread_stack_size as usize)
        .build()
        .context("build Tokio runtime")?;

    runtime.block_on(async {
        telemetry::init_tracing(tracing_config);
        telemetry::init_metrics(metrics_config);

        let pool = pool::sqlite::SqlitePool::new(storage_config)
            .await
            .context("create DB pool for Sqlite")?;
        if run_migrations {
            migrations::sqlite::run(&pool)
                .await
                .context("run Sqlite migrations")?;
        }

        let cipher = make_cipher(secret).context("make cipher")?;

        let pub_sub = pub_sub::in_mem::InMemPubSub::default();

        ledger_db::init(ledger_db_config)
            .await
            .context("initialize ledger db")?;

        // Move the node connection setup *inside* each spawned task so a slow
        // or unreachable URL only blocks its own component, not the whole
        // runtime startup. The previous shape `task::spawn({ ... .await? ... })`
        // ran the .await synchronously in the outer block_on, holding back the
        // indexer-api and wallet-indexer spawns for up to
        // `reconnect_max_attempts × reconnect_max_delay` (≈5 min by default).
        let chain_indexer = {
            let storage = chain_storage::Storage::new(pool.clone());
            let publisher = pub_sub.publisher();
            let application_config = application_config.clone();
            task::spawn(async move {
                let node = SubxtNode::new(node_config)
                    .await
                    .context("create SubxtNode")?;
                let sigterm =
                    signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");
                chain_app::run(application_config.into(), node, storage, publisher, sigterm).await
            })
        };

        let spo_indexer = {
            let storage = spo_storage::Storage::new(pool.clone());
            task::spawn(async move {
                let node = SPOClient::new(spo_node_config.into())
                    .await
                    .context("create SPOClient")?;
                let sigterm =
                    signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");
                spo_app::run(spo_config.into(), node, storage, sigterm).await
            })
        };

        let indexer_api = task::spawn({
            let subscriber = pub_sub.subscriber();
            let storage = api_storage::Storage::new(cipher.clone(), pool.clone());
            let api = AxumApi::new(api_config, storage, subscriber.clone());

            api_app::run(application_config.clone().into(), api, subscriber)
        });

        let wallet_indexer = task::spawn({
            let storage = wallet_storage::Storage::new(cipher, pool);
            let publisher = pub_sub.publisher();
            let subscriber = pub_sub.subscriber();
            let sigterm =
                signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

            wallet_app::run(
                application_config.into(),
                storage,
                publisher,
                subscriber,
                sigterm,
            )
        });

        select! {
            result = chain_indexer => handle_exit("chain-indexer", result),
            result = spo_indexer => handle_exit("spo-indexer", result),
            result = wallet_indexer => handle_exit("wallet-indexer", result),
            result = indexer_api => handle_exit("indexer-api", result),
        }

        info!("indexer shutting down");

        std::process::exit(1);
    })
}

#[cfg(feature = "standalone")]
fn handle_exit(task_name: &str, result: Result<anyhow::Result<()>, tokio::task::JoinError>) {
    use log::error;

    match result {
        Ok(Err(error)) => {
            let backtrace = error.backtrace();
            let error = format!("{error:#}");
            error!(error, backtrace:%; "{task_name} exited with ERROR");
        }

        Err(error) => {
            error!(error:% = format!("{error:#}"); "{task_name} panicked");
        }

        _ => {
            error!("{task_name} terminated");
        }
    }
}

#[cfg(not(feature = "standalone"))]
fn main() -> anyhow::Result<()> {
    unimplemented!()
}

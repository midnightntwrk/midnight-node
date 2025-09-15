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

#[cfg(feature = "standalone")]
mod config;

#[cfg(feature = "standalone")]
#[tokio::main]
async fn main() {
    use indexer_common::telemetry;
    use log::error;
    use std::panic;

    // Initialize logging.
    telemetry::init_logging();

    // Replace the default panic hook with one that uses structured logging at ERROR level.
    panic::set_hook(Box::new(|panic| error!(panic:%; "process panicked")));

    // Run and log any error.
    if let Err(error) = run().await {
        let backtrace = error.backtrace();
        let error = format!("{error:#}");
        error!(error, backtrace:%; "process exited with ERROR")
    }
}

#[cfg(feature = "standalone")]
async fn run() -> anyhow::Result<()> {
    use crate::config::{Config, InfraConfig};
    use anyhow::Context;
    use chain_indexer::infra::subxt_node::SubxtNode;
    use indexer_api::infra::api::AxumApi;
    use indexer_common::{
        cipher::make_cipher,
        config::ConfigExt,
        infra::{ledger_state_storage, migrations, pool, pub_sub},
        telemetry,
    };
    use log::info;
    use std::panic;
    use tokio::{
        select,
        signal::unix::{SignalKind, signal},
        task,
    };

    // Load configuration.
    let Config {
        run_migrations,
        application_config,
        infra_config,
        telemetry_config:
            telemetry::Config {
                tracing_config,
                metrics_config,
            },
    } = Config::load().context("load configuration")?;

    // Initialize tracing and metrics.
    telemetry::init_tracing(tracing_config);
    telemetry::init_metrics(metrics_config);

    info!(
        run_migrations,
        application_config:?,
        infra_config:?;
        "starting"
    );

    let InfraConfig {
        secret,
        node_config,
        storage_config,
        api_config,
    } = infra_config;

    let pool = pool::sqlite::SqlitePool::new(storage_config)
        .await
        .context("create DB pool for Sqlite")?;
    if run_migrations {
        migrations::sqlite::run(&pool)
            .await
            .context("run Sqlite migrations")?;
    }

    let cipher = make_cipher(secret).context("make cipher")?;

    let ledger_state_storage = ledger_state_storage::in_mem::InMemZswapStateStorage::default();

    let pub_sub = pub_sub::in_mem::InMemPubSub::default();

    let chain_indexer = task::spawn({
        let node = SubxtNode::new(node_config)
            .await
            .context("create SubxtNode")?;
        let storage = chain_indexer::infra::storage::Storage::new(pool.clone());
        let sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

        chain_indexer::application::run(
            application_config.into(),
            node,
            storage,
            ledger_state_storage.clone(),
            pub_sub.publisher(),
            sigterm,
        )
    });

    let indexer_api = task::spawn({
        let subscriber = pub_sub.subscriber();
        let storage = indexer_api::infra::storage::Storage::new(cipher.clone(), pool.clone());
        let api = AxumApi::new(
            api_config,
            storage,
            ledger_state_storage,
            subscriber.clone(),
        );
        let sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

        indexer_api::application::run(application_config.into(), api, subscriber, sigterm)
    });

    let wallet_indexer = task::spawn({
        let storage = wallet_indexer::infra::storage::Storage::new(cipher, pool);
        let publisher = pub_sub.publisher();
        let subscriber = pub_sub.subscriber();
        let sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

        wallet_indexer::application::run(
            application_config.into(),
            storage,
            publisher,
            subscriber,
            sigterm,
        )
    });

    select! {
        result = chain_indexer => handle_exit("chain-indexer", result),
        result = wallet_indexer => handle_exit("wallet-indexer", result),
        result = indexer_api => handle_exit("indexer-api", result),
    }

    info!("indexer shutting down");

    Ok(())
}

#[cfg(feature = "standalone")]
fn handle_exit(task_name: &str, result: Result<anyhow::Result<()>, tokio::task::JoinError>) {
    use log::error;

    match result {
        Ok(Err(error)) => {
            let backtrace = error.backtrace();
            let error = format!("{error:#}");
            error!(error, backtrace:%; "{task_name} exited with ERROR")
        }

        Err(error) => error!(error:% = format!("{error:#}"); "{task_name} panicked"),

        _ => error!("{task_name} terminated"),
    }
}

#[cfg(not(feature = "standalone"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    unimplemented!()
}

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

#[cfg(any(feature = "cloud", feature = "standalone"))]
#[tokio::main]
async fn main() {
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
    if let Err(error) = run().await {
        let backtrace = error.backtrace();
        let error = format!("{error:#}");
        error!(error, backtrace:%; "process exited with ERROR");
        std::process::exit(1);
    }
}

#[cfg(any(feature = "cloud", feature = "standalone"))]
async fn run() -> anyhow::Result<()> {
    use anyhow::Context;
    use indexer_common::{
        config::ConfigExt,
        infra::{migrations, pool},
        telemetry,
    };
    use log::info;
    use spo_indexer::{
        application,
        config::Config,
        infra::{self, spo_client::SPOClient},
    };
    use tokio::signal::unix::{SignalKind, signal};

    // Load configuration.
    let config = Config::load().context("load configuration")?;
    info!(config:?; "starting");
    let Config {
        run_migrations,
        application_config,
        infra_config,
        telemetry_config:
            telemetry::Config {
                tracing_config,
                metrics_config,
            },
    } = config.clone();

    // Initialize tracing and metrics.
    telemetry::init_tracing(tracing_config);
    telemetry::init_metrics(metrics_config);

    let sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

    let node = SPOClient::new(infra_config.node_config)
        .await
        .context("create SPOClient")?;

    #[cfg(feature = "cloud")]
    let storage = {
        let pool = pool::postgres::PostgresPool::new(infra_config.storage_config)
            .await
            .context("create DB pool for Postgres")?;
        if run_migrations {
            migrations::postgres::run(&pool)
                .await
                .context("run Postgres migrations")?;
        }
        infra::storage::Storage::new(pool)
    };

    #[cfg(feature = "standalone")]
    let storage = {
        let pool = pool::sqlite::SqlitePool::new(infra_config.storage_config)
            .await
            .context("create DB pool for Sqlite")?;
        if run_migrations {
            migrations::sqlite::run(&pool)
                .await
                .context("run Sqlite migrations")?;
        }
        infra::storage::Storage::new(pool)
    };

    application::run(application_config, node, storage, sigterm).await
}

#[cfg(not(any(feature = "cloud", feature = "standalone")))]
fn main() {
    unimplemented!()
}

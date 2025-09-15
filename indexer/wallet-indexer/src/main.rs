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

#[cfg(feature = "cloud")]
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

#[cfg(feature = "cloud")]
async fn run() -> anyhow::Result<()> {
    use anyhow::Context;
    use indexer_common::{
        cipher::make_cipher,
        config::ConfigExt,
        infra::{migrations, pool, pub_sub},
        telemetry,
    };
    use log::info;
    use tokio::signal::unix::{SignalKind, signal};
    use wallet_indexer::{application, config::Config, infra};

    // Register SIGTERM handler.
    let sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

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
    } = config;

    // Initialize tracing and metrics.
    telemetry::init_tracing(tracing_config);
    telemetry::init_metrics(metrics_config);

    let infra::Config {
        secret,
        storage_config,
        pub_sub_config,
    } = infra_config;

    let pool = pool::postgres::PostgresPool::new(storage_config)
        .await
        .context("create DB pool for Postgres")?;
    if run_migrations {
        migrations::postgres::run(&pool)
            .await
            .context("run Postgres migrations")?;
    }
    let cipher = make_cipher(secret).context("make cipher")?;
    let storage = infra::storage::Storage::new(cipher, pool);

    let publisher = pub_sub::nats::publisher::NatsPublisher::new(pub_sub_config.clone())
        .await
        .context("create NatsPublisher")?;
    let subscriber = pub_sub::nats::subscriber::NatsSubscriber::new(pub_sub_config)
        .await
        .context("create NatsSubscriber")?;

    // Run indexing.
    application::run(application_config, storage, publisher, subscriber, sigterm).await
}

#[cfg(not(feature = "cloud"))]
fn main() {
    unimplemented!()
}

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

#[cfg(feature = "cloud")]
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

#[cfg(feature = "cloud")]
fn run() -> anyhow::Result<()> {
    use anyhow::Context;
    use chain_indexer::{
        application,
        config::Config,
        infra::{self, subxt_node::SubxtNode},
    };
    use indexer_common::{
        config::ConfigExt,
        infra::{
            ledger_db::{self},
            migrations, pool, pub_sub,
        },
        telemetry,
    };
    use log::info;
    use std::time::Duration;
    use tokio::{
        runtime::Builder,
        signal::unix::{SignalKind, signal},
    };

    // Load configuration.
    let config = Config::load().context("load configuration")?;
    info!(config:?; "starting");
    let Config {
        thread_stack_size,
        application_config,
        infra_config,
        telemetry_config:
            telemetry::Config {
                tracing_config,
                metrics_config,
            },
    } = config;

    let infra::Config {
        run_migrations,
        storage_config,
        ledger_db_config,
        pub_sub_config,
        node_config,
    } = infra_config;

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(thread_stack_size as usize)
        .build()
        .context("build Tokio runtime")?;

    let result = runtime.block_on(async {
        let sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");

        telemetry::init_tracing(tracing_config);
        telemetry::init_metrics(metrics_config);

        let node = SubxtNode::new(node_config)
            .await
            .context("create SubxtNode")?;

        let pool = pool::postgres::PostgresPool::new(storage_config)
            .await
            .context("create DB pool for Postgres")?;
        if run_migrations {
            migrations::postgres::run(&pool)
                .await
                .context("run Postgres migrations")?;
        }

        let storage = infra::storage::Storage::new(pool.clone());

        ledger_db::init(ledger_db_config, pool);

        let publisher = pub_sub::nats::publisher::NatsPublisher::new(pub_sub_config)
            .await
            .context("create NatsPublisher")?;

        application::run(application_config, node, storage, publisher, sigterm).await
    });

    // The implicit runtime drop hangs indefinitely when spawned tasks are inside
    // block_in_place calls (e.g. ledger DB) that cannot be cancelled by abort().
    runtime.shutdown_timeout(Duration::from_secs(5));

    result
}

#[cfg(not(feature = "cloud"))]
fn main() {
    unimplemented!()
}

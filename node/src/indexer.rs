// This file is part of midnight-node.
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

mod config;

use anyhow::Context;
use chain_indexer::{
	application as chain_app,
	infra::{storage as chain_storage, subxt_node::SubxtNode},
};
use config::{Config, InfraConfig};
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
use spo_indexer::{
	application as spo_app,
	infra::{spo_client::SPOClient, storage as spo_storage},
};
use std::{thread, time::Duration};
use tokio::{
	runtime::Builder,
	select,
	signal::unix::{SignalKind, signal},
	task,
	time::sleep,
};
use wallet_indexer::{application as wallet_app, infra::storage as wallet_storage};

const RESTART_DELAY: Duration = Duration::from_secs(5);

pub async fn run_supervised() {
	run_supervised_with(run_on_dedicated_thread, RESTART_DELAY).await;
}

async fn run_supervised_with(mut run: impl FnMut() -> anyhow::Result<()>, restart_delay: Duration) {
	loop {
		match run() {
			Ok(()) => {
				log::warn!("embedded indexer exited normally; restarting in {restart_delay:?}")
			},
			Err(error) => {
				log::error!("embedded indexer exited: {error:#}; restarting in {restart_delay:?}")
			},
		}

		sleep(restart_delay).await;
	}
}

pub fn run_on_dedicated_thread() -> anyhow::Result<()> {
	run_in_thread(run)
}

fn run_in_thread<T>(run: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> anyhow::Result<T>
where
	T: Send + 'static,
{
	thread::Builder::new()
		.name("embedded-indexer".into())
		.spawn(run)
		.context("spawn embedded indexer thread")?
		.join()
		.map_err(|_| anyhow::anyhow!("embedded indexer thread panicked"))?
}

pub fn run() -> anyhow::Result<()> {
	let Config {
		thread_stack_size,
		application_config,
		spo_config,
		infra_config,
		telemetry_config: telemetry::Config { tracing_config, metrics_config },
	} = Config::load().context("load indexer configuration")?;

	log::info!(application_config:?, infra_config:?; "starting embedded indexer");

	let ledger_state_retention = application_config.ledger_state_retention.get();
	let max_snapshot_age =
		infra_config.api_config.subscription_config.dust_generations.max_snapshot_age;
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
		.context("build embedded indexer Tokio runtime")?;

	runtime.block_on(async {
		if tracing_config.enabled {
			log::warn!("indexer tracing is disabled because midnight-node owns global tracing");
		}
		telemetry::init_metrics(metrics_config);

		let pool = pool::sqlite::SqlitePool::new(storage_config)
			.await
			.context("create indexer SQLite pool")?;
		if run_migrations {
			migrations::sqlite::run(&pool).await.context("run indexer SQLite migrations")?;
		}

		let cipher = make_cipher(secret).context("make indexer cipher")?;
		let pub_sub = pub_sub::in_mem::InMemPubSub::default();

		ledger_db::init(ledger_db_config)
			.await
			.context("initialize indexer ledger DB")?;

		let chain_indexer = {
			let storage = chain_storage::Storage::new(pool.clone());
			let publisher = pub_sub.publisher();
			let application_config = application_config.clone();
			task::spawn(async move {
				let node =
					SubxtNode::new(node_config).await.context("create indexer node client")?;
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
					.context("create indexer SPO client")?;
				let mut sigterm =
					signal(SignalKind::terminate()).expect("SIGTERM handler can be registered");
				loop {
					match node.get_first_epoch_num().await {
						Ok(_) => break,
						Err(error) => {
							log::info!(error:?; "waiting for block 1 before starting SPO indexer");
						},
					}

					select! {
						_ = sleep(Duration::from_secs(1)) => {},
						_ = sigterm.recv() => return Ok(()),
					}
				}
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

			wallet_app::run(application_config.into(), storage, publisher, subscriber, sigterm)
		});

		let result = select! {
			result = chain_indexer => task_result("chain-indexer", result),
			result = spo_indexer => task_result("spo-indexer", result),
			result = wallet_indexer => task_result("wallet-indexer", result),
			result = indexer_api => task_result("indexer-api", result),
		};

		log::info!("embedded indexer shutting down");
		result
	})
}

fn task_result(
	task_name: &str,
	result: Result<anyhow::Result<()>, tokio::task::JoinError>,
) -> anyhow::Result<()> {
	use anyhow::anyhow;

	match result {
		Ok(Ok(())) => Err(anyhow!("{task_name} terminated")),
		Ok(Err(error)) => Err(error.context(format!("{task_name} exited"))),
		Err(error) => Err(anyhow!(error).context(format!("{task_name} panicked"))),
	}
}

#[cfg(test)]
mod tests {
	use super::{run_in_thread, run_supervised_with};
	use std::time::Duration;
	use tokio::{runtime::Builder, sync::mpsc, time::timeout};

	#[tokio::test(flavor = "multi_thread")]
	async fn dedicated_thread_can_own_a_tokio_runtime() {
		let result = run_in_thread(|| {
			Builder::new_current_thread()
				.enable_all()
				.build()?
				.block_on(async { Ok::<_, anyhow::Error>(()) })
		});

		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn supervisor_restarts_after_indexer_exit() {
		let (attempt_tx, mut attempt_rx) = mpsc::unbounded_channel();
		let supervisor = tokio::spawn(run_supervised_with(
			move || {
				attempt_tx.send(()).expect("attempt receiver should remain open");
				anyhow::bail!("temporary failure")
			},
			Duration::ZERO,
		));

		for _ in 0..3 {
			timeout(Duration::from_secs(1), attempt_rx.recv())
				.await
				.expect("indexer should be restarted promptly")
				.expect("attempt sender should remain open");
		}

		supervisor.abort();
	}
}

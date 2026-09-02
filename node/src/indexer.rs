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

use anyhow::Context;
use std::{future::Future, path::Path, time::Duration};
use tokio::{process::Command, time::sleep};

const INDEXER_BINARY: &str = "midnight-indexer";
const RESTART_DELAY: Duration = Duration::from_secs(5);

pub async fn run_supervised() {
	run_supervised_with(run_worker, RESTART_DELAY).await;
}

async fn run_supervised_with<F, Fut>(mut run: F, restart_delay: Duration)
where
	F: FnMut() -> Fut,
	Fut: Future<Output = anyhow::Result<()>>,
{
	loop {
		match run().await {
			Ok(()) => log::warn!("native indexer worker exited; restarting in {restart_delay:?}"),
			Err(error) => {
				log::error!(
					"native indexer worker exited: {error:#}; restarting in {restart_delay:?}"
				)
			},
		}

		sleep(restart_delay).await;
	}
}

async fn run_worker() -> anyhow::Result<()> {
	let current_exe = std::env::current_exe().context("resolve midnight-node executable")?;
	let worker = worker_path(&current_exe).context("resolve native indexer worker")?;
	let mut child = Command::new(&worker)
		.kill_on_drop(true)
		.spawn()
		.with_context(|| format!("spawn native indexer worker at {}", worker.display()))?;

	log::info!("native indexer worker started with pid {}", child.id().unwrap_or_default());
	let status = child.wait().await.context("wait for native indexer worker")?;
	anyhow::bail!("native indexer worker returned {status}")
}

fn worker_path(current_exe: &Path) -> anyhow::Result<std::path::PathBuf> {
	let directory = current_exe
		.parent()
		.context("midnight-node executable has no parent directory")?;
	Ok(directory.join(INDEXER_BINARY))
}

#[cfg(test)]
mod tests {
	use super::{run_supervised_with, worker_path};
	use std::{path::Path, time::Duration};
	use tokio::{sync::mpsc, time::timeout};

	#[test]
	fn worker_is_resolved_next_to_node() {
		assert_eq!(
			worker_path(Path::new("/usr/local/bin/midnight-node")).unwrap(),
			Path::new("/usr/local/bin/midnight-indexer")
		);
	}

	#[tokio::test]
	async fn supervisor_restarts_after_worker_exit() {
		let (attempt_tx, mut attempt_rx) = mpsc::unbounded_channel();
		let supervisor = tokio::spawn(run_supervised_with(
			move || {
				let attempt_tx = attempt_tx.clone();
				async move {
					attempt_tx.send(()).expect("attempt receiver should remain open");
					anyhow::bail!("temporary failure")
				}
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

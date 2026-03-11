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

use clap::{Args, Parser};
use midnight_node_toolkit::cli::{Cli, run_command};
use std::{
	error::Error,
	fmt,
	panic::{self, AssertUnwindSafe},
};

#[derive(Args)]
#[group(required = false, multiple = false)]
pub struct GenesisSource {
	/// RPC URL of node instance; Used to fetch existing transactions
	#[arg(long, short = 'u')]
	rpc_url: Option<String>,
	/// Filename of genesis tx. Used as initial state for generated txs.
	#[arg(long)]
	genesis_tx: Option<String>,
	/// Number of threads to use when fetching transactions from a live network
	#[arg(long, default_value = "20")]
	fetch_concurrency: usize,
}

#[derive(Debug)]
struct PanicError(String);

impl fmt::Display for PanicError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Panic occurred: {}", self.0)
	}
}

impl Error for PanicError {}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let result = panic::catch_unwind(AssertUnwindSafe(|| {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
			.block_on(async {
				let cli = Cli::parse();

				// Build the log filter. RUST_LOG overrides CLI flags.
				// CLI path uses with_regex(false) for prefix matching, where
				// longer prefixes properly override shorter ones (e.g.
				// midnight_node_toolkit::fetcher=info overrides midnight_node_toolkit=debug).
				let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
					.unwrap_or_else(|_| {
						let base =
							if cli.quiet { "warn" } else { "warn,midnight_node_toolkit=info" };
						let mut directives = base.to_string();
						if cli.verbose {
							directives += ",midnight_node_toolkit=debug";
							directives += ",midnight_node_toolkit::fetcher=info";
						}
						if cli.verbose_fetch {
							directives += ",midnight_node_toolkit::fetcher=debug";
						}
						if cli.verbose_ledger {
							directives += ",midnight_ledger=debug";
						}
						tracing_subscriber::EnvFilter::builder()
							.with_regex(false)
							.parse_lossy(directives)
					});

				// Initialize unified logging (captures both `log` and `tracing` events).
				if cli.log_json {
					tracing_subscriber::fmt()
						.json()
						.with_env_filter(env_filter)
						.with_writer(std::io::stderr)
						.init();
				} else {
					tracing_subscriber::fmt()
						.with_env_filter(env_filter)
						.with_writer(std::io::stderr)
						.with_target(cli.verbose)
						.with_file(cli.verbose)
						.with_line_number(cli.verbose)
						.init();
				}

				let res = run_command(cli.command).await;

				if let Err(ref e) = res {
					eprintln!("{e}");
				}

				return res;
			})
	}));

	// Pass through standard `Error`s or transform panics into `Error`
	result.unwrap_or_else(|panic_info| {
		let msg = match panic_info.downcast_ref::<&str>() {
			Some(s) => s.to_string(),
			None => match panic_info.downcast_ref::<String>() {
				Some(s) => s.clone(),
				None => "Unknown panic".to_string(),
			},
		};
		let err: Box<dyn std::error::Error + Send + Sync> = Box::new(PanicError(msg));
		Err(err)
	})
}

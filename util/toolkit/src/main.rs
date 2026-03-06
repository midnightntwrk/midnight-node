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

				// Initialize the logger.
				let writer: Box<dyn structured_logger::Writer> = if cli.log_json {
					structured_logger::json::new_writer(std::io::stderr())
				} else {
					midnight_node_toolkit::log_writer::new_writer(std::io::stderr(), cli.verbose)
				};
				structured_logger::Builder::new().with_default_writer(writer).init();

				let log_level = if cli.verbose {
					log::LevelFilter::Debug
				} else if cli.quiet {
					log::LevelFilter::Warn
				} else {
					log::LevelFilter::Info
				};
				log::set_max_level(log_level);

				// Initialize tracing (used by ledger to emit warnings)
				let subscriber =
					tracing_subscriber::fmt().with_max_level(tracing::Level::WARN).finish();
				tracing::subscriber::set_global_default(subscriber)?;

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

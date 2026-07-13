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

use async_trait::async_trait;
use std::{fs::File, io::Write, sync::Arc};

use crate::sender::{SendBatchError, Sender};
use midnight_node_ledger_helpers::fork::raw_block_data::{SerializedTx, SerializedTxBatches};

pub const DEFAULT_DEST_URL: &'static str = "ws://127.0.0.1:9944";

#[derive(clap::Args)]
pub struct Destination {
	/// RPC URL(s) of node instance(s) used to send generated transactions. Can set multiple.
	#[arg(long = "dest-url", short = 'd', conflicts_with = "dest_file", default_values_t = [DEFAULT_DEST_URL.to_string()], env = "MN_DEST_URL", global = true)]
	pub dest_urls: Vec<String>,
	/// The rate at which to send txs (per second)
	#[arg(long, short, default_value = "1", conflicts_with = "dest_file", global = true)]
	pub rate: f32,
	/// Output filename to write generated transaction.
	#[arg(long, conflicts_with = "dest_urls", global = true)]
	pub dest_file: Option<String>,
	/// Do not wait for finalization when sending transactions. May cause errors when sending batches.
	#[arg(long, conflicts_with = "dest_file", env = "MN_DONT_WATCH_PROGRESS", global = true)]
	pub no_watch_progress: bool,
}

pub struct SendTxsToFile {
	file: String,
}

impl SendTxsToFile {
	pub fn new(file: String) -> Self {
		Self { file }
	}

	fn save_multiple(
		&self,
		txs: &SerializedTxBatches,
		filename: &str,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let mut file = File::create(filename)?;
		file.write_all(&serde_json::to_vec(txs)?)?;
		Ok(())
	}

	fn save_single(
		&self,
		tx: &SerializedTx,
		filename: &str,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let mut file = File::create(filename)?;
		file.write_all(&serde_json::to_vec(tx)?)?;
		Ok(())
	}
}

pub struct SendTxsToUrl {
	urls: Vec<String>,
	rate: f32,
	no_watch_progress: bool,
}

impl SendTxsToUrl {
	pub fn new(urls: Vec<String>, rate: f32, no_watch_progress: bool) -> Self {
		Self { urls, rate, no_watch_progress }
	}
}

#[async_trait]
pub trait SendTxs: Send + Sync {
	async fn send_txs(
		&self,
		txs: &SerializedTxBatches,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl SendTxs for () {
	async fn send_txs(
		&self,
		_txs: &SerializedTxBatches,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		Ok(())
	}
}

#[async_trait]
impl SendTxs for SendTxsToFile {
	async fn send_txs(
		&self,
		txs: &SerializedTxBatches,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		// If txs.len() == 1, save SerializedTx
		if let [batch] = txs.batches.as_slice()
			&& let [tx] = batch.as_slice()
		{
			self.save_single(tx, &self.file)?;
		} else {
			self.save_multiple(txs, &self.file)?;
		}
		Ok(())
	}
}

#[async_trait]
impl SendTxs for SendTxsToUrl {
	async fn send_txs(
		&self,
		txs: &SerializedTxBatches,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		if self.rate <= 0.0 {
			return Err("rate must be greater than 0".into());
		}

		let sender = Arc::new(Sender::new(&self.urls, self.no_watch_progress).await?);

		let mut total_failed = 0;
		for (i, batch) in txs.batches.iter().enumerate() {
			log::info!("Sending batch {}...", i);
			let sender = sender.clone();
			let failed = sender.send_worker(self.rate, batch.clone()).await;
			total_failed += failed;
		}

		if total_failed > 0 {
			return Err(Box::new(SendBatchError { failed_count: total_failed }));
		}
		Ok(())
	}
}

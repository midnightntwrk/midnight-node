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

use thiserror::Error;

use crate::serde_def::SourceTransactions;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

pub mod builder;
pub mod destination;
pub mod source;

use builder::{Builder, DynamicError, ProverConfig, build_fork_aware_context_raw};
use destination::{Destination, SendTxs, SendTxsToFile, SendTxsToUrl};
use source::{GetTxs, GetTxsFromFile, GetTxsFromUrl, Source, SourceError};

#[derive(Debug, Error)]
pub enum TxGeneratorError {
	#[error("invalid source: {0}")]
	SourceError(#[from] SourceError),
	#[error("invalid destination: {0}")]
	DestinationError(#[from] DestinationError),
}

#[derive(Debug, Error)]
#[error("failed to create OnlineClient: {source}")]
pub struct DestinationError {
	#[from]
	source: subxt::Error,
}

pub struct TxGenerator {
	pub source: Box<dyn GetTxs>,
	pub destinations: Vec<Box<dyn SendTxs>>,
	pub builder_config: Builder,
	pub prover_config: ProverConfig,
	pub dry_run: bool,
}

impl TxGenerator {
	pub async fn new(
		src: Source,
		dest: Destination,
		builder: Builder,
		proof_server: Option<String>,
		dry_run: bool,
	) -> Result<Self, TxGeneratorError> {
		let source = Self::source(src, dry_run).await?;
		let destinations = Self::destinations(dest, dry_run).await?;
		if dry_run {
			log::info!("Dry-run: Builder type: {:?}", &builder);
		}
		let prover_config = Self::prover_config(proof_server, dry_run);

		Ok(Self { source, destinations, builder_config: builder, prover_config, dry_run })
	}

	pub async fn source(src: Source, dry_run: bool) -> Result<Box<dyn GetTxs>, SourceError> {
		if let Some(ref src_files) = src.src_files {
			if dry_run {
				log::info!("Dry-run: Source transactions from file(s): {:?}", &src_files);
				return Ok(Box::new(()));
			}
			let source: Box<dyn GetTxs> = Box::new(GetTxsFromFile::new(
				src_files.clone(),
				src.dust_warp,
				src.ignore_block_context,
			));
			Ok(source)
		} else if let Some(url) = src.src_url {
			if dry_run {
				log::info!("Dry-run: Source transactions from url: {:?}", &url);
				return Ok(Box::new(()));
			}
			let source: Box<dyn GetTxs> = Box::new(GetTxsFromUrl::new(
				&url,
				src.fetch_concurrency,
				src.fetch_compute_concurrency
					.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get())),
				src.dust_warp,
				src.fetch_only_cached,
				src.fetch_cache,
			));
			Ok(source)
		} else {
			Err(SourceError::InvalidSourceArgs(src))
		}
	}

	async fn destinations(
		dest: Destination,
		dry_run: bool,
	) -> Result<Vec<Box<dyn SendTxs>>, DestinationError> {
		if let Some(ref dest_file) = dest.dest_file {
			if dry_run {
				log::info!("Dry-run: Destination file: {:?}", &dest_file);
				return Ok(vec![Box::new(())]);
			}
			let destination: Box<dyn SendTxs> = Box::new(SendTxsToFile::new(dest_file.clone()));

			return Ok(vec![destination]);
		}

		// ------ accept multiple urls ------
		let mut dests = vec![];
		if dry_run {
			log::info!("Dry-run: Destination RPC(s): {:?}", &dest.dest_urls);
			log::info!("Dry-run: Destination rate: {:?} TPS", &dest.rate);
		}

		let destination: Box<dyn SendTxs> =
			Box::new(SendTxsToUrl::new(dest.dest_urls.clone(), dest.rate, dest.no_watch_progress));

		dests.push(destination);

		Ok(dests)
	}

	pub fn prover_config(proof_server: Option<String>, dry_run: bool) -> ProverConfig {
		if let Some(url) = proof_server {
			if dry_run {
				log::info!("Dry-run: remote prover: {url}");
			}
			ProverConfig::Remote(url)
		} else {
			if dry_run {
				log::info!("Dry-run: local prover (no proof server)");
			}
			ProverConfig::Local
		}
	}

	pub async fn get_txs(
		&self,
	) -> Result<SourceTransactions, Box<dyn std::error::Error + Send + Sync>> {
		self.source.get_txs().await
	}

	pub async fn send_txs(
		&self,
		txs: &SerializedTxBatches,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let sends_txs_futs: Vec<_> =
			self.destinations.iter().map(|dest| dest.send_txs(txs)).collect();

		let results = futures::future::join_all(sends_txs_futs).await;

		let mut any_failed = false;
		for result in results {
			if let Err(e) = result {
				eprintln!("ERROR: {e}");
				any_failed = true;
			}
		}

		if any_failed {
			return Err("one or more destination tasks failed".into());
		}
		Ok(())
	}

	pub async fn build_txs(
		&self,
		received_txs: &SourceTransactions,
	) -> Result<SerializedTxBatches, DynamicError> {
		let seeds = self.builder_config.relevant_wallet_seeds();
		let fork_ctx = if seeds.is_empty() {
			None
		} else {
			Some(build_fork_aware_context_raw(received_txs, &seeds))
		};

		let builder = self.builder_config.clone().to_versioned_builder(
			fork_ctx,
			&self.prover_config,
			self.dry_run,
		)?;

		builder.build_txs_from(received_txs.clone()).await
	}
}

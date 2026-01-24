// This file is part of midnight-node.
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

use midnight_node_ledger_helpers::*;
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use std::{
	marker::PhantomData,
	sync::{
		Arc,
		atomic::{self, AtomicUsize},
	},
	time::Duration,
};
use subxt::{
	OnlineClient,
	ext::{codec::Encode, subxt_core::config::Hash},
	tx::{TxInBlock, TxProgress},
};
use thiserror::Error;

use crate::{
	client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig},
	hash_to_str,
};

// Display from what url the sending error occurred
#[derive(Debug, Error)]
#[error("failed sending to {url}: {source}")]
pub struct SendToUrlError {
	url: String,
	#[source]
	source: subxt::Error,
}

#[derive(Debug, Clone)]
pub struct TxHashes {
	midnight_tx_hash: String,
	extrinsic_hash: String,
}

impl TxHashes {
	fn new<H: Hash + Encode>(midnight_tx_hash: &TransactionHash, extrinsic_hash: &H) -> Self {
		Self {
			midnight_tx_hash: Self::format_midnight_tx_hash(midnight_tx_hash),
			extrinsic_hash: Self::format_extrinsic_hash(extrinsic_hash),
		}
	}

	pub fn format_midnight_tx_hash(midnight_tx_hash: &TransactionHash) -> String {
		format!("0x{}", hex::encode(midnight_tx_hash.0.0))
	}

	pub fn format_extrinsic_hash<H: Hash + Encode>(extrinsic_hash: &H) -> String {
		format!("0x{}", hex::encode(extrinsic_hash.encode()))
	}
}

#[derive(Clone)]
pub struct ClientHandle {
	url: String,
	client: Arc<MidnightNodeClient>,
}

struct Progress {
	url: String,
	tx_progress: TxProgress<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>,
}

pub struct Sender<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static> {
	clients: Vec<ClientHandle>,
	counter: AtomicUsize,
	watch_progress: bool,
	_marker: PhantomData<(P, S)>,
}

impl<
	S: SignatureKind<DefaultDB> + Send + Sync + 'static,
	P: ProofKind<DefaultDB> + Send + Sync + 'static,
> Sender<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PureGeneratorPedersen, DefaultDB>: Tagged,
{
	pub async fn new(urls: &[String], no_watch_progress: bool) -> Result<Self, ClientError> {
		let clients: Result<Vec<ClientHandle>, ClientError> =
			futures::future::try_join_all(urls.iter().map(|url| async move {
				Ok(ClientHandle {
					url: url.clone(),
					client: Arc::new(MidnightNodeClient::new(url).await?),
				})
			}))
			.await;

		if no_watch_progress {
			log::warn!("toolkit send will not wait for finalization when sending txs");
		}

		Ok(Self {
			clients: clients?,
			counter: AtomicUsize::new(0),
			watch_progress: !no_watch_progress,
			_marker: Default::default(),
		})
	}

	pub fn get_client(&self) -> ClientHandle {
		let i = self.counter.fetch_add(1, atomic::Ordering::SeqCst);
		self.clients[i % self.clients.len()].clone()
	}

	pub async fn send_tx(
		&self,
		tx: &SerdeTransaction<S, P, DefaultDB>,
	) -> Result<(), SendToUrlError> {
		let (tx_hash_string, tx_progress) = self.send_tx_no_wait(tx).await?;
		if self.watch_progress {
			self.send_and_log(&tx_hash_string, tx_progress).await;
		}
		Ok(())
	}

	pub async fn send_worker(
		self: Arc<Self>,
		rate: f32,
		txs: Vec<TransactionWithContext<S, P, DefaultDB>>,
	) {
		log::debug!("send_worker: starting with {} txs", txs.len());
		let mut pending_finalized = vec![];
		for (i, tx) in txs.into_iter().enumerate() {
			let arc_self = self.clone();
			let task = tokio::spawn(async move {
				log::debug!("send_worker: spawned task for tx {} starting", i);
				let (tx_hashes, tx_progress) =
					arc_self.send_tx_no_wait(&tx.tx).await.expect("Failed to send tx");
				if arc_self.watch_progress {
					arc_self.send_and_log(&tx_hashes, tx_progress).await;
				}
				log::debug!("send_worker: spawned task for tx {} done", i);
			});
			pending_finalized.push(task);
			tokio::time::sleep(Duration::from_secs_f32(1f32 / rate)).await;
		}

		log::debug!("send_worker: waiting for {} tasks to complete", pending_finalized.len());
		for (i, task) in pending_finalized.into_iter().enumerate() {
			log::debug!("send_worker: waiting for task {}", i);
			task.await.expect("Transaction task failed");
			log::debug!("send_worker: task {} completed", i);
		}
		log::debug!("send_worker: all tasks completed");
	}

	async fn send_tx_no_wait(
		&self,
		tx: &SerdeTransaction<S, P, DefaultDB>,
	) -> Result<(TxHashes, Progress), SendToUrlError> {
		let client = self.get_client();
		log::debug!(url = client.url; "send_tx_no_wait: got client");

		let midnight_tx_hash = tx.transaction_hash();
		log::debug!(url = client.url; "send_tx_no_wait: computed hash");

		let tx_serialize = tx.serialize_inner().map_err(|e| Self::error(&client.url, e.into()))?;
		log::debug!(url = client.url; "send_tx_no_wait: serialized tx");

		let mn_tx = mn_meta::tx().midnight().send_mn_transaction(tx_serialize.clone());
		log::debug!(url = client.url; "send_tx_no_wait: created mn_tx");

		let unsigned_extrinsic = client
			.client
			.api
			.tx()
			.create_unsigned(&mn_tx)
			.map_err(|e| Self::error(&client.url, e.into()))?;
		log::debug!(url = client.url; "send_tx_no_wait: created unsigned extrinsic");

		log::info!(
			url = client.url,
			midnight_tx_hash = TxHashes::format_midnight_tx_hash(&midnight_tx_hash);
			"SENDING"
		);
		let tx_progress = unsigned_extrinsic
			.submit_and_watch()
			.await
			.map_err(|e| Self::error(&client.url, e.into()))?;

		let extrinsic_hash = tx_progress.extrinsic_hash();
		let tx_hashes = TxHashes::new(&midnight_tx_hash, &extrinsic_hash);

		log::info!(
			url = client.url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash;
			"SENT"
		);
		Ok((tx_hashes, Progress { url: client.url.clone(), tx_progress }))
	}

	async fn wait_for_best_block(
		mut progress: Progress,
	) -> (
		Progress,
		Option<TxInBlock<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>>,
	) {
		const BEST_BLOCK_TIMEOUT: Duration = Duration::from_secs(30);

		let wait_future = async {
			while let Some(prog) = progress.tx_progress.next().await {
				if let Ok(subxt::tx::TxStatus::InBestBlock(info)) = prog {
					return Some(info);
				}
			}
			None
		};

		match tokio::time::timeout(BEST_BLOCK_TIMEOUT, wait_future).await {
			Ok(result) => (progress, result),
			Err(_) => {
				log::warn!(
					url = progress.url;
					"Timeout waiting for best block after {} seconds",
					BEST_BLOCK_TIMEOUT.as_secs()
				);
				(progress, None)
			},
		}
	}

	async fn wait_for_finalized(
		mut progress: Progress,
	) -> Option<TxInBlock<MidnightNodeClientConfig, OnlineClient<MidnightNodeClientConfig>>> {
		const FINALIZED_TIMEOUT: Duration = Duration::from_secs(60);

		let url = progress.url.clone();
		let wait_future = async {
			while let Some(prog) = progress.tx_progress.next().await {
				if let Ok(subxt::tx::TxStatus::InFinalizedBlock(info)) = prog {
					return Some(info);
				}
			}
			None
		};

		match tokio::time::timeout(FINALIZED_TIMEOUT, wait_future).await {
			Ok(result) => result,
			Err(_) => {
				log::warn!(
					url = url;
					"Timeout waiting for finalization after {} seconds",
					FINALIZED_TIMEOUT.as_secs()
				);
				None
			},
		}
	}

	async fn send_and_log(&self, tx_hashes: &TxHashes, tx: Progress) {
		let url = tx.url.clone();
		let (progress, best_block) = Self::wait_for_best_block(tx).await;
		if best_block.is_none() {
			log::info!(
				url = &url,
				extrinsic_hash = &tx_hashes.extrinsic_hash,
				midnight_tx_hash = &tx_hashes.midnight_tx_hash;
				"FAILED_TO_REACH_BEST_BLOCK"
			);
			return;
		}
		let best_block = best_block.unwrap();
		log::info!(
			url = &url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"BEST_BLOCK"
		);

		let finalized = Self::wait_for_finalized(progress).await;
		let message = if finalized.is_some() { "FINALIZED" } else { "FAILED_TO_FINALIZE" };
		log::info!(
			url = &url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"{message}"
		);
	}

	fn error(url: &str, e: subxt::Error) -> SendToUrlError {
		SendToUrlError { url: url.to_string(), source: e }
	}
}

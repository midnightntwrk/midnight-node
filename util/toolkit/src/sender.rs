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

use backoff::ExponentialBackoff;
use midnight_node_ledger_helpers::{fork::raw_block_data::RawTransaction, *};
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use parity_scale_codec::Encode;
use std::{
	sync::{
		Arc,
		atomic::{self, AtomicUsize},
	},
	time::Duration,
};
use subxt::{
	client::OnlineClientAtBlockImpl,
	config::{Hash, HashFor},
	error::{BackendError, ExtrinsicError, RpcError},
	tx::{TransactionInBlock, TransactionProgress, TransactionStatus},
};
use thiserror::Error;

use crate::{
	client::{ClientError, MidnightNodeClient, MidnightNodeClientConfig},
	hash_to_str,
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTx;

#[derive(Debug, Error)]
#[error("{failed_count} transaction(s) failed during send")]
pub struct SendBatchError {
	pub failed_count: usize,
}

#[derive(Debug, Error)]
pub enum SenderError {
	#[error(
		"tx did not reach a best block within timeout (last seen status: {last_status}). \
		 The node accepted the extrinsic but it was never included — common causes: \
		 runtime rejected the tx during block-building (check the node's logs for \
		 `InvalidTransaction`/`UnknownTransaction`), fee/weight too high, or the tx pool \
		 evicted it. A synced node with finalized blocks does not imply the tx is valid."
	)]
	FailedToReachBestBlock { last_status: String },
	#[error("tx reached best block but was not finalized within timeout: {reason}")]
	FailedToFinalize { reason: String },
	#[error("runtime reported tx invalid: {message}")]
	InvalidTransaction { message: String },
	#[error("tx was dropped from the pool: {message}")]
	DroppedTransaction { message: String },
	#[error("tx subscription returned error status: {message}")]
	TransactionError { message: String },
	#[error("failed sending to {url}: {source}")]
	SendToUrlError {
		url: String,
		#[source]
		source: subxt::Error,
	},
}

impl SenderError {
	fn is_retryable(&self) -> bool {
		let SenderError::SendToUrlError { source, .. } = self else {
			return false;
		};

		// Reconnection in progress — always retryable.
		if source.is_disconnected_will_reconnect() {
			return true;
		}

		// Transport errors from transaction submission (e.g., HTTP 429 rate limiting).
		if let subxt::Error::ExtrinsicError(ExtrinsicError::ErrorSubmittingTransaction(
			BackendError::Rpc(RpcError::ClientError(subxt::rpcs::Error::Client(_))),
		)) = source
		{
			return true;
		}

		// Direct RPC client errors (e.g., from .tx().await).
		if let subxt::Error::OtherRpcClientError(subxt::rpcs::Error::Client(_)) = source {
			return true;
		}

		false
	}
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
	client: Arc<MidnightNodeClient>,
	tx_progress: TransactionProgress<
		MidnightNodeClientConfig,
		OnlineClientAtBlockImpl<MidnightNodeClientConfig>,
	>,
}

/// How finalization of a sent tx was confirmed.
enum Finalized {
	/// The tx subscription delivered `InFinalizedBlock`.
	Subscription,
	/// The subscription died without a finalization event, but a direct query
	/// confirmed the including block finalized. Carries the reason the
	/// subscription gave up.
	Fallback { watch_reason: String },
}

pub struct Sender {
	clients: Vec<ClientHandle>,
	counter: AtomicUsize,
	watch_progress: bool,
}

impl Sender {
	/// Connect a client per url. `rpc_request_timeout` is applied to every RPC
	/// request made by the created clients.
	pub async fn new(
		urls: &[String],
		no_watch_progress: bool,
		rpc_request_timeout: Duration,
	) -> Result<Self, ClientError> {
		let clients: Result<Vec<ClientHandle>, ClientError> =
			futures::future::try_join_all(urls.iter().map(|url| async move {
				Ok(ClientHandle {
					url: url.clone(),
					client: Arc::new(
						MidnightNodeClient::new(url, None, rpc_request_timeout).await?,
					),
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
		})
	}

	pub fn get_client(&self) -> ClientHandle {
		let i = self.counter.fetch_add(1, atomic::Ordering::SeqCst);
		self.clients[i % self.clients.len()].clone()
	}

	pub async fn send_tx(&self, tx: &SerializedTx) -> Result<(), SenderError> {
		let backoff = ExponentialBackoff {
			max_elapsed_time: Some(Duration::from_secs(60)),
			..ExponentialBackoff::default()
		};

		let (tx_hash_string, tx_progress) = backoff::future::retry(backoff, || async {
			self.send_tx_no_wait(tx).await.map_err(|e| {
				if e.is_retryable() {
					log::warn!("retryable error sending tx, will retry: {e}");
					backoff::Error::transient(e)
				} else {
					backoff::Error::permanent(e)
				}
			})
		})
		.await?;

		if self.watch_progress {
			self.send_and_log(&tx_hash_string, tx_progress).await?;
		}
		Ok(())
	}

	pub async fn send_worker(self: Arc<Self>, rate: f32, txs: Vec<SerializedTx>) -> usize {
		log::debug!("send_worker: starting with {} txs", txs.len());
		let failed_count = Arc::new(AtomicUsize::new(0));
		let mut pending_finalized = vec![];
		for (i, tx) in txs.into_iter().enumerate() {
			let arc_self = self.clone();
			let failed_count = failed_count.clone();
			let task = tokio::spawn(async move {
				log::debug!("send_worker: spawned task for tx {} starting", i);
				let result = arc_self.send_tx(&tx).await;
				if let Err(e) = result {
					log::error!("Failed to send tx {}: {}", i, e);
					failed_count.fetch_add(1, atomic::Ordering::SeqCst);
				}
				log::debug!("send_worker: spawned task for tx {} done", i);
			});
			pending_finalized.push(task);
			tokio::time::sleep(Duration::from_secs_f32(1f32 / rate)).await;
		}

		log::debug!("send_worker: waiting for {} tasks to complete", pending_finalized.len());
		for (i, task) in pending_finalized.into_iter().enumerate() {
			log::debug!("send_worker: waiting for task {}", i);
			if let Err(e) = task.await {
				log::error!("Transaction task {} failed: {}", i, e);
				failed_count.fetch_add(1, atomic::Ordering::SeqCst);
			}
			log::debug!("send_worker: task {} completed", i);
		}
		log::debug!("send_worker: all tasks completed");
		failed_count.load(atomic::Ordering::SeqCst)
	}

	async fn send_tx_no_wait(
		&self,
		tx: &SerializedTx,
	) -> Result<(TxHashes, Progress), SenderError> {
		let client = self.get_client();
		tracing::debug!(url = client.url, "send_tx_no_wait: got client");

		let midnight_tx_hash = TransactionHash(HashOutput(tx.tx_hash));
		tracing::debug!(url = client.url, "send_tx_no_wait: computed hash");

		let unsigned_extrinsic = match &tx.tx {
			RawTransaction::Midnight(tx) => {
				let mn_tx = mn_meta::tx().midnight().send_mn_transaction(tx.clone());
				tracing::debug!(url = client.url, "send_tx_no_wait: created mn_tx");
				client
					.client
					.api
					.tx()
					.await
					.map_err(|e| SenderError::SendToUrlError {
						url: client.url.clone(),
						source: e.into(),
					})?
					.create_unsigned(&mn_tx)
					.expect("failed to create unsigned extrinsic")
			},
			RawTransaction::System(tx) => {
				let mn_tx = mn_meta::tx().midnight_system().send_mn_system_transaction(tx.clone());
				tracing::debug!(url = client.url, "send_tx_no_wait: created mn_system_tx");
				client
					.client
					.api
					.tx()
					.await
					.map_err(|e| SenderError::SendToUrlError {
						url: client.url.clone(),
						source: e.into(),
					})?
					.create_unsigned(&mn_tx)
					.expect("failed to create unsigned extrinsic")
			},
		};

		tracing::debug!(url = client.url, "send_tx_no_wait: created unsigned extrinsic");

		tracing::info!(
			url = client.url,
			midnight_tx_hash = TxHashes::format_midnight_tx_hash(&midnight_tx_hash),
			"SENDING"
		);
		let tx_progress = unsigned_extrinsic.submit_and_watch().await.map_err(|e| {
			SenderError::SendToUrlError { url: client.url.clone(), source: e.into() }
		})?;

		let extrinsic_hash = tx_progress.extrinsic_hash();
		let tx_hashes = TxHashes::new(&midnight_tx_hash, &extrinsic_hash);

		log::info!(
			url = client.url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash;
			"SENT"
		);
		Ok((
			tx_hashes,
			Progress { url: client.url.clone(), client: client.client.clone(), tx_progress },
		))
	}

	/// Waits until the tx lands in a block. The `bool` in the success value is
	/// true when the subscription skipped straight to `InFinalizedBlock`
	/// (event coalescing under load) — the caller can skip the finality wait.
	async fn wait_for_best_block(
		mut progress: Progress,
	) -> (
		Progress,
		Result<
			(
				TransactionInBlock<
					MidnightNodeClientConfig,
					OnlineClientAtBlockImpl<MidnightNodeClientConfig>,
				>,
				bool,
			),
			SenderError,
		>,
	) {
		const BEST_BLOCK_TIMEOUT: Duration = Duration::from_secs(30);

		let mut last_status: &'static str = "<none>";
		let wait_future = async {
			while let Some(prog) = progress.tx_progress.next().await {
				match prog {
					Ok(TransactionStatus::InBestBlock(info)) => return Ok((info, false)),
					Ok(TransactionStatus::InFinalizedBlock(info)) => return Ok((info, true)),
					Ok(TransactionStatus::Invalid { message }) => {
						return Err(SenderError::InvalidTransaction { message });
					},
					Ok(TransactionStatus::Dropped { message }) => {
						return Err(SenderError::DroppedTransaction { message });
					},
					Ok(TransactionStatus::Error { message }) => {
						return Err(SenderError::TransactionError { message });
					},
					Ok(status) => {
						last_status = match status {
							TransactionStatus::Validated => "Validated",
							TransactionStatus::Broadcasted => "Broadcasted",
							TransactionStatus::NoLongerInBestBlock => "NoLongerInBestBlock",
							_ => "Unknown",
						};
					},
					Err(e) => {
						return Err(SenderError::TransactionError { message: e.to_string() });
					},
				}
			}
			Err(SenderError::FailedToReachBestBlock {
				last_status: format!("{last_status} (stream ended)"),
			})
		};

		match tokio::time::timeout(BEST_BLOCK_TIMEOUT, wait_future).await {
			Ok(result) => (progress, result),
			Err(_) => {
				log::warn!(
					url = progress.url;
					"Timeout waiting for best block after {} seconds",
					BEST_BLOCK_TIMEOUT.as_secs()
				);
				let err = SenderError::FailedToReachBestBlock {
					last_status: format!(
						"{last_status} (no terminal status after {}s)",
						BEST_BLOCK_TIMEOUT.as_secs()
					),
				};
				(progress, Err(err))
			},
		}
	}

	async fn wait_for_finalized(
		mut progress: Progress,
		best_block_hash: HashFor<MidnightNodeClientConfig>,
	) -> Result<Finalized, String> {
		const FINALIZED_TIMEOUT: Duration = Duration::from_secs(60);
		const FINALITY_POLL_INTERVAL: Duration = Duration::from_secs(2);
		const MIN_FALLBACK_WINDOW: Duration = Duration::from_secs(10);

		let url = progress.url.clone();
		let deadline = tokio::time::Instant::now() + FINALIZED_TIMEOUT;

		let watch_future = async {
			while let Some(prog) = progress.tx_progress.next().await {
				let reason = match prog {
					Ok(TransactionStatus::InFinalizedBlock(_)) => return Ok(()),
					Ok(TransactionStatus::Invalid { message }) => {
						format!("pool reported Invalid: {message}")
					},
					Ok(TransactionStatus::Dropped { message }) => {
						format!("pool reported Dropped: {message}")
					},
					Ok(TransactionStatus::Error { message }) => {
						format!("pool reported Error: {message}")
					},
					Ok(_) => continue,
					Err(e) => format!("subscription error: {e}"),
				};
				log::warn!(
					url = url;
					"terminal event on tx subscription after best block: {reason}"
				);
				return Err(reason);
			}
			let reason = "subscription ended without a finalization event".to_string();
			log::warn!(url = url; "{reason}");
			Err(reason)
		};

		let watch_reason = match tokio::time::timeout_at(deadline, watch_future).await {
			Ok(Ok(())) => return Ok(Finalized::Subscription),
			Ok(Err(reason)) => reason,
			Err(_) => format!("no finalization event after {}s", FINALIZED_TIMEOUT.as_secs()),
		};

		// The subscription is not authoritative for a tx that already reached a best
		// block: the pool can drop its watcher (terminal event, stream end) even
		// though that block finalizes normally. Ask the node directly about the
		// including block before declaring failure.
		log::debug!(
			url = url;
			"tx subscription gave no finalization event ({watch_reason}), \
			 checking finality of the including block directly"
		);
		// Guarantee a minimum direct-check window even when the watcher died late
		// in the finalization budget — the check is cheap, and a watcher death at
		// t=59s must not reintroduce the false failure right when the node is
		// under load.
		let fallback_deadline = deadline.max(tokio::time::Instant::now() + MIN_FALLBACK_WINDOW);
		loop {
			match progress.client.is_block_finalized(best_block_hash).await {
				Ok(true) => return Ok(Finalized::Fallback { watch_reason }),
				Ok(false) => {},
				// Transient RPC failures must not fail the tx: log and let the
				// next tick retry until the deadline.
				Err(e) => {
					log::warn!(url = url; "failed to check block finality: {e}");
				},
			}
			if tokio::time::Instant::now() >= fallback_deadline {
				break;
			}
			tokio::time::sleep(FINALITY_POLL_INTERVAL).await;
		}
		log::warn!(
			url = url;
			"including block not finalized within {}s ({watch_reason})",
			FINALIZED_TIMEOUT.as_secs()
		);
		Err(watch_reason)
	}

	async fn send_and_log(&self, tx_hashes: &TxHashes, tx: Progress) -> Result<(), SenderError> {
		let url = tx.url.clone();
		let (progress, best_block_result) = Self::wait_for_best_block(tx).await;
		let (best_block, already_finalized) = match best_block_result {
			Ok(info) => info,
			Err(err) => {
				let tag = match &err {
					SenderError::InvalidTransaction { .. } => "INVALID_TRANSACTION",
					SenderError::DroppedTransaction { .. } => "DROPPED_TRANSACTION",
					SenderError::TransactionError { .. } => "TRANSACTION_ERROR",
					_ => "FAILED_TO_REACH_BEST_BLOCK",
				};
				log::info!(
					url = &url,
					extrinsic_hash = &tx_hashes.extrinsic_hash,
					midnight_tx_hash = &tx_hashes.midnight_tx_hash,
					reason = err.to_string().as_str();
					"{tag}"
				);
				return Err(err);
			},
		};
		log::info!(
			url = &url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"BEST_BLOCK"
		);

		if already_finalized {
			log::info!(
				url = &url,
				extrinsic_hash = &tx_hashes.extrinsic_hash,
				midnight_tx_hash = &tx_hashes.midnight_tx_hash,
				block_hash = hash_to_str(best_block.block_hash()).as_str();
				"FINALIZED"
			);
			return Ok(());
		}

		match Self::wait_for_finalized(progress, best_block.block_hash()).await {
			Ok(Finalized::Subscription) => {
				log::info!(
					url = &url,
					extrinsic_hash = &tx_hashes.extrinsic_hash,
					midnight_tx_hash = &tx_hashes.midnight_tx_hash,
					block_hash = hash_to_str(best_block.block_hash()).as_str();
					"FINALIZED"
				);
				Ok(())
			},
			Ok(Finalized::Fallback { watch_reason }) => {
				log::info!(
					url = &url,
					extrinsic_hash = &tx_hashes.extrinsic_hash,
					midnight_tx_hash = &tx_hashes.midnight_tx_hash,
					block_hash = hash_to_str(best_block.block_hash()).as_str(),
					reason = watch_reason.as_str();
					"FINALIZED_VIA_FALLBACK"
				);
				Ok(())
			},
			Err(reason) => {
				log::info!(
					url = &url,
					extrinsic_hash = &tx_hashes.extrinsic_hash,
					midnight_tx_hash = &tx_hashes.midnight_tx_hash,
					block_hash = hash_to_str(best_block.block_hash()).as_str(),
					reason = reason.as_str();
					"FAILED_TO_FINALIZE"
				);
				Err(SenderError::FailedToFinalize { reason })
			},
		}
	}
}

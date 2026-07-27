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
	config::Hash,
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

/// Minimum number of finalized blocks to scan backwards when the watch stream
/// timed out without reporting finality. The effective depth scales with the
/// configured finalization timeout (see [`Sender::finalized_fallback_scan_depth`]):
/// a longer wait lets the finalized head advance further past the block that
/// carried the tx, so a fixed window would miss it.
const FINALIZED_FALLBACK_MIN_SCAN_DEPTH: u32 = 64;

/// Expected block production interval, used to convert the finalization
/// timeout into a scan depth.
const EXPECTED_BLOCK_SECONDS: u64 = 6;

/// Extra blocks scanned beyond the timeout-derived depth, covering finality
/// lag and the retraction/re-inclusion window.
const FINALIZED_FALLBACK_SCAN_MARGIN: u32 = 16;

/// The finalized-chain fallback scan could not run to completion, so the
/// tx's finality is unknown rather than refuted.
#[derive(Debug, Error)]
pub enum FinalityScanError {
	#[error("failed to fetch the finalized head: {0}")]
	FinalizedHead(subxt::rpcs::Error),
	#[error("failed to fetch finalized block {hash}: {source}")]
	FetchBlock {
		hash: String,
		#[source]
		source: subxt::rpcs::Error,
	},
	#[error("finalized block {hash} not returned by the node")]
	MissingBlock { hash: String },
	#[error("extrinsic hash {hash} is not valid hex")]
	InvalidTargetHash { hash: String },
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
	#[error("tx reached best block but was not finalized within timeout")]
	FailedToFinalize,
	#[error(
		"tx reached best block, the finalization watch timed out, and the \
		 finalized-chain check could not complete: {source}. Finality is UNKNOWN — \
		 the tx may have landed; do not treat this exit as proof of failure for \
		 at-most-once operations."
	)]
	FinalityUnverified {
		#[source]
		source: FinalityScanError,
	},
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
	tx_progress: TransactionProgress<
		MidnightNodeClientConfig,
		OnlineClientAtBlockImpl<MidnightNodeClientConfig>,
	>,
}

pub struct Sender {
	clients: Vec<ClientHandle>,
	counter: AtomicUsize,
	watch_progress: bool,
}

impl Sender {
	pub async fn new(urls: &[String], no_watch_progress: bool) -> Result<Self, ClientError> {
		let clients: Result<Vec<ClientHandle>, ClientError> =
			futures::future::try_join_all(urls.iter().map(|url| async move {
				Ok(ClientHandle {
					url: url.clone(),
					client: Arc::new(MidnightNodeClient::new(url, None).await?),
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
		Ok((tx_hashes, Progress { url: client.url.clone(), tx_progress }))
	}

	/// Read a duration override (in seconds) from the environment, falling back
	/// to `default`. Lets slow or fault-injected environments stretch the send
	/// watch phases without a CLI change (see #1853/#1854).
	fn duration_from_env(var: &str, default: Duration) -> Duration {
		std::env::var(var)
			.ok()
			.and_then(|v| v.parse::<u64>().ok())
			.map(Duration::from_secs)
			.unwrap_or(default)
	}

	async fn wait_for_best_block(
		mut progress: Progress,
	) -> (
		Progress,
		Result<
			TransactionInBlock<
				MidnightNodeClientConfig,
				OnlineClientAtBlockImpl<MidnightNodeClientConfig>,
			>,
			SenderError,
		>,
	) {
		let best_block_timeout =
			Self::duration_from_env("MN_SEND_BEST_BLOCK_TIMEOUT", Duration::from_secs(30));

		let mut last_status: &'static str = "<none>";
		let wait_future = async {
			while let Some(prog) = progress.tx_progress.next().await {
				match prog {
					Ok(TransactionStatus::InBestBlock(info)) => return Ok(info),
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
							TransactionStatus::InFinalizedBlock(_) => "InFinalizedBlock",
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

		match tokio::time::timeout(best_block_timeout, wait_future).await {
			Ok(result) => (progress, result),
			Err(_) => {
				log::warn!(
					url = progress.url;
					"Timeout waiting for best block after {} seconds",
					best_block_timeout.as_secs()
				);
				let err = SenderError::FailedToReachBestBlock {
					last_status: format!(
						"{last_status} (no terminal status after {}s)",
						best_block_timeout.as_secs()
					),
				};
				(progress, Err(err))
			},
		}
	}

	async fn wait_for_finalized(
		mut progress: Progress,
	) -> Option<
		TransactionInBlock<
			MidnightNodeClientConfig,
			OnlineClientAtBlockImpl<MidnightNodeClientConfig>,
		>,
	> {
		let finalized_timeout =
			Self::duration_from_env("MN_SEND_FINALIZED_TIMEOUT", Duration::from_secs(60));

		let url = progress.url.clone();
		let wait_future = async {
			while let Some(prog) = progress.tx_progress.next().await {
				match prog {
					Ok(TransactionStatus::InFinalizedBlock(info)) => return Some(info),
					// A fork retraction: the block that carried the tx fell off
					// the best chain. The tx normally returns to the pool for
					// re-inclusion, so keep watching — and make the retraction
					// visible, since it is the precursor of the lost-tx failure
					// mode (#1854).
					Ok(TransactionStatus::NoLongerInBestBlock) => {
						log::warn!(
							url = progress.url;
							"tx retracted from best block; watching for re-inclusion"
						);
					},
					_ => {},
				}
			}
			None
		};

		match tokio::time::timeout(finalized_timeout, wait_future).await {
			Ok(result) => result,
			Err(_) => {
				log::warn!(
					url = url;
					"Timeout waiting for finalization after {} seconds",
					finalized_timeout.as_secs()
				);
				None
			},
		}
	}

	/// Scan depth for the finalized-chain fallback, scaled to the configured
	/// finalization timeout: while the watcher waited out the timeout on a dead
	/// branch, the finalized head advanced ~timeout/block_time blocks past the
	/// block that re-included the tx.
	fn finalized_fallback_scan_depth(finalized_timeout: Duration) -> u32 {
		let timeout_blocks =
			u32::try_from(finalized_timeout.as_secs() / EXPECTED_BLOCK_SECONDS).unwrap_or(u32::MAX);
		FINALIZED_FALLBACK_MIN_SCAN_DEPTH
			.max(timeout_blocks.saturating_add(FINALIZED_FALLBACK_SCAN_MARGIN))
	}

	/// Check the finalized chain directly for an extrinsic, newest block first,
	/// up to `max_depth` blocks below the finalized head.
	///
	/// The watch stream can lose track of a tx across a fork retraction: the
	/// tx is re-included and finalized in a different block, but the stream
	/// never reports it and the watcher times out (#1854). The chain itself is
	/// the source of truth, so consult it before declaring a landed tx failed.
	///
	/// `Ok(Some(hash))` means the extrinsic is finalized in `hash`; `Ok(None)`
	/// means the scan completed and the extrinsic is definitively absent from
	/// the window; `Err` means the scan could not complete (RPC failure), so
	/// finality is unknown — callers must not read it as absence.
	pub async fn find_in_finalized_chain(
		client: &MidnightNodeClient,
		extrinsic_hash_hex: &str,
		max_depth: u32,
	) -> Result<Option<String>, FinalityScanError> {
		let target = hex::decode(extrinsic_hash_hex.trim_start_matches("0x")).map_err(|_| {
			FinalityScanError::InvalidTargetHash { hash: extrinsic_hash_hex.to_string() }
		})?;

		let mut hash = client
			.rpc
			.chain_get_finalized_head()
			.await
			.map_err(FinalityScanError::FinalizedHead)?;

		for _ in 0..max_depth {
			let block = match client.rpc.chain_get_block(Some(hash)).await {
				Ok(Some(b)) => b,
				Ok(None) => {
					return Err(FinalityScanError::MissingBlock { hash: hash_to_str(hash) });
				},
				Err(e) => {
					return Err(FinalityScanError::FetchBlock {
						hash: hash_to_str(hash),
						source: e,
					});
				},
			};

			for ext in &block.block.extrinsics {
				if sp_crypto_hashing::blake2_256(&ext.0)[..] == target[..] {
					return Ok(Some(hash_to_str(hash)));
				}
			}

			if block.block.header.number == 0 {
				return Ok(None);
			}
			hash = block.block.header.parent_hash;
		}
		Ok(None)
	}

	async fn send_and_log(&self, tx_hashes: &TxHashes, tx: Progress) -> Result<(), SenderError> {
		let url = tx.url.clone();
		let (progress, best_block_result) = Self::wait_for_best_block(tx).await;
		let best_block = match best_block_result {
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

		let finalized = Self::wait_for_finalized(progress).await;
		let (finalized_block_hash, scan_error) = match &finalized {
			Some(info) => (Some(hash_to_str(info.block_hash())), None),
			// The watch stream said nothing — but the tx may have been
			// re-included after a retraction and finalized in a block the
			// stream never reported (#1854). Ask the chain before failing.
			// A scan that cannot complete is tracked separately from a scan
			// that completes without a match: only the latter refutes finality.
			None => {
				let client = self.clients.iter().find(|c| c.url == url);
				match client {
					Some(handle) => {
						let finalized_timeout = Self::duration_from_env(
							"MN_SEND_FINALIZED_TIMEOUT",
							Duration::from_secs(60),
						);
						match Self::find_in_finalized_chain(
							&handle.client,
							&tx_hashes.extrinsic_hash,
							Self::finalized_fallback_scan_depth(finalized_timeout),
						)
						.await
						{
							Ok(found) => (found, None),
							Err(e) => {
								log::warn!(
									url = &url;
									"finalized-chain check inconclusive: {e}"
								);
								(None, Some(e))
							},
						}
					},
					None => (None, None),
				}
			},
		};

		let message = if finalized_block_hash.is_some() {
			if finalized.is_some() { "FINALIZED" } else { "FINALIZED_AFTER_RETRACTION" }
		} else if scan_error.is_some() {
			// Neither confirmed nor refuted: the watch timed out AND the
			// chain could not be consulted. Distinct from FAILED_TO_FINALIZE
			// so at-most-once callers don't read it as proof of absence.
			"FINALITY_UNVERIFIED"
		} else {
			"FAILED_TO_FINALIZE"
		};
		let display_block_hash = finalized_block_hash
			.clone()
			.unwrap_or_else(|| hash_to_str(best_block.block_hash()));
		log::info!(
			url = &url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash,
			block_hash = display_block_hash.as_str();
			"{message}"
		);
		match (finalized_block_hash.is_some(), scan_error) {
			(true, _) => Ok(()),
			(false, Some(source)) => Err(SenderError::FinalityUnverified { source }),
			(false, None) => Err(SenderError::FailedToFinalize),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fallback_scan_depth_keeps_minimum_for_default_timeout() {
		// 60s / 6s = 10 blocks + 16 margin = 26, below the 64 floor.
		assert_eq!(Sender::finalized_fallback_scan_depth(Duration::from_secs(60)), 64);
	}

	#[test]
	fn fallback_scan_depth_scales_with_long_timeouts() {
		// 600s / 6s = 100 blocks + 16 margin — a fixed 64 would miss the tx.
		assert_eq!(Sender::finalized_fallback_scan_depth(Duration::from_secs(600)), 116);
	}

	#[test]
	fn fallback_scan_depth_saturates_on_absurd_timeouts() {
		assert_eq!(Sender::finalized_fallback_scan_depth(Duration::from_secs(u64::MAX)), u32::MAX);
	}
}

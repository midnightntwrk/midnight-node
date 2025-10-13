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
use std::{marker::PhantomData, sync::Arc};
use subxt::{
	OnlineClient, PolkadotConfig,
	ext::{codec::Encode, subxt_core::config::Hash},
	tx::{TxInBlock, TxProgress},
};
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::hash_to_str;

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

pub struct Sender<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static> {
	api: OnlineClient<PolkadotConfig>,
	url: String,
	_marker_p: PhantomData<P>,
	_marker_s: PhantomData<S>,
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
	pub fn new(api: OnlineClient<PolkadotConfig>, url: String) -> Self {
		Self { api, url, _marker_p: PhantomData, _marker_s: PhantomData }
	}

	pub async fn send_tx(
		&self,
		tx: &SerdeTransaction<S, P, DefaultDB>,
	) -> Result<(), SendToUrlError> {
		let (tx_hash_string, tx_progress) = self.send_tx_no_wait(tx).await?;
		self.send_and_log(&tx_hash_string, tx_progress).await;
		Ok(())
	}

	pub async fn send_worker(
		self: Arc<Self>,
		semaphore: Arc<Semaphore>,
		txs: Vec<TransactionWithContext<S, P, DefaultDB>>,
	) {
		let mut permits = vec![];
		let mut pending_finalized = vec![];
		for tx in txs {
			let permit = semaphore.acquire().await.unwrap();
			permits.push(permit);
			let self_clone = self.clone();
			let task = tokio::spawn(async move {
				let (tx_hashes, tx_progress) =
					self_clone.send_tx_no_wait(&tx.tx).await.expect("Failed to send tx");
				self_clone.send_and_log(&tx_hashes, tx_progress).await;
			});
			pending_finalized.push(task);
		}

		for task in pending_finalized {
			task.await.expect("Transaction task failed");
		}
	}

	async fn send_tx_no_wait(
		&self,
		tx: &SerdeTransaction<S, P, DefaultDB>,
	) -> Result<(TxHashes, TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>), SendToUrlError>
	{
		let midnight_tx_hash = tx.transaction_hash();
		let tx_serialize = tx.serialize_inner().map_err(|e| self.error(e.into()))?;
		let mn_tx = mn_meta::tx().midnight().send_mn_transaction(tx_serialize.clone());

		let unsigned_extrinsic =
			self.api.tx().create_unsigned(&mn_tx).map_err(|e| self.error(e.into()))?;

		log::info!(
			url = self.url,
			midnight_tx_hash = TxHashes::format_midnight_tx_hash(&midnight_tx_hash);
			"SENDING"
		);
		let tx_progress =
			unsigned_extrinsic.submit_and_watch().await.map_err(|e| self.error(e.into()))?;

		let extrinsic_hash = tx_progress.extrinsic_hash();
		let tx_hashes = TxHashes::new(&midnight_tx_hash, &extrinsic_hash);

		log::info!(
			url = self.url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash;
			"SENT"
		);
		Ok((tx_hashes, tx_progress))
	}

	async fn wait_for_best_block(
		mut progress: TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	) -> (
		TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
		Option<TxInBlock<PolkadotConfig, OnlineClient<PolkadotConfig>>>,
	) {
		while let Some(prog) = progress.next().await {
			if let Ok(subxt::tx::TxStatus::InBestBlock(info)) = prog {
				return (progress, Some(info));
			}
		}

		(progress, None)
	}

	async fn wait_for_finalized(
		mut progress: TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	) -> Option<TxInBlock<PolkadotConfig, OnlineClient<PolkadotConfig>>> {
		while let Some(prog) = progress.next().await {
			if let Ok(subxt::tx::TxStatus::InFinalizedBlock(info)) = prog {
				return Some(info);
			}
		}

		None
	}

	async fn send_and_log(
		&self,
		tx_hashes: &TxHashes,
		tx: TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	) {
		let (progress, best_block) = Self::wait_for_best_block(tx).await;
		if best_block.is_none() {
			log::info!(
				url = self.url,
				extrinsic_hash = &tx_hashes.extrinsic_hash,
				midnight_tx_hash = &tx_hashes.midnight_tx_hash;
				"FAILED_TO_REACH_BEST_BLOCK"
			);
			return;
		}
		let best_block = best_block.unwrap();
		log::info!(
			url = self.url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"BEST_BLOCK"
		);

		let finalized = Self::wait_for_finalized(progress).await;
		let message = if finalized.is_some() { "FINALIZED" } else { "FAILED_TO_FINALIZE" };
		log::info!(
			url = self.url,
			extrinsic_hash = &tx_hashes.extrinsic_hash,
			midnight_tx_hash = &tx_hashes.midnight_tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"{message}"
		);
	}

	fn error(&self, e: subxt::Error) -> SendToUrlError {
		SendToUrlError { url: self.url.clone(), source: e }
	}
}

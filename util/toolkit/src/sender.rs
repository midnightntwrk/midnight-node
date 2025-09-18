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

use crate::mn_meta;
use midnight_node_ledger_helpers::*;
use std::{marker::PhantomData, sync::Arc};
use subxt::{
	OnlineClient, PolkadotConfig,
	tx::{TxInBlock, TxProgress},
};
use tokio::sync::Semaphore;

use crate::hash_to_str;

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
	) -> Result<(), subxt::Error> {
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
				let (tx_hash_string, tx_progress) =
					self_clone.send_tx_no_wait(&tx.tx).await.expect("Failed to send tx");
				self_clone.send_and_log(&tx_hash_string, tx_progress).await;
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
	) -> Result<(String, TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>), subxt::Error> {
		let tx_serialize = tx.serialize_inner()?;

		let mn_tx = mn_meta::tx().midnight().send_mn_transaction(tx_serialize);
		let unsigned_extrinsic = self.api.tx().create_unsigned(&mn_tx)?;
		let tx_hash_string = format!("0x{}", hex::encode(unsigned_extrinsic.hash().as_bytes()));

		log::info!(
			url = self.url,
			tx_hash = &tx_hash_string;
			"SENDING"
		);
		let tx_progress = self.api.tx().create_unsigned(&mn_tx)?.submit_and_watch().await?;
		log::info!(
			url = self.url,
			tx_hash = &tx_hash_string;
			"SENT"
		);
		Ok((tx_hash_string, tx_progress))
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
		tx_hash: &str,
		tx: TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
	) {
		let (progress, best_block) = Self::wait_for_best_block(tx).await;
		if best_block.is_none() {
			log::info!(
				url = self.url,
				tx_hash;
				"FAILED_TO_REACH_BEST_BLOCK"
			);
			return;
		}
		let best_block = best_block.unwrap();
		log::info!(
			url = self.url,
			tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"BEST_BLOCK"
		);

		let finalized = Self::wait_for_finalized(progress).await;
		let message = if finalized.is_some() { "FINALIZED" } else { "FAILED_TO_FINALIZE" };
		log::info!(
			url = self.url,
			tx_hash,
			block_hash = hash_to_str(best_block.block_hash()).as_str();
			"{message}"
		);
	}
}

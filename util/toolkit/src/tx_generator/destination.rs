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

use async_trait::async_trait;
use midnight_node_ledger_helpers::*;
use std::{fs::File, io::Write, marker::PhantomData, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

use crate::{
	sender::Sender,
	serde_def::{DeserializedTransactionsWithContext, SerializedTransactionsWithContext},
};

pub const DEFAULT_DEST_URL: &'static str = "ws://127.0.0.1:9944";

#[derive(clap::Args)]
pub struct Destination {
	/// RPC URL(s) of node instance(s) used to send generated transactions. Can set multiple.
	#[arg(long = "dest-url", short = 'd', conflicts_with = "dest_file", default_values_t = [DEFAULT_DEST_URL.to_string()], global = true)]
	pub dest_urls: Vec<String>,
	/// The rate at which to send txs (per second)
	#[arg(long, short, default_value = "1", conflicts_with = "dest_file", global = true)]
	pub rate: f32,
	/// Output filename to write generated transaction.
	#[arg(long, conflicts_with = "dest_urls", global = true)]
	pub dest_file: Option<String>,
	/// Save generated tx file as bytes rather than JSON.
	#[arg(long, default_value = "false", conflicts_with = "dest_urls", global = true)]
	pub to_bytes: bool,
}

pub struct SendTxsToFile<S, P> {
	file: String,
	to_bytes: bool,
	_marker_p: PhantomData<P>,
	_marker_s: PhantomData<S>,
}

impl<S: SignatureKind<DefaultDB> + Tagged, P: ProofKind<DefaultDB> + Send + Sync + 'static>
	SendTxsToFile<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PedersenRandomness, DefaultDB>: Tagged,
{
	pub fn new(file: String, to_bytes: bool) -> Self {
		Self { file, to_bytes, _marker_p: PhantomData, _marker_s: PhantomData }
	}

	fn save_json_file(
		&self,
		txs: &DeserializedTransactionsWithContext<S, P>,
		filename: &str,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let mut file = File::create(filename)?;
		let generated_tx = SerializedTransactionsWithContext::new(txs)?;
		file.write_all(&serde_json::to_vec(&generated_tx)?)?;
		Ok(())
	}
}

pub struct SendTxsToUrl<
	S: SignatureKind<DefaultDB>,
	P: ProofKind<DefaultDB> + Send + Sync + 'static,
> {
	sender: Arc<Sender<S, P>>,
	rate: f32,
}

impl<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>
	SendTxsToUrl<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send,
{
	pub fn new(sender: Arc<Sender<S, P>>, rate: f32) -> Self {
		Self { sender, rate }
	}
}

#[async_trait]
pub trait SendTxs<
	S: SignatureKind<DefaultDB> + Tagged + Send + 'static,
	P: ProofKind<DefaultDB> + Send + 'static,
> where
	Transaction<S, P, PedersenRandomness, DefaultDB>: Tagged,
{
	async fn send_txs(
		&self,
		txs: &DeserializedTransactionsWithContext<S, P>,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged + Send + 'static,
	P: ProofKind<DefaultDB> + Send + 'static,
> SendTxs<S, P> for ()
{
	async fn send_txs(
		&self,
		_txs: &DeserializedTransactionsWithContext<S, P>,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		Ok(())
	}
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged + Send + Sync + 'static,
	P: ProofKind<DefaultDB> + Send + Sync + 'static,
> SendTxs<S, P> for SendTxsToFile<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PedersenRandomness, DefaultDB>: Tagged,
{
	async fn send_txs(
		&self,
		txs: &DeserializedTransactionsWithContext<S, P>,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		if !self.to_bytes {
			self.save_json_file(&txs, &self.file)?;
		} else if txs.batches.is_empty() {
			std::fs::write(&self.file, serialize(&txs.initial_tx)?)?;
		} else {
			std::fs::write(&self.file, serialize(&txs.clone().flat())?)?;
		}
		Ok(())
	}
}

#[async_trait]
impl<
	S: SignatureKind<DefaultDB> + Tagged + Send + Sync + 'static,
	P: ProofKind<DefaultDB> + Send + Sync + 'static,
> SendTxs<S, P> for SendTxsToUrl<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
	Transaction<S, P, PedersenRandomness, DefaultDB>: Tagged,
{
	async fn send_txs(
		&self,
		txs: &DeserializedTransactionsWithContext<S, P>,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let num_batches = txs.batches.len();
		let num_per_batch = txs.batches.first().map(|batch| batch.txs.len()).unwrap_or(0);
		let total_txs = num_per_batch * num_batches;

		println!("Sending initial tx...");
		self.sender.send_tx(&txs.initial_tx.tx).await?;

		for (i, batch) in txs.batches.iter().enumerate() {
			println!("Sending batch {}...", i);
			let semaphore = Arc::new(Semaphore::new(0));
			let sender = self.sender.clone();
			let worker = tokio::spawn(sender.send_worker(semaphore.clone(), batch.txs.clone()));

			// Trigger sending
			for _i in 0..total_txs {
				semaphore.add_permits(1);
				tokio::time::sleep(Duration::from_secs_f32(1f32 / self.rate)).await;
			}

			worker.await?;
		}
		Ok(())
	}
}

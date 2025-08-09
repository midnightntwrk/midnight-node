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
use std::{fs::File, io::Read, path::Path, sync::Arc};
use subxt::{OnlineClient, PolkadotConfig};
use thiserror::Error;

use crate::{
	ProofType, SignatureType,
	indexer::Indexer,
	mn_meta,
	remote_prover::RemoteProofServer,
	sender::Sender,
	serde_def::{DeserializedTransactionsWithContext, SerializedTransactionsWithContext},
};

pub mod builder;
pub mod destination;
pub mod source;

use builder::{BuildTxs, Builder, DynamicError};
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

pub struct TxGenerator<S: SignatureKind<DefaultDB>, P: ProofKind<DefaultDB> + Send + Sync + 'static>
{
	pub source: Box<dyn GetTxs<S, P>>,
	pub destination: Box<dyn SendTxs<S, P>>,
	pub builder: Box<dyn BuildTxs<Error = DynamicError>>,
	pub prover: Arc<dyn ProofProvider<DefaultDB>>,
}

impl<
	S: SignatureKind<DefaultDB> + Send + Sync + 'static,
	P: ProofKind<DefaultDB> + Send + Sync + 'static + std::fmt::Debug,
> TxGenerator<S, P>
where
	<P as ProofKind<DefaultDB>>::Pedersen: Send + Sync,
	<P as ProofKind<DefaultDB>>::LatestProof: Send + Sync,
	<P as ProofKind<DefaultDB>>::Proof: Send + Sync,
{
	pub async fn new(
		src: Source,
		dest: Destination,
		builder: Builder,
		proof_server: Option<String>,
	) -> Result<Self, TxGeneratorError> {
		let (source, network_id) = Self::source(src).await?;
		let destination = Self::destination(source.network_id(), dest).await?;
		let builder = builder.into();
		let prover = Self::prover(proof_server, network_id);

		Ok(Self { source, destination, builder, prover })
	}

	pub async fn source(src: Source) -> Result<(Box<dyn GetTxs<S, P>>, NetworkId), SourceError> {
		if let Some(ref src_files) = src.src_files {
			// As all txs in the files should belong to the same network, we can extract the value just from one file
			let path = Path::new(&src_files[0]);
			let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
			let mut file = File::open(path)?;

			let bytes = if extension == "json" {
				let loaded_txs: SerializedTransactionsWithContext = serde_json::from_reader(file)?;
				// The third byte in the file is the network id
				// See mn_ledger::serialize crate for more info
				let byte_hex = &loaded_txs.initial_tx[0..6];
				hex::decode(byte_hex)?
			} else {
				// The third byte in the file is the network id
				// See mn_ledger::serialize crate for more info
				let mut bytes = vec![0u8; 3];
				file.read_exact(&mut bytes)?;
				bytes
			};

			let network_id = NetworkId::deserialize(&mut &bytes[..], 0)?;

			let source: Box<dyn GetTxs<S, P>> =
				Box::new(GetTxsFromFile::new(network_id, src_files.clone(), extension.to_string()));

			Ok((source, network_id))
		} else if let Some(url) = src.src_url {
			let api = OnlineClient::<PolkadotConfig>::from_insecure_url(url.clone()).await?;
			let storage_query = mn_meta::storage().midnight().network_id();
			let network_id_vec =
				api.storage().at_latest().await.unwrap().fetch(&storage_query).await?;

			let network_id = if let Some(val) = network_id_vec {
				match val.0[0] {
					1 => NetworkId::DevNet,
					2 => NetworkId::TestNet,
					3 => NetworkId::MainNet,
					_ => NetworkId::Undeployed,
				}
			} else {
				NetworkId::Undeployed
			};

			let indexer =
				Arc::new(Indexer::<S, P>::new(network_id, url, api, src.fetch_concurrency).await?);
			let source: Box<dyn GetTxs<S, P>> = Box::new(GetTxsFromUrl::new(indexer));

			Ok((source, network_id))
		} else {
			unreachable!()
		}
	}

	async fn destination(
		network_id: NetworkId,
		dest: Destination,
	) -> Result<Box<dyn SendTxs<S, P>>, DestinationError> {
		if let Some(ref dest_file) = dest.dest_file {
			let destination: Box<dyn SendTxs<S, P>> =
				Box::new(SendTxsToFile::new(network_id, dest_file.clone(), dest.to_bytes));

			Ok(destination)
		} else if let Some(url) = dest.dest_url {
			let api = OnlineClient::<PolkadotConfig>::from_insecure_url(url.clone()).await?;
			let sender = Arc::new(Sender::<S, P>::new(network_id, api));
			let destination: Box<dyn SendTxs<S, P>> =
				Box::new(SendTxsToUrl::new(sender, dest.rate));

			Ok(destination)
		} else {
			unreachable!()
		}
	}

	fn prover(
		proof_server: Option<String>,
		network_id: NetworkId,
	) -> Arc<dyn ProofProvider<DefaultDB>> {
		if let Some(url) = proof_server {
			Arc::new(RemoteProofServer::new(url, network_id))
		} else {
			Arc::new(LocalProofServer::new())
		}
	}

	pub async fn get_txs(
		&self,
	) -> Result<DeserializedTransactionsWithContext<S, P>, Box<dyn std::error::Error>> {
		let txs = self.source.get_txs().await?;
		Ok(txs)
	}

	pub async fn send_txs(
		&self,
		txs: &DeserializedTransactionsWithContext<S, P>,
	) -> Result<(), Box<dyn std::error::Error>> {
		self.destination.send_txs(txs).await
	}

	pub async fn build_txs(
		&self,
		received_txs: &DeserializedTransactionsWithContext<SignatureType, ProofType>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, DynamicError> {
		self.builder.build_txs_from(received_txs.clone(), self.prover.clone()).await
	}
}

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

use std::time::Duration;

use backoff::ExponentialBackoff;
use backoff::future::retry;
use midnight_node_ledger_helpers::{LedgerParameters, deserialize};
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use parity_scale_codec::Decode;
use subxt::config::HashFor;
use subxt::rpcs::methods::legacy::{BlockNumber, SystemProperties};
use subxt::utils::{AccountId32, MultiAddress, MultiSignature};
use subxt::{
	Config, OnlineClient,
	config::substrate::{BlakeTwo256, SubstrateExtrinsicParams, SubstrateHeader},
	rpcs::{LegacyRpcMethods, RpcClient},
};
use thiserror::Error;

/// Maximum time to wait for a client connection before giving up.
/// Set generously to handle rate-limiting (429) during concurrent connection attempts.
const CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default)]
pub struct MidnightNodeClientConfig;

impl Config for MidnightNodeClientConfig {
	type AccountId = AccountId32;
	type Address = MultiAddress<Self::AccountId, ()>;
	type Signature = MultiSignature;
	type Hasher = BlakeTwo256;
	type Header = SubstrateHeader<<Self::Hasher as subxt::config::Hasher>::Hash>;
	type TransactionExtensions = SubstrateExtrinsicParams<Self>;
	type AssetId = u32;
}

impl subxt::rpcs::RpcConfig for MidnightNodeClientConfig {
	type Header = SubstrateHeader<<BlakeTwo256 as subxt::config::Hasher>::Hash>;
	type Hash = <BlakeTwo256 as subxt::config::Hasher>::Hash;
	type AccountId = AccountId32;
}

pub struct MidnightNodeClient {
	pub api: OnlineClient<MidnightNodeClientConfig>,
	pub rpc: LegacyRpcMethods<MidnightNodeClientConfig>,
	pub rpc_client: RpcClient,
}

impl MidnightNodeClient {
	pub async fn new(rpc_url: &str, timeout: Option<Duration>) -> Result<Self, ClientError> {
		let backoff = ExponentialBackoff {
			max_elapsed_time: Some(timeout.unwrap_or(CLIENT_CONNECT_TIMEOUT)),
			..ExponentialBackoff::default()
		};

		retry(backoff, || async {
			MidnightNodeClient::new_without_timeout(rpc_url).await.map_err(|e| {
				log::warn!("rpc connection attempt failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await
	}

	pub async fn new_without_timeout(rpc_url: &str) -> Result<Self, ClientError> {
		let rpc_client = RpcClient::from_insecure_url(rpc_url).await?;
		let rpc = LegacyRpcMethods::<MidnightNodeClientConfig>::new(rpc_client.clone());
		let api =
			OnlineClient::<MidnightNodeClientConfig>::from_rpc_client(rpc_client.clone()).await?;
		Ok(MidnightNodeClient { rpc, api, rpc_client })
	}

	pub async fn get_network_id(&self) -> Result<String, ClientError> {
		// let storage_query = mn_meta::storage().midnight().network_id();
		// let network_id = self.api.storage().at_latest().await?.fetch(&storage_query).await??;
		let network_id_call =
			mn_meta::runtime_apis::RuntimeApi.midnight_runtime_api().get_network_id();
		// Submit the call and get back a result.
		let network_id =
			self.api.at_current_block().await?.runtime_apis().call(network_id_call).await?;

		Ok(network_id)
	}

	pub async fn get_state_root_at(
		&self,
		at: Option<HashFor<MidnightNodeClientConfig>>,
	) -> Result<Option<Vec<u8>>, ClientError> {
		// Use a raw storage query to avoid IncompatibleCodegen errors when the
		// toolkit is compiled against a different runtime version than the node.
		// The storage key is stable: twox_128("Midnight") ++ twox_128("StateKey").
		let key =
			[sp_crypto_hashing::twox_128(b"Midnight"), sp_crypto_hashing::twox_128(b"StateKey")]
				.concat();
		let at_block = match at {
			Some(hash) => self.api.at_block(hash).await?,
			None => self.api.at_current_block().await?,
		};
		let storage = at_block.storage();
		let raw = match storage.fetch_raw(key).await {
			Ok(bytes) => Some(bytes),
			Err(subxt::error::StorageError::NoValueFound) => None,
			Err(e) => return Err(e.into()),
		};
		Ok(raw.map(|bytes| Vec::<u8>::decode(&mut &bytes[..]).expect("failed to decode StateKey")))
	}

	pub async fn get_block_one_hash(
		&self,
	) -> Result<HashFor<MidnightNodeClientConfig>, ClientError> {
		if self.get_finalized_height().await? < 1 {
			return Err(ClientError::OnlyGenesisFinalized);
		}
		let hash = self.rpc.chain_get_block_hash(Some(BlockNumber::Number(1))).await?;
		hash.ok_or_else(|| ClientError::BlockHashNotFound(1))
	}

	pub async fn get_system_properties(&self) -> Result<SystemProperties, ClientError> {
		let system_properties = self.rpc.system_properties().await?;
		Ok(system_properties)
	}

	pub async fn get_finalized_height(&self) -> Result<u64, ClientError> {
		let latest_block = self.api.at_current_block().await?;
		Ok(latest_block.block_number())
	}

	/// Best (head-of-chain) block height. May be ahead of finality; can also reorg.
	/// Use this when you only need "the node is producing"; use
	/// [`Self::get_finalized_height`] when you need a stable, GRANDPA-finalized number.
	pub async fn get_best_height(&self) -> Result<u64, ClientError> {
		let header = self.rpc.chain_get_header(None).await?.ok_or(ClientError::NoBestHeader)?;
		Ok(header.number)
	}

	pub async fn get_ledger_parameters(&self) -> Result<LedgerParameters, ClientError> {
		let call = mn_meta::runtime_apis::RuntimeApi.midnight_runtime_api().get_ledger_parameters();
		let response = self.api.at_current_block().await?.runtime_apis().call(call).await?;
		let bytes = response.expect("Unable to retrieve ledger parameters from RPC server");
		let parameters: LedgerParameters = deserialize(&mut &bytes[..])
			.map_err(|e| ClientError::DeserializeLedgerParameters(e.into()))?;
		Ok(parameters)
	}
}

#[derive(Error, Debug)]
pub enum ClientError {
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("subxt_rpc error: {0}")]
	RpcClientError(#[from] subxt::rpcs::Error),
	#[error("online client error: {0}")]
	OnlineClientError(#[from] subxt::error::OnlineClientError),
	#[error("online client at block error: {0}")]
	OnlineClientAtBlockError(#[from] subxt::error::OnlineClientAtBlockError),
	#[error("runtime api error: {0}")]
	RuntimeApiError(#[from] subxt::error::RuntimeApiError),
	#[error("storage error: {0}")]
	StorageError(#[from] subxt::error::StorageError),
	#[error("midnight node client received an unsupported network id")]
	UnsupportedNetworkId(Vec<u8>),
	#[error("failed to get block hash for block {0}")]
	BlockHashNotFound(u32),
	#[error("chain not yet started - only genesis is finalized")]
	OnlyGenesisFinalized,
	#[error("node returned no best block header (rpc returned null)")]
	NoBestHeader,
	#[error("Failed to deserialize ledger parameters: {0}")]
	DeserializeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
}

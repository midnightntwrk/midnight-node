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

use crate::fetcher::runtimes::{RuntimeVersion, RuntimeVersionError};

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
		runtime_version: Option<RuntimeVersion>,
	) -> Result<Option<Vec<u8>>, ClientError> {
		let at_block = match at {
			Some(hash) => self.api.at_block(hash).await?,
			None => self.api.at_current_block().await?,
		};

		let runtime_version = match runtime_version {
			Some(v) => v,
			None => {
				let header = at_block.block_header().await?;
				RuntimeVersion::from_header(&header)?
			},
		};

		match runtime_version {
			RuntimeVersion::V0_21_0 => {
				// V0_21_0 predates the `get_ledger_state_root` runtime API and stores
				// `tagged_serialize(&Ledger::as_typed_key())` in `Midnight::StateKey`
				// — i.e., the typed-key hash of `Ledger<D>` (which wraps `LedgerState`
				// with a `block_fullness` field). The consumer (`LedgerContext::
				// compute_state_root`) hashes `LedgerState<D>` directly to match the
				// V0_22_0+ runtime API, so V0_21_0's stored hash is over a different
				// struct and cannot be transformed to match. We have no way to derive
				// the pure `LedgerState` typed key without the runtime API, so skip
				// verification for V0_21_0 blocks.
				Ok(None)
			},
			// V0_22_0 onwards all expose `get_ledger_state_root() -> Result<Vec<u8>,
			// LedgerApiError>`. Call it raw: subxt's static codegen validates the
			// method's type hash against the metadata of the block being queried, and
			// the checked-in `midnight_metadata_<version>.scale` snapshots are not
			// faithful to the released runtimes of the same name (e.g. the 1.0.0 file
			// carries ledger-9 types and a `C2MBridge` pallet the 1.0.1 release does
			// not have), so validation fails with `IncompatibleCodegen` against a real
			// pre-fork node. `call_raw` skips validation; the wire shape is stable.
			_ => {
				let raw = at_block
					.runtime_apis()
					.call_raw("MidnightRuntimeApi_get_ledger_state_root", None)
					.await?;
				// `Result<Vec<u8>, LedgerApiError>`. The error variant's payload differs
				// per runtime version, so decode it as opaque.
				//
				// An error here is not fatal: the state root only feeds block
				// verification, which is skipped when it is `None` (as for V0_21_0
				// above). The hardfork's `set_code` block *always* errors — it stores
				// the new code but its ledger data is still the old version, so the
				// new runtime's deserializer rejects it (`0x010005` =
				// `LedgerApiError::Deserialization`). That block is genuinely
				// unverifiable, and one unverified block must not fail the whole fetch.
				match Result::<Vec<u8>, ()>::decode(&mut &raw[..]) {
					Ok(Ok(root)) => Ok(Some(root)),
					_ => {
						log::warn!(
							"get_ledger_state_root failed at block {:?} (raw: 0x{}); \
							 skipping state-root verification for this block",
							at_block.block_ref().hash(),
							hex::encode(&raw),
						);
						Ok(None)
					},
				}
			},
		}
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

	/// Whether `hash` is on the finalized chain: at or below the finalized head
	/// and canonical at its height.
	pub async fn is_block_finalized(
		&self,
		hash: HashFor<MidnightNodeClientConfig>,
	) -> Result<bool, ClientError> {
		let Some(header) = self.rpc.chain_get_header(Some(hash)).await? else {
			// The node no longer knows the block: pruned or reorged away.
			return Ok(false);
		};
		if header.number > self.get_finalized_height().await? {
			return Ok(false);
		}
		let canonical =
			self.rpc.chain_get_block_hash(Some(BlockNumber::Number(header.number))).await?;
		Ok(canonical == Some(hash))
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
	#[error("block error: {0}")]
	BlockError(#[from] subxt::error::BlockError),
	#[error("runtime version error: {0}")]
	RuntimeVersion(#[from] RuntimeVersionError),
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

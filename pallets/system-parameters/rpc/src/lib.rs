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

//! RPC endpoints for the System Parameters pallet
//!
//! This module provides RPC endpoints for accessing system parameters:
//! - `systemParameters_getTermsAndConditions` - Get current Terms and Conditions
//! - `systemParameters_getDParameter` - Get current D Parameter
//! - `systemParameters_getAriadneParameters` - Get Ariadne parameters with D Parameter from pallet
//!
//! The `getAriadneParameters` endpoint returns the same response schema as
//! `sidechain_getAriadneParameters` but sources the D Parameter from the on-chain
//! `pallet-system-parameters` instead of from Cardano.

use std::fmt::{Display, Formatter};
use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned, INTERNAL_ERROR_CODE},
};
use serde::{Deserialize, Serialize};

use pallet_system_parameters::SystemParametersApi;
use sc_client_api::BlockchainEvents;
use sidechain_domain::McEpochNumber;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use sp_session_validator_management_query::SessionValidatorManagementQueryApi;

/// Terms and Conditions response for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermsAndConditionsRpcResponse {
	/// SHA-256 hash of the terms and conditions document
	pub hash: H256,
	/// URL where the terms and conditions can be found
	pub url: String,
}

/// D-Parameter response for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DParameterRpcResponse {
	/// Number of permissioned candidates
	pub num_permissioned_candidates: u16,
	/// Number of registered candidates
	pub num_registered_candidates: u16,
}

/// Ariadne parameters response
///
/// Returns the same schema as `sidechain_getAriadneParameters` but with D Parameter
/// sourced from pallet-system-parameters instead of Cardano.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AriadneParametersRpcResponse {
	/// The D-parameter (from pallet-system-parameters)
	pub d_parameter: DParameterRpcResponse,
	/// List of permissioned candidates from Cardano. None signifies a list was not set on mainchain.
	pub permissioned_candidates: Option<Vec<serde_json::Value>>,
	/// Map of candidate registrations from Cardano
	pub candidate_registrations: serde_json::Value,
}

/// RPC error types
#[derive(Debug)]
pub enum SystemParametersRpcError {
	/// Unable to get terms and conditions
	UnableToGetTermsAndConditions,
	/// Unable to get D-parameter
	UnableToGetDParameter,
	/// Unable to get Ariadne parameters
	UnableToGetAriadneParameters(String),
	/// Runtime API error
	RuntimeApiError(String),
}

impl Display for SystemParametersRpcError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			SystemParametersRpcError::UnableToGetTermsAndConditions => {
				write!(f, "Unable to get terms and conditions")
			},
			SystemParametersRpcError::UnableToGetDParameter => {
				write!(f, "Unable to get D-parameter")
			},
			SystemParametersRpcError::UnableToGetAriadneParameters(msg) => {
				write!(f, "Unable to get Ariadne parameters: {}", msg)
			},
			SystemParametersRpcError::RuntimeApiError(msg) => {
				write!(f, "Runtime API error: {}", msg)
			},
		}
	}
}

impl std::error::Error for SystemParametersRpcError {}

impl From<SystemParametersRpcError> for ErrorObjectOwned {
	fn from(value: SystemParametersRpcError) -> Self {
		ErrorObject::owned(INTERNAL_ERROR_CODE, value.to_string(), None::<()>)
	}
}

/// System Parameters RPC API definition
#[rpc(client, server)]
pub trait SystemParametersRpcApi<BlockHash> {
	/// Get the current Terms and Conditions
	///
	/// Returns the hash (hex-encoded) and URL of the current terms and conditions,
	/// or null if not set.
	#[method(name = "systemParameters_getTermsAndConditions")]
	fn get_terms_and_conditions(
		&self,
		at: Option<BlockHash>,
	) -> RpcResult<Option<TermsAndConditionsRpcResponse>>;

	/// Get the current D-Parameter
	///
	/// Returns the number of permissioned and registered candidates.
	#[method(name = "systemParameters_getDParameter")]
	fn get_d_parameter(&self, at: Option<BlockHash>) -> RpcResult<DParameterRpcResponse>;

	/// Get Ariadne parameters for a given mainchain epoch.
	///
	/// Returns permissioned candidates and candidate registrations from Cardano,
	/// but the D Parameter is sourced from `pallet-system-parameters` on-chain storage.
	///
	/// # Parameters
	/// - `epoch_number`: The mainchain epoch number to query candidates for
	/// - `d_parameter_at`: Optional block hash to query D Parameter from. If not provided,
	///   uses the best (latest) block. This is useful when querying historical epoch data
	///   and you want the D Parameter value that was in effect at a specific block.
	///
	/// This endpoint should be used instead of `sidechain_getAriadneParameters` which
	/// sources D Parameter from the deprecated Cardano contract.
	#[method(name = "systemParameters_getAriadneParameters")]
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
		d_parameter_at: Option<BlockHash>,
	) -> RpcResult<AriadneParametersRpcResponse>;
}

/// System Parameters RPC implementation
pub struct SystemParametersRpc<C, Block, Q> {
	client: Arc<C>,
	query_api: Arc<Q>,
	_marker: std::marker::PhantomData<Block>,
}

impl<C, Block, Q> SystemParametersRpc<C, Block, Q> {
	/// Create a new instance of the System Parameters RPC handler
	pub fn new(client: Arc<C>, query_api: Arc<Q>) -> Self {
		Self { client, query_api, _marker: Default::default() }
	}
}

#[async_trait]
impl<C, Block, Q> SystemParametersRpcApiServer<<Block as BlockT>::Hash>
	for SystemParametersRpc<C, Block, Q>
where
	Block: BlockT,
	C: Send + Sync + 'static,
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block>,
	C: BlockchainEvents<Block>,
	C::Api: SystemParametersApi<Block, H256>,
	Q: SessionValidatorManagementQueryApi + Send + Sync + 'static,
{
	fn get_terms_and_conditions(
		&self,
		at: Option<<Block as BlockT>::Hash>,
	) -> RpcResult<Option<TermsAndConditionsRpcResponse>> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		let api = self.client.runtime_api();

		let result = api
			.get_terms_and_conditions(at)
			.map_err(|e| SystemParametersRpcError::RuntimeApiError(format!("{:?}", e)))?;

		Ok(result.map(|tc| TermsAndConditionsRpcResponse {
			hash: tc.hash,
			url: String::from_utf8_lossy(&tc.url).to_string(),
		}))
	}

	fn get_d_parameter(
		&self,
		at: Option<<Block as BlockT>::Hash>,
	) -> RpcResult<DParameterRpcResponse> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		let api = self.client.runtime_api();

		let result = api
			.get_d_parameter(at)
			.map_err(|e| SystemParametersRpcError::RuntimeApiError(format!("{:?}", e)))?;

		Ok(DParameterRpcResponse {
			num_permissioned_candidates: result.num_permissioned_candidates,
			num_registered_candidates: result.num_registered_candidates,
		})
	}

	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
		d_parameter_at: Option<<Block as BlockT>::Hash>,
	) -> RpcResult<AriadneParametersRpcResponse> {
		// Get the full Ariadne parameters from the underlying query API
		// (this gets candidates from Cardano)
		let ariadne_params = self
			.query_api
			.get_ariadne_parameters(epoch_number)
			.await
			.map_err(|e| SystemParametersRpcError::UnableToGetAriadneParameters(e))?;

		// Determine which block to query D Parameter from
		let block_hash = d_parameter_at.unwrap_or_else(|| self.client.info().best_hash);

		// Get D Parameter from pallet-system-parameters at the specified block
		let pallet_d_param = self
			.client
			.runtime_api()
			.get_d_parameter(block_hash)
			.map_err(|e| SystemParametersRpcError::RuntimeApiError(format!("{:?}", e)))?;

		Ok(AriadneParametersRpcResponse {
			d_parameter: DParameterRpcResponse {
				num_permissioned_candidates: pallet_d_param.num_permissioned_candidates,
				num_registered_candidates: pallet_d_param.num_registered_candidates,
			},
			permissioned_candidates: ariadne_params.permissioned_candidates.map(|candidates| {
				candidates
					.into_iter()
					.map(|c| serde_json::to_value(c).unwrap_or_default())
					.collect()
			}),
			candidate_registrations: serde_json::to_value(&ariadne_params.candidate_registrations)
				.unwrap_or_default(),
		})
	}
}

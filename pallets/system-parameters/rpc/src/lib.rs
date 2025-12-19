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
use sp_session_validator_management_query::types::AriadneParameters;

/// Terms and Conditions response for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermsAndConditionsRpcResponse {
	/// SHA-256 hash of the terms and conditions document (hex-encoded)
	pub hash: String,
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
	/// This endpoint should be used instead of `sidechain_getAriadneParameters` which
	/// sources D Parameter from the deprecated Cardano contract.
	#[method(name = "systemParameters_getAriadneParameters")]
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
	) -> RpcResult<AriadneParameters>;
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
			hash: format!("0x{}", hex::encode(tc.hash.as_bytes())),
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
	) -> RpcResult<AriadneParameters> {
		// Get the full Ariadne parameters from the underlying query API
		// (this gets candidates from Cardano and D Parameter from Cardano)
		let mut ariadne_params = self
			.query_api
			.get_ariadne_parameters(epoch_number)
			.await
			.map_err(|e| SystemParametersRpcError::UnableToGetAriadneParameters(e))?;

		// Replace D Parameter with the value from pallet-system-parameters
		let best_block = self.client.info().best_hash;
		let pallet_d_param = self
			.client
			.runtime_api()
			.get_d_parameter(best_block)
			.map_err(|e| SystemParametersRpcError::RuntimeApiError(format!("{:?}", e)))?;

		// Update the D Parameter in the response
		ariadne_params.d_parameter.num_permissioned_candidates =
			pallet_d_param.num_permissioned_candidates;
		ariadne_params.d_parameter.num_registered_candidates =
			pallet_d_param.num_registered_candidates;

		Ok(ariadne_params)
	}
}

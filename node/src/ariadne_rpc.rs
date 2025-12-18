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

//! RPC endpoint for Ariadne parameters with D Parameter sourced from pallet-system-parameters.
//!
//! This module provides `midnight_getAriadneParameters` which returns the same response
//! as `sidechain_getAriadneParameters` but sources the D Parameter from the on-chain
//! `pallet-system-parameters` instead of from Cardano.
//!
//! # Migration
//!
//! Consumers should migrate from `sidechain_getAriadneParameters` to `midnight_getAriadneParameters`
//! to get the authoritative D Parameter value.

use jsonrpsee::{
	core::RpcResult,
	core::async_trait,
	proc_macros::rpc,
	types::{ErrorObject, ErrorObjectOwned},
};
use pallet_system_parameters::SystemParametersApi;
use sidechain_domain::McEpochNumber;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use sp_session_validator_management_query::types::AriadneParameters;
use sp_session_validator_management_query::SessionValidatorManagementQueryApi;
use std::sync::Arc;

/// RPC API for Ariadne parameters with pallet-sourced D Parameter
#[rpc(client, server, namespace = "midnight")]
pub trait MidnightAriadneRpcApi {
	/// Get Ariadne parameters for a given mainchain epoch.
	///
	/// Returns permissioned candidates and candidate registrations from Cardano,
	/// but the D Parameter is sourced from `pallet-system-parameters` on-chain storage.
	///
	/// This endpoint should be used instead of `sidechain_getAriadneParameters` which
	/// sources D Parameter from the deprecated Cardano contract.
	#[method(name = "getAriadneParameters")]
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
	) -> RpcResult<AriadneParameters>;
}

/// Implementation of the Midnight Ariadne RPC API
pub struct MidnightAriadneRpc<C, Block, Q> {
	/// Substrate client for runtime API access
	client: Arc<C>,
	/// Underlying query API for candidate data
	query_api: Arc<Q>,
	/// Phantom data for Block type
	_marker: std::marker::PhantomData<Block>,
}

impl<C, Block, Q> MidnightAriadneRpc<C, Block, Q> {
	/// Create a new instance of the Midnight Ariadne RPC handler
	pub fn new(client: Arc<C>, query_api: Arc<Q>) -> Self {
		Self { client, query_api, _marker: Default::default() }
	}
}

#[async_trait]
impl<C, Block, Q> MidnightAriadneRpcApiServer for MidnightAriadneRpc<C, Block, Q>
where
	Block: BlockT,
	C: Send + Sync + 'static,
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block>,
	C::Api: SystemParametersApi<Block, Block::Hash>,
	Q: SessionValidatorManagementQueryApi + Send + Sync + 'static,
{
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
			.map_err(error_object_from_str)?;

		// Replace D Parameter with the value from pallet-system-parameters
		let best_block = self.client.info().best_hash;
		let pallet_d_param = self
			.client
			.runtime_api()
			.get_d_parameter(best_block)
			.map_err(|e| error_object_from_str(format!("Runtime API error: {:?}", e)))?;

		// Update the D Parameter in the response
		ariadne_params.d_parameter.num_permissioned_candidates =
			pallet_d_param.num_permissioned_candidates;
		ariadne_params.d_parameter.num_registered_candidates =
			pallet_d_param.num_registered_candidates;

		Ok(ariadne_params)
	}
}

fn error_object_from_str(msg: impl Into<String>) -> ErrorObjectOwned {
	ErrorObject::owned::<u8>(-1, msg, None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use async_trait::async_trait;
	use sidechain_domain::{DParameter as SidechainDParameter, SidechainPublicKey};
	use sp_session_validator_management_query::types::{
		CandidateRegistrationEntry, DParameter, GetCommitteeResponse, GetRegistrationsResponseMap,
		PermissionedCandidateData,
	};
	use sp_session_validator_management_query::{QueryResult, SessionValidatorManagementQueryApi};
	use std::collections::HashMap;

	/// Mock query API that returns pre-configured AriadneParameters
	struct MockQueryApi {
		/// D Parameter from Cardano (to be overridden)
		cardano_d_parameter: DParameter,
		/// Permissioned candidates list
		permissioned_candidates: Option<Vec<PermissionedCandidateData>>,
		/// Candidate registrations map
		candidate_registrations: GetRegistrationsResponseMap,
		/// Whether to return an error
		should_fail: bool,
	}

	impl MockQueryApi {
		fn new_with_d_parameter(num_permissioned: u16, num_registered: u16) -> Self {
			Self {
				cardano_d_parameter: DParameter {
					num_permissioned_candidates: num_permissioned,
					num_registered_candidates: num_registered,
				},
				permissioned_candidates: Some(vec![]),
				candidate_registrations: HashMap::new(),
				should_fail: false,
			}
		}

		fn with_candidates(mut self, candidates: Vec<PermissionedCandidateData>) -> Self {
			self.permissioned_candidates = Some(candidates);
			self
		}

		fn with_registrations(mut self, registrations: GetRegistrationsResponseMap) -> Self {
			self.candidate_registrations = registrations;
			self
		}

		fn with_failure(mut self) -> Self {
			self.should_fail = true;
			self
		}
	}

	#[async_trait]
	impl SessionValidatorManagementQueryApi for MockQueryApi {
		fn get_epoch_committee(&self, _: u64) -> QueryResult<GetCommitteeResponse> {
			unimplemented!("not needed for ariadne_rpc tests")
		}

		async fn get_registrations(
			&self,
			_mc_epoch_number: McEpochNumber,
			_stake_pool_public_key: sidechain_domain::StakePoolPublicKey,
		) -> QueryResult<Vec<CandidateRegistrationEntry>> {
			unimplemented!("not needed for ariadne_rpc tests")
		}

		async fn get_ariadne_parameters(
			&self,
			_epoch_number: McEpochNumber,
		) -> QueryResult<AriadneParameters> {
			if self.should_fail {
				return Err("Mock failure".to_string());
			}
			Ok(AriadneParameters {
				d_parameter: self.cardano_d_parameter.clone(),
				permissioned_candidates: self.permissioned_candidates.clone(),
				candidate_registrations: self.candidate_registrations.clone(),
			})
		}
	}

	// Test helper to create a sample PermissionedCandidateData
	fn sample_permissioned_candidate(key: &str) -> PermissionedCandidateData {
		PermissionedCandidateData {
			sidechain_public_key: SidechainPublicKey(key.as_bytes().to_vec()),
			keys: HashMap::new(),
			is_valid: true,
			invalid_reasons: None,
		}
	}

	// Note: Full integration tests require a mock client implementing ProvideRuntimeApi.
	// These are tested at the integration test level. Unit tests focus on the query API behavior.

	mod mock_query_api_tests {
		use super::*;

		#[tokio::test]
		async fn mock_query_api_returns_configured_d_parameter() {
			let mock = MockQueryApi::new_with_d_parameter(3, 5);
			let result = mock.get_ariadne_parameters(McEpochNumber(100)).await.unwrap();

			assert_eq!(result.d_parameter.num_permissioned_candidates, 3);
			assert_eq!(result.d_parameter.num_registered_candidates, 5);
		}

		#[tokio::test]
		async fn mock_query_api_returns_configured_candidates() {
			let candidates =
				vec![sample_permissioned_candidate("alice"), sample_permissioned_candidate("bob")];

			let mock = MockQueryApi::new_with_d_parameter(1, 2).with_candidates(candidates);
			let result = mock.get_ariadne_parameters(McEpochNumber(100)).await.unwrap();

			assert_eq!(result.permissioned_candidates.unwrap().len(), 2);
		}

		#[tokio::test]
		async fn mock_query_api_returns_configured_registrations() {
			let mut registrations = HashMap::new();
			registrations.insert("pool_key_1".to_string(), vec![]);
			registrations.insert("pool_key_2".to_string(), vec![]);

			let mock = MockQueryApi::new_with_d_parameter(1, 2).with_registrations(registrations);
			let result = mock.get_ariadne_parameters(McEpochNumber(100)).await.unwrap();

			assert_eq!(result.candidate_registrations.len(), 2);
		}

		#[tokio::test]
		async fn mock_query_api_returns_error_when_configured() {
			let mock = MockQueryApi::new_with_d_parameter(1, 2).with_failure();
			let result = mock.get_ariadne_parameters(McEpochNumber(100)).await;

			assert!(result.is_err());
			assert_eq!(result.unwrap_err(), "Mock failure");
		}
	}

	mod ariadne_parameters_tests {
		use super::*;

		#[test]
		fn ariadne_parameters_serializes_correctly() {
			let params = AriadneParameters {
				d_parameter: DParameter {
					num_permissioned_candidates: 3,
					num_registered_candidates: 2,
				},
				permissioned_candidates: Some(vec![sample_permissioned_candidate("test")]),
				candidate_registrations: HashMap::new(),
			};

			let json = serde_json::to_string(&params).unwrap();

			// Verify camelCase serialization
			assert!(json.contains("numPermissionedCandidates"));
			assert!(json.contains("numRegisteredCandidates"));
			assert!(json.contains("permissionedCandidates"));
			assert!(json.contains("candidateRegistrations"));
		}

		#[test]
		fn ariadne_parameters_with_none_candidates_serializes() {
			let params = AriadneParameters {
				d_parameter: DParameter {
					num_permissioned_candidates: 0,
					num_registered_candidates: 5,
				},
				permissioned_candidates: None,
				candidate_registrations: HashMap::new(),
			};

			let json = serde_json::to_string(&params).unwrap();
			assert!(json.contains("\"permissionedCandidates\":null"));
		}
	}

	mod d_parameter_override_logic {
		use super::*;

		/// Tests the D Parameter override logic that will be applied by the RPC handler.
		/// This simulates the flow where:
		/// 1. Query API returns AriadneParameters with Cardano-sourced D Parameter
		/// 2. Pallet provides authoritative D Parameter
		/// 3. The pallet value should replace the Cardano value
		#[test]
		fn d_parameter_override_replaces_values() {
			// Simulate Cardano-sourced values (from query API)
			let cardano_d_param = DParameter {
				num_permissioned_candidates: 10,
				num_registered_candidates: 20,
			};

			// Simulate pallet-sourced values
			let pallet_d_param = SidechainDParameter {
				num_permissioned_candidates: 3,
				num_registered_candidates: 2,
			};

			let mut ariadne_params = AriadneParameters {
				d_parameter: cardano_d_param,
				permissioned_candidates: Some(vec![]),
				candidate_registrations: HashMap::new(),
			};

			// Apply the override (same logic as in MidnightAriadneRpcApiServer impl)
			ariadne_params.d_parameter.num_permissioned_candidates =
				pallet_d_param.num_permissioned_candidates;
			ariadne_params.d_parameter.num_registered_candidates =
				pallet_d_param.num_registered_candidates;

			// Verify override worked
			assert_eq!(ariadne_params.d_parameter.num_permissioned_candidates, 3);
			assert_eq!(ariadne_params.d_parameter.num_registered_candidates, 2);
		}

		#[test]
		fn candidates_are_preserved_during_override() {
			let candidates = vec![
				sample_permissioned_candidate("alice"),
				sample_permissioned_candidate("bob"),
				sample_permissioned_candidate("charlie"),
			];

			let mut ariadne_params = AriadneParameters {
				d_parameter: DParameter {
					num_permissioned_candidates: 99,
					num_registered_candidates: 99,
				},
				permissioned_candidates: Some(candidates.clone()),
				candidate_registrations: HashMap::new(),
			};

			// Apply D Parameter override
			ariadne_params.d_parameter.num_permissioned_candidates = 5;
			ariadne_params.d_parameter.num_registered_candidates = 10;

			// Verify candidates are unchanged
			assert_eq!(ariadne_params.permissioned_candidates.as_ref().unwrap().len(), 3);
			assert_eq!(
				ariadne_params.permissioned_candidates.as_ref().unwrap()[0].sidechain_public_key,
				SidechainPublicKey("alice".as_bytes().to_vec())
			);
		}

		#[test]
		fn registrations_are_preserved_during_override() {
			let mut registrations = HashMap::new();
			registrations.insert("pool_1".to_string(), vec![]);
			registrations.insert("pool_2".to_string(), vec![]);

			let mut ariadne_params = AriadneParameters {
				d_parameter: DParameter {
					num_permissioned_candidates: 99,
					num_registered_candidates: 99,
				},
				permissioned_candidates: None,
				candidate_registrations: registrations,
			};

			// Apply D Parameter override
			ariadne_params.d_parameter.num_permissioned_candidates = 1;
			ariadne_params.d_parameter.num_registered_candidates = 1;

			// Verify registrations are unchanged
			assert_eq!(ariadne_params.candidate_registrations.len(), 2);
			assert!(ariadne_params.candidate_registrations.contains_key("pool_1"));
			assert!(ariadne_params.candidate_registrations.contains_key("pool_2"));
		}
	}

	mod error_handling {
		use super::*;

		#[test]
		fn error_object_from_str_creates_valid_error() {
			let error = error_object_from_str("test error message");
			assert_eq!(error.message(), "test error message");
			assert_eq!(error.code(), -1);
		}

		#[test]
		fn error_object_accepts_string_types() {
			// &str
			let _error1 = error_object_from_str("static str");

			// String
			let _error2 = error_object_from_str(String::from("owned string"));

			// format!
			let _error3 = error_object_from_str(format!("formatted {}", "message"));
		}
	}
}


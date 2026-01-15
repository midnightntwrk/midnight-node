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

//! FederatedOps Authority Selection Data Source with Runtime Format Detection
//!
//! This module provides a wrapper around `CandidatesDataSourceImpl` that can parse
//! multiple datum formats for permissioned candidates:
//!
//! 1. **FederatedOps format** (Aiken contracts): Used by `federated_ops_forever` contract
//!    `[data, [[partner_chains_key, [[key_id, key_bytes], ...]], ...], logic_round]`
//!
//! 2. **Partner-chains SDK format**: Legacy format
//!    `[[sidechain_key, aura_key, grandpa_key], ...]`
//!
//! The wrapper automatically detects which format is in use by attempting to parse
//! the FederatedOps format first, falling back to the inner data source if parsing fails.

use authority_selection_inherents::{AriadneParameters, AuthoritySelectionDataSource};
use midnight_primitives_mainchain_follower::MidnightAuthoritySelectionDataSource;
use sidechain_domain::{
	CandidateRegistrations, MainchainAddress, McEpochNumber, PermissionedCandidateData, PolicyId,
};
use sqlx::PgPool;

/// A wrapper around `CandidatesDataSourceImpl` that adds FederatedOps datum parsing
/// with automatic format detection.
///
/// This data source implements `AuthoritySelectionDataSource` by:
/// 1. Attempting to parse permissioned candidates using FederatedOps format
/// 2. Falling back to the inner data source if FederatedOps parsing fails
/// 3. Always delegating D-parameter and other queries to the inner data source
pub struct FederatedOpsAuthoritySelectionDataSource<T> {
	inner: T,
	pool: PgPool,
	/// Optional policy ID override for FederatedOps contracts.
	/// If set, uses this instead of the permissioned_candidates_policy from chain config.
	/// Required for local-env where contracts are dynamically deployed.
	override_policy_id: Option<PolicyId>,
}

impl<T> FederatedOpsAuthoritySelectionDataSource<T> {
	/// Create a new authority selection data source with FederatedOps format detection
	///
	/// # Arguments
	/// * `inner` - The underlying data source for D-parameter and fallback candidate queries
	/// * `pool` - Database connection pool for FederatedOps queries
	/// * `override_policy_id` - Optional policy ID override. If set, uses this for FederatedOps
	///   parsing instead of the permissioned_candidates_policy from chain config.
	pub fn new(inner: T, pool: PgPool, override_policy_id: Option<PolicyId>) -> Self {
		Self { inner, pool, override_policy_id }
	}

	/// Try to get permissioned candidates from the FederatedOps contract.
	/// Returns Ok(Some(candidates)) if found and parsed successfully,
	/// Ok(None) if not found or parsing failed (should fall back to inner).
	async fn try_federated_ops_candidates(
		&self,
		policy_id: &PolicyId,
	) -> Result<Option<Vec<PermissionedCandidateData>>, Box<dyn std::error::Error + Send + Sync>> {
		// Create a temporary data source to query with this policy ID
		let config = midnight_primitives_mainchain_follower::AikenFederatedOpsConfig {
			policy_id: policy_id.clone(),
		};
		let data_source = MidnightAuthoritySelectionDataSource::new((), self.pool.clone(), config);

		// The federated_ops_forever contract uses a static datum - the candidate list doesn't
		// change per epoch (hence "forever"). We query the latest UTxO state using i32::MAX
		// as the block number to avoid overflow when cast to i32 in SQL query.
		match data_source.get_aiken_permissioned_candidates(i32::MAX as u32).await {
			Ok(candidates) if !candidates.is_empty() => {
				let converted =
					MidnightAuthoritySelectionDataSource::<()>::convert_candidates(candidates);
				log::info!("FederatedOps parser found {} permissioned candidates", converted.len());
				Ok(Some(converted))
			},
			Ok(_) => {
				log::debug!("FederatedOps parser found no candidates, will try inner data source");
				Ok(None)
			},
			Err(e) => {
				log::debug!("FederatedOps parsing failed ({}), will try inner data source", e);
				Ok(None)
			},
		}
	}
}

#[async_trait::async_trait]
impl<T> AuthoritySelectionDataSource for FederatedOpsAuthoritySelectionDataSource<T>
where
	T: AuthoritySelectionDataSource + Send + Sync,
{
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
		d_parameter_policy: PolicyId,
		permissioned_candidates_policy: PolicyId,
	) -> Result<AriadneParameters, Box<dyn std::error::Error + Send + Sync>> {
		// Use override policy ID if set (local-env), otherwise try runtime detection
		// with the permissioned_candidates_policy from chain config
		let policy_id_to_try =
			self.override_policy_id.as_ref().unwrap_or(&permissioned_candidates_policy);

		// Try FederatedOps format first
		let federated_ops_candidates = self.try_federated_ops_candidates(policy_id_to_try).await?;

		if let Some(candidates) = federated_ops_candidates {
			// FederatedOps format succeeded - get D-parameter from inner and combine
			let inner_params = self
				.inner
				.get_ariadne_parameters(
					epoch_number,
					d_parameter_policy,
					permissioned_candidates_policy,
				)
				.await?;

			return Ok(AriadneParameters {
				d_parameter: inner_params.d_parameter,
				permissioned_candidates: Some(candidates),
			});
		}

		// Fall back to inner data source for both D-parameter and candidates
		log::debug!(
			"Using inner data source for permissioned candidates (epoch {})",
			epoch_number.0
		);
		self.inner
			.get_ariadne_parameters(
				epoch_number,
				d_parameter_policy,
				permissioned_candidates_policy,
			)
			.await
	}

	async fn get_candidates(
		&self,
		epoch_number: McEpochNumber,
		committee_candidate_address: MainchainAddress,
	) -> Result<Vec<CandidateRegistrations>, Box<dyn std::error::Error + Send + Sync>> {
		self.inner.get_candidates(epoch_number, committee_candidate_address).await
	}

	async fn get_epoch_nonce(
		&self,
		epoch_number: McEpochNumber,
	) -> Result<Option<sidechain_domain::EpochNonce>, Box<dyn std::error::Error + Send + Sync>> {
		self.inner.get_epoch_nonce(epoch_number).await
	}

	async fn data_epoch(
		&self,
		epoch_number: McEpochNumber,
	) -> Result<McEpochNumber, Box<dyn std::error::Error + Send + Sync>> {
		self.inner.data_epoch(epoch_number).await
	}
}

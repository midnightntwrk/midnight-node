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

//! Aiken-aware Authority Selection Data Source
//!
//! This module provides a wrapper around `CandidatesDataSourceImpl` that can parse
//! the Aiken FederatedOps datum format for permissioned candidates.
//!
//! The partner-chains SDK expects permissioned candidate datums in the format:
//! `[[sidechain_key, aura_key, grandpa_key], ...]`
//!
//! But Aiken contracts use:
//! `[data, [[partner_chains_key, [[key_id, key_bytes], ...]], ...], logic_round]`
//!
//! This wrapper intercepts the datum parsing and converts between formats.

use authority_selection_inherents::{AriadneParameters, AuthoritySelectionDataSource};
use midnight_primitives_mainchain_follower::{
	AikenFederatedOpsConfig, MidnightAuthoritySelectionDataSource,
};
use sidechain_domain::{
	CandidateRegistrations, MainchainAddress, McEpochNumber, PermissionedCandidateData, PolicyId,
};
use sqlx::PgPool;

/// A placeholder policy ID used when the actual value is not needed.
/// This is passed to inner data source calls where we only care about D-parameter,
/// not permissioned candidates (which are handled by the Aiken parser).
const UNUSED_POLICY_ID: PolicyId = PolicyId([0u8; 28]);

/// A wrapper around `CandidatesDataSourceImpl` that adds Aiken datum parsing support.
///
/// This data source implements `AuthoritySelectionDataSource` by:
/// 1. Using the Aiken parser for permissioned candidates
/// 2. Delegating D-parameter queries to the inner data source
/// 3. Delegating all other methods to the inner data source
pub struct AikenAuthoritySelectionDataSource<T> {
	inner: T,
	aiken_data_source: MidnightAuthoritySelectionDataSource<()>,
}

impl<T> AikenAuthoritySelectionDataSource<T> {
	/// Create a new Aiken-aware authority selection data source
	pub fn new(inner: T, pool: PgPool, config: AikenFederatedOpsConfig) -> Self {
		Self {
			inner,
			aiken_data_source: MidnightAuthoritySelectionDataSource::new((), pool, config),
		}
	}

	/// Get permissioned candidates from the Aiken FederatedOps contract
	async fn get_aiken_permissioned_candidates(
		&self,
		block_number: u32,
	) -> Result<Vec<PermissionedCandidateData>, Box<dyn std::error::Error + Send + Sync>> {
		let candidates =
			self.aiken_data_source.get_aiken_permissioned_candidates(block_number).await?;

		Ok(MidnightAuthoritySelectionDataSource::<()>::convert_candidates(candidates))
	}
}

#[async_trait::async_trait]
impl<T> AuthoritySelectionDataSource for AikenAuthoritySelectionDataSource<T>
where
	T: AuthoritySelectionDataSource + Send + Sync,
{
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
		d_parameter_policy: PolicyId,
		_permissioned_candidates_policy: PolicyId,
	) -> Result<AriadneParameters, Box<dyn std::error::Error + Send + Sync>> {
		// Use Aiken parser directly for permissioned candidates
		// Note: Use i32::MAX to avoid overflow when cast to i32 in SQL query
		let candidates = self.get_aiken_permissioned_candidates(i32::MAX as u32).await?;

		log::info!(
			"Aiken parser found {} permissioned candidates for epoch {}",
			candidates.len(),
			epoch_number.0
		);

		// Get D parameter from the inner data source
		// Note: D parameter is stored separately from permissioned candidates
		let inner_params = self
			.inner
			.get_ariadne_parameters(epoch_number, d_parameter_policy, UNUSED_POLICY_ID)
			.await?;

		let d_parameter = inner_params.d_parameter;

		Ok(AriadneParameters { d_parameter, permissioned_candidates: Some(candidates) })
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

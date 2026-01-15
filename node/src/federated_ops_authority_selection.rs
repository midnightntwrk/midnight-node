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

//! Local Environment Authority Selection Data Source
//!
//! This module provides a thin wrapper around `CandidatesDataSourceImpl` that allows
//! overriding the permissioned candidates policy ID for local development environments.
//!
//! ## Why This Wrapper Is Needed
//!
//! In production networks, the `permissioned_candidates_policy` is known at compile time
//! and included in the chain specification. However, in local development environments
//! (local-env), the FederatedOps contracts are deployed dynamically, and their policy IDs
//! are not known until after deployment.
//!
//! This wrapper allows injecting the dynamically-deployed policy ID at runtime via
//! the `FEDERATED_OPS_POLICY_ID` environment variable or node configuration.
//!
//! ## Datum Format Compatibility
//!
//! The FederatedOps datum format is structurally compatible with the partner-chains SDK's
//! `VersionedGenericDatum` V1 format. The FederatedOps `logic_round` field occupies the
//! same position as the SDK's `version` field:
//!
//! - When `logic_round=1`, the SDK correctly parses the datum as V1 format
//! - The partner-chains team has agreed to maintain `logic_round=1` for compatibility
//!
//! **Important**: If `logic_round` changes to a value other than 1 in the future,
//! the SDK parsing will fail and this code will need to be updated.

use authority_selection_inherents::{AriadneParameters, AuthoritySelectionDataSource};
use sidechain_domain::{CandidateRegistrations, MainchainAddress, McEpochNumber, PolicyId};

/// A wrapper around `CandidatesDataSourceImpl` that supports policy ID override
/// for local development environments.
///
/// This data source implements `AuthoritySelectionDataSource` by delegating all
/// operations to the inner data source, but optionally overriding the
/// `permissioned_candidates_policy` with a dynamically-configured policy ID.
pub struct LocalEnvAuthoritySelectionDataSource<T> {
	inner: T,
	/// Optional policy ID override for FederatedOps contracts.
	/// If set, uses this instead of the permissioned_candidates_policy from chain config.
	/// Required for local-env where contracts are dynamically deployed.
	override_policy_id: Option<PolicyId>,
}

impl<T> LocalEnvAuthoritySelectionDataSource<T> {
	/// Create a new authority selection data source with optional policy ID override
	///
	/// # Arguments
	/// * `inner` - The underlying data source for all queries
	/// * `override_policy_id` - Optional policy ID override. If set, uses this for
	///   permissioned candidates queries instead of the policy from chain config.
	pub fn new(inner: T, override_policy_id: Option<PolicyId>) -> Self {
		Self { inner, override_policy_id }
	}
}

#[async_trait::async_trait]
impl<T> AuthoritySelectionDataSource for LocalEnvAuthoritySelectionDataSource<T>
where
	T: AuthoritySelectionDataSource + Send + Sync,
{
	async fn get_ariadne_parameters(
		&self,
		epoch_number: McEpochNumber,
		d_parameter_policy: PolicyId,
		permissioned_candidates_policy: PolicyId,
	) -> Result<AriadneParameters, Box<dyn std::error::Error + Send + Sync>> {
		// Use override policy ID if set (local-env), otherwise use the one from chain config
		let effective_policy = self
			.override_policy_id
			.clone()
			.unwrap_or(permissioned_candidates_policy);

		if self.override_policy_id.is_some() {
			log::debug!(
				"Using overridden FederatedOps policy ID for epoch {}",
				epoch_number.0
			);
		}

		self.inner
			.get_ariadne_parameters(epoch_number, d_parameter_policy, effective_policy)
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

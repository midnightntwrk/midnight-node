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

//! Federated Authority Observation Inherent Data Provider

use crate::FederatedAuthorityObservationDataSource;
use midnight_primitives_federated_authority_observation::FederatedAuthorityData;
use std::error::Error;

pub struct FederatedAuthorityInherentDataProvider {
	pub data: FederatedAuthorityData,
}

impl FederatedAuthorityInherentDataProvider {
	pub async fn new<FA>(
		data_source: &(dyn FederatedAuthorityObservationDataSource<FA> + Send + Sync),
		mc_block_hash: &sidechain_domain::McBlockHash,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let data = data_source.get_federated_authority_data(mc_block_hash).await?;
		Ok(Self { data })
	}
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for FederatedAuthorityInherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(
			midnight_primitives_federated_authority_observation::INHERENT_IDENTIFIER,
			&FederatedAuthorityData {
				council_authorities: self.data.council_authorities.clone(),
				technical_committee_authorities: self.data.technical_committee_authorities.clone(),
				mc_block_hash: self.data.mc_block_hash.clone(),
			},
		)
	}

	async fn try_handle_error(
		&self,
		_identifier: &sp_inherents::InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

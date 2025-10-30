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
use midnight_primitives_federated_authority_observation::{
	AuthBodyConfig, FederatedAuthorityData, FederatedAuthorityObservationApi,
	FederatedAuthorityObservationConfig,
};
use sp_api::ProvideRuntimeApi;
use sp_runtime::traits::Block as BlockT;
use std::{error::Error, sync::Arc};

pub struct FederatedAuthorityInherentDataProvider {
	pub data: FederatedAuthorityData,
}

impl FederatedAuthorityInherentDataProvider {
	pub async fn new<Block, C>(
		client: Arc<C>,
		data_source: &(dyn FederatedAuthorityObservationDataSource + Send + Sync),
		parent_hash: <Block as BlockT>::Hash,
		mc_block_hash: &sidechain_domain::McBlockHash,
	) -> Result<Self, Box<dyn Error + Send + Sync>>
	where
		Block: BlockT,
		C: ProvideRuntimeApi<Block> + Send + Sync,
		C::Api: FederatedAuthorityObservationApi<Block>,
	{
		let api = client.runtime_api();

		let council_address = api.get_council_address(parent_hash)?.bytes();
		let council_address = String::from_utf8(council_address)?;

		let council_policy_id = api.get_council_policy_id(parent_hash)?;
		let council_policy_id = hex::encode(council_policy_id.0);

		let technical_committee_address = api.get_technical_committee_address(parent_hash)?.bytes();
		let technical_committee_address = String::from_utf8(technical_committee_address)?;

		let technical_committee_policy_id = api.get_technical_committee_policy_id(parent_hash)?;
		let technical_committee_policy_id = hex::encode(technical_committee_policy_id.0);

		let council = AuthBodyConfig {
			address: council_address,
			policy_id: council_policy_id,
			members: vec![], // TODO: Federated Authority Observation - populate with current set from API
		};

		let technical_committee = AuthBodyConfig {
			address: technical_committee_address,
			policy_id: technical_committee_policy_id,
			members: vec![], // TODO: Federated Authority Observation - populate with current set from API
		};

		let config = FederatedAuthorityObservationConfig { council, technical_committee };

		let data = data_source.get_federated_authority_data(&config, mc_block_hash).await?;

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

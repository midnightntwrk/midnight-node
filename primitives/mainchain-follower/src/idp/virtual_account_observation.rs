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

use crate::VirtualAccountObservationDataSource;
use midnight_primitives_block_rewards::{
	ACCOUNTS_INHERENT_IDENTIFIER, BlockRewardsApi, RewardAccountsData,
};
use sp_api::ProvideRuntimeApi;
use sp_runtime::traits::Block as BlockT;
use std::{error::Error, sync::Arc};

pub struct VirtualAccountInherentDataProvider {
	pub data: Option<RewardAccountsData>,
}

impl VirtualAccountInherentDataProvider {
	pub async fn new<Block, C>(
		client: Arc<C>,
		data_source: &(dyn VirtualAccountObservationDataSource + Send + Sync),
		parent_hash: <Block as BlockT>::Hash,
		mc_block_hash: &sidechain_domain::McBlockHash,
		epoch: u64,
		is_new_epoch: bool,
	) -> Result<Self, Box<dyn Error + Send + Sync>>
	where
		Block: BlockT,
		C: ProvideRuntimeApi<Block> + Send + Sync,
		C::Api: BlockRewardsApi<Block>,
	{
		let data = if is_new_epoch {
			match client.runtime_api().virtual_account_policy(parent_hash)? {
				Some(policy) => Some(RewardAccountsData {
					epoch,
					accounts: data_source.get_reward_accounts(&policy, mc_block_hash).await?,
				}),
				None => None,
			}
		} else {
			None
		};
		Ok(Self { data })
	}
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for VirtualAccountInherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		if let Some(data) = &self.data {
			inherent_data.put_data(ACCOUNTS_INHERENT_IDENTIFIER, data)?;
		}
		Ok(())
	}

	async fn try_handle_error(
		&self,
		_identifier: &sp_inherents::InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

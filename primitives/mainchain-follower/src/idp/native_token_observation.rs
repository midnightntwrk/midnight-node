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

//! Native Token Observation Inherent Data Provider

use crate::{
	MidnightNativeTokenObservationDataSource, MidnightObservationTokenMovement, ObservedUtxo,
};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, INHERENT_IDENTIFIER, InherentError, NativeTokenObservationApi,
};
use parity_scale_codec::Decode;
use sp_api::{ApiError, ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{error::Error, string::FromUtf8Error, sync::Arc};

pub const DEFAULT_CARDANO_BLOCK_WINDOW_SIZE: u32 = 10000;

pub struct MidnightNativeTokenObservationInherentDataProvider {
	pub utxos: Vec<ObservedUtxo>,
	pub next_cardano_position: CardanoPosition,
}

#[derive(thiserror::Error, sp_runtime::RuntimeDebug)]
pub enum IDPCreationError {
	#[error("Failed to read native token data from data source: {0:?}")]
	DataSourceError(Box<dyn Error + Send + Sync>),
	#[error("Failed to read native token data from data source. Db sync may need to be synced")]
	DbSyncDataDiscrepancy,
	#[error("Failed to call runtime API: {0:?}")]
	ApiError(ApiError),
	#[error("Failed to retrieve previous MC hash: {0:?}")]
	McHashError(Box<dyn Error + Send + Sync>),
	#[error("Onchain asset name or policy id likely invalid: {0:?}")]
	InvalidOnchainState(FromUtf8Error),
}

impl From<ApiError> for IDPCreationError {
	fn from(err: ApiError) -> Self {
		Self::ApiError(err)
	}
}

impl From<FromUtf8Error> for IDPCreationError {
	fn from(err: FromUtf8Error) -> Self {
		Self::InvalidOnchainState(err)
	}
}

impl MidnightNativeTokenObservationInherentDataProvider {
	/// Creates inherent data provider only if the pallet is present in the runtime.
	/// Returns empty data if not.
	pub async fn new_if_pallet_present<Block, C>(
		client: Arc<C>,
		data_source: &(dyn MidnightNativeTokenObservationDataSource + Send + Sync),
		parent_hash: <Block as BlockT>::Hash,
		mc_hash: sidechain_domain::McBlockHash,
	) -> Result<Self, IDPCreationError>
	where
		Block: BlockT,
		C: HeaderBackend<Block>,
		C: ProvideRuntimeApi<Block> + Send + Sync,
		C::Api: NativeTokenObservationApi<Block>,
	{
		if let Ok(true) = client
			.runtime_api()
			.has_api::<dyn NativeTokenObservationApi<Block>>(parent_hash)
		{
			Self::new(client, data_source, parent_hash, mc_hash).await
		} else {
			Ok(Self {
				utxos: vec![],
				next_cardano_position: CardanoPosition {
					block_hash: [0; 32],
					block_number: 0,
					tx_index_in_block: 0,
				},
			})
		}
	}

	pub async fn new<Block, C>(
		client: Arc<C>,
		data_source: &(dyn MidnightNativeTokenObservationDataSource + Send + Sync),
		parent_hash: <Block as BlockT>::Hash,
		mc_hash: sidechain_domain::McBlockHash,
	) -> Result<Self, IDPCreationError>
	where
		Block: BlockT,
		C: HeaderBackend<Block>,
		C: ProvideRuntimeApi<Block> + Send + Sync,
		C::Api: NativeTokenObservationApi<Block>,
	{
		let api = client.runtime_api();
		let redemption_validator_address = api.get_redemption_validator_address(parent_hash)?;
		let redemption_validator_address = String::from_utf8(redemption_validator_address)?;

		let mapping_validator_address = api.get_mapping_validator_address(parent_hash)?;
		let mapping_validator_address = String::from_utf8(mapping_validator_address)?;

		let utxo_capacity = api.get_utxo_capacity_per_block(parent_hash)?;

		let (policy_id, asset_name) = api.get_native_token_identifier(parent_hash)?;
		let policy_id = hex::encode(policy_id.clone());
		let asset_name =
			String::from_utf8(asset_name.clone()).map_err(IDPCreationError::InvalidOnchainState)?;

		let cardano_position_start = api.get_next_cardano_position(parent_hash)?;

		let config = CNightAddresses {
			mapping_validator_address,
			redemption_validator_address,
			policy_id,
			asset_name,
		};

		let observed_utxos = data_source
			.get_utxos_up_to_capacity(
				&config,
				cardano_position_start,
				mc_hash,
				utxo_capacity as usize,
			)
			.await
			.map_err(IDPCreationError::DataSourceError)?;

		Ok(Self { utxos: observed_utxos.utxos, next_cardano_position: observed_utxos.end })
	}
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for MidnightNativeTokenObservationInherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(
			INHERENT_IDENTIFIER,
			&MidnightObservationTokenMovement {
				utxos: self.utxos.clone(),
				next_cardano_position: self.next_cardano_position,
			},
		)
	}

	async fn try_handle_error(
		&self,
		identifier: &sp_inherents::InherentIdentifier,
		mut error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		if *identifier != INHERENT_IDENTIFIER {
			return None;
		}

		let error = InherentError::decode(&mut error).ok()?;

		Some(Err(sp_inherents::Error::Application(Box::from(error))))
	}
}

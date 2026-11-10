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

//! Native Token Observation Inherent Data Provider

use crate::{
	MidnightCNightObservationDataSource, MidnightObservationTokenMovement, ObservationBound,
};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CNightObservationApi, CardanoCursor, CardanoPosition, INHERENT_IDENTIFIER,
	InherentError, ObservationBatch, TimestampUnixMillis,
};
use parity_scale_codec::Decode;
use sidechain_domain::McBlockHash;
use sp_api::{ApiError, ApiExt, CallApiAt, CallApiAtParams, CallContext, Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{error::Error, string::FromUtf8Error, sync::Arc};

pub const DEFAULT_CARDANO_BLOCK_WINDOW_SIZE: u32 = 10000;

/// Runtime exports of methods only version 1 of `CNightObservationApi` declares,
/// so the generated client cannot call them.
const UTXO_CAPACITY_PER_BLOCK_V1: &str = "CNightObservationApi_get_utxo_capacity_per_block";
const NEXT_CARDANO_POSITION_V1: &str = "CNightObservationApi_get_next_cardano_position";

fn call_v1<Block: BlockT, C: CallApiAt<Block>, R: Decode>(
	client: &C,
	at: Block::Hash,
	function: &'static str,
) -> Result<R, ApiError> {
	let raw = client.call_api_at(CallApiAtParams {
		at,
		function,
		arguments: Vec::new(),
		overlayed_changes: &Default::default(),
		call_context: CallContext::Offchain,
		recorder: &None,
		extensions: &Default::default(),
	})?;
	let decoded = R::decode(&mut &raw[..]);
	decoded.map_err(|error| ApiError::FailedToDecodeReturnValue { function, error, raw })
}

pub struct MidnightCNightObservationInherentDataProvider {
	pub utxos: ObservationBatch,
	pub next_cardano_position: CardanoPosition,
}

#[derive(thiserror::Error, Debug)]
pub enum IDPCreationError {
	#[error("Failed to read native token data from data source: {0:?}")]
	DataSourceError(Box<dyn Error + Send + Sync>),
	#[error("Failed to read native token data from data source. Db sync may need to be synced")]
	DbSyncDataDiscrepancy,
	#[error("Failed to call runtime API: {0:?}")]
	ApiError(#[from] ApiError),
	#[error("Failed to decode string as UTF8 (check address values)")]
	StringDecodeError(#[from] FromUtf8Error),
	#[error("Failed to retrieve previous MC hash: {0:?}")]
	McHashError(Box<dyn Error + Send + Sync>),
	#[error("Onchain state for CNight invalid: {0:?}")]
	InvalidOnchainStateCNight(String),
	#[error("Auth token asset name is not a string")]
	AuthTokenAssetNameNotString,
}

impl MidnightCNightObservationInherentDataProvider {
	/// Creates inherent data provider only if the pallet is present in the runtime.
	/// Returns empty data if not.
	pub async fn new_if_pallet_present<Block, C>(
		client: Arc<C>,
		data_source: &(dyn MidnightCNightObservationDataSource + Send + Sync),
		parent_hash: <Block as BlockT>::Hash,
		mc_hash: sidechain_domain::McBlockHash,
	) -> Result<Self, IDPCreationError>
	where
		Block: BlockT,
		C: HeaderBackend<Block>,
		C: ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync,
		C::Api: CNightObservationApi<Block>,
	{
		if let Ok(true) =
			client.runtime_api().has_api::<dyn CNightObservationApi<Block>>(parent_hash)
		{
			Self::new(client, data_source, parent_hash, mc_hash).await
		} else {
			Ok(Self {
				utxos: ObservationBatch::default(),
				next_cardano_position: CardanoPosition {
					block_hash: McBlockHash([0; 32]),
					block_number: 0,
					block_timestamp: TimestampUnixMillis(0),
					tx_index_in_block: 0,
				},
			})
		}
	}

	pub async fn new<Block, C>(
		client: Arc<C>,
		data_source: &(dyn MidnightCNightObservationDataSource + Send + Sync),
		parent_hash: <Block as BlockT>::Hash,
		mc_hash: sidechain_domain::McBlockHash,
	) -> Result<Self, IDPCreationError>
	where
		Block: BlockT,
		C: HeaderBackend<Block>,
		C: ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync,
		C::Api: CNightObservationApi<Block>,
	{
		let api = client.runtime_api();
		let mapping_validator_address =
			String::from_utf8(api.get_mapping_validator_address(parent_hash)?)?;
		let (cnight_policy_id, cnight_asset_name) = api.get_cnight_token_identifier(parent_hash)?;
		let auth_token_asset_name: String = api
			.get_auth_token_asset_name(parent_hash)?
			.try_into()
			.map_err(|_| IDPCreationError::AuthTokenAssetNameNotString)?;
		let spec_version = api.version(parent_hash)?.spec_version;

		// On version 1 runtimes the count bound keeps the batch byte-identical to what
		// nodes without the cost model build and check our inherent against.
		let version_2 = api
			.api_version::<dyn CNightObservationApi<Block>>(parent_hash)?
			.is_some_and(|version| version >= 2);
		let (bound, start) = if version_2 {
			let start = api.get_next_cardano_cursor(parent_hash)?;
			match api.get_observation_cost_model(parent_hash)? {
				Some(model) if model.utxo_gas > 0 => (ObservationBound::Cost(model), start),
				// Not the count bound, which can skip rows: an empty batch holding the
				// cursor loses nothing until a runtime upgrade restores pricing.
				model => {
					log::error!(
						target: "cnight-observation",
						"runtime could not price the observation batch ({model:?}); cNIGHT observation halted at {start:?}"
					);
					return Ok(Self {
						utxos: ObservationBatch::default(),
						next_cardano_position: start.position,
					});
				},
			}
		} else {
			let capacity: u32 = call_v1(&*client, parent_hash, UTXO_CAPACITY_PER_BLOCK_V1)?;
			let position = call_v1(&*client, parent_hash, NEXT_CARDANO_POSITION_V1)?;
			(ObservationBound::Count(capacity as usize), CardanoCursor { position, utxo_offset: 0 })
		};
		log::debug!(target: "cnight-observation", "observation bound: {bound:?}");

		let config = CNightAddresses {
			mapping_validator_address,
			auth_token_asset_name,
			cnight_policy_id: cnight_policy_id.try_into().map_err(|_e| {
				IDPCreationError::InvalidOnchainStateCNight("cnight_policy_id".to_string())
			})?,
			cnight_asset_name: cnight_asset_name.try_into().map_err(|_e| {
				IDPCreationError::InvalidOnchainStateCNight("cnight_asset_name".to_string())
			})?,
		};

		let observed_utxos = data_source
			.get_utxos_up_to_capacity(
				&config,
				&start.position,
				start.utxo_offset,
				mc_hash,
				&bound,
				spec_version,
			)
			.await
			.map_err(IDPCreationError::DataSourceError)?;

		Ok(Self { utxos: observed_utxos.utxos, next_cardano_position: observed_utxos.end })
	}
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for MidnightCNightObservationInherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(
			INHERENT_IDENTIFIER,
			&MidnightObservationTokenMovement {
				utxos: self.utxos.clone(),
				next_cardano_position: self.next_cardano_position.clone(),
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

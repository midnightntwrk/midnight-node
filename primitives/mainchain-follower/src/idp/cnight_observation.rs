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

use crate::{MidnightCNightObservationDataSource, MidnightObservationTokenMovement, ObservedUtxo};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CNightObservationApi, CardanoPosition, INHERENT_IDENTIFIER, InherentError,
	TimestampUnixMillis,
};
use parity_scale_codec::Decode;
use sidechain_domain::McBlockHash;
use sp_api::{ApiError, ApiExt, Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{error::Error, string::FromUtf8Error, sync::Arc};

pub const DEFAULT_CARDANO_BLOCK_WINDOW_SIZE: u32 = 10000;

/// Runtime `spec_version` at (and above) which the cNIGHT observation inherent
/// is derived with the **v2** mechanism. Below it, the frozen v1 derivation
/// (`data_source::cnight_observation_v1`) is used so pre-upgrade blocks replay
/// byte-identically on import.
///
/// The switch is gated on `spec_version` (read at `parent_hash`) because that is
/// consensus state every node agrees on, and it bumps exactly at the v2 runtime
/// upgrade. The handover is atomic: the upgrade block itself still derives v1
/// (its parent predates the bump); the first child is the first v2 block.
///
/// PLACEHOLDER `u32::MAX` ⟹ v2 is **disabled** — every block uses v1 — until the
/// v2 runtime upgrade ships and its real `spec_version` is pinned here. This
/// threshold is itself consensus: all nodes must carry the same value.
pub const CNIGHT_OBSERVATION_V2_SPEC_VERSION: u32 = u32::MAX;

pub struct MidnightCNightObservationInherentDataProvider {
	pub utxos: Vec<ObservedUtxo>,
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
	#[error("CNightObservationApi version not reported by runtime")]
	CNightObservationApiUnavailable,
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
		C: ProvideRuntimeApi<Block> + Send + Sync,
		C::Api: CNightObservationApi<Block> + Core<Block>,
	{
		if let Ok(true) =
			client.runtime_api().has_api::<dyn CNightObservationApi<Block>>(parent_hash)
		{
			Self::new(client, data_source, parent_hash, mc_hash).await
		} else {
			Ok(Self {
				utxos: vec![],
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
		C: ProvideRuntimeApi<Block> + Send + Sync,
		C::Api: CNightObservationApi<Block> + Core<Block>,
	{
		let api = client.runtime_api();
		let mapping_validator_address =
			String::from_utf8(api.get_mapping_validator_address(parent_hash)?)?;
		let tx_capacity = api.get_utxo_capacity_per_block(parent_hash)?;

		let (cnight_policy_id, cnight_asset_name) = api.get_cnight_token_identifier(parent_hash)?;
		let auth_token_asset_name: String = api
			.get_auth_token_asset_name(parent_hash)?
			.try_into()
			.map_err(|_| IDPCreationError::AuthTokenAssetNameNotString)?;
		let cardano_position_start = api.get_next_cardano_position(parent_hash)?;

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

		// Consensus gate: pick the derivation by the parent's runtime
		// `spec_version`. Below the v2 activation, run the frozen v1 derivation
		// so pre-upgrade blocks replay byte-identically; at/after it, the v2
		// (bulk-cached, one-block) path. Both `create_inherent` (author) and
		// `check_inherent` (importer) reach this through the same parent state,
		// so they never disagree on which version to use.
		let spec_version = api.version(parent_hash)?.spec_version;
		let observed_utxos = if spec_version < CNIGHT_OBSERVATION_V2_SPEC_VERSION {
			data_source
				.get_utxos_v1(&config, &cardano_position_start, mc_hash, tx_capacity as usize)
				.await
		} else {
			// UTXO acceptance envelope = tx_capacity * multiplier, with *both* factors
			// read from the runtime at `parent_hash` (same as `tx_capacity`), so the
			// author's truncation cap always equals the runtime's `process_tokens`
			// bound — across upgrades, not just within one build. `get_utxo_per_tx_overestimate`
			// is a v3 `CNightObservationApi` method that ships in the same runtime as
			// the v2 activation `spec_version`, so this branch only ever calls it
			// against a runtime that has it.
			let multiplier = api.get_utxo_per_tx_overestimate(parent_hash)?;
			let max_utxos = tx_capacity.saturating_mul(multiplier) as usize;
			data_source
				.get_utxos_v2(
					&config,
					&cardano_position_start,
					mc_hash,
					tx_capacity as usize,
					max_utxos,
				)
				.await
		}
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

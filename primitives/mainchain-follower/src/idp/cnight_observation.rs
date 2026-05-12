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
	MidnightCNightObservationDataSource, MidnightObservationTokenMovement, ObservedUtxo,
	ObservedUtxoData,
};
use midnight_node_ledger::latest::api::dust_public_key_is_valid;
use midnight_primitives_cnight_observation::{
	CNightAddresses, CNightObservationApi, CardanoPosition, INHERENT_IDENTIFIER, InherentError,
	TimestampUnixMillis,
};
use parity_scale_codec::Decode;
use sidechain_domain::McBlockHash;
use sp_api::{ApiError, ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{error::Error, string::FromUtf8Error, sync::Arc};

pub const DEFAULT_CARDANO_BLOCK_WINDOW_SIZE: u32 = 10000;

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
		C::Api: CNightObservationApi<Block>,
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
		C::Api: CNightObservationApi<Block>,
	{
		let api = client.runtime_api();
		let mapping_validator_address =
			String::from_utf8(api.get_mapping_validator_address(parent_hash)?)?;
		let tx_capacity = api.get_utxo_capacity_per_block(parent_hash)?;

		// The over-fetch quantity used when querying db-sync is consensus-affecting:
		// validators must agree on it to produce identical inherents. The reduction
		// from 64x to 4x is therefore gated on the on-chain `CNightObservationApi`
		// version: v2+ runtimes use the new factor, older runtimes keep the legacy
		// 64x used by node binaries that shipped against v1.
		let api_version = api
			.api_version::<dyn CNightObservationApi<Block>>(parent_hash)?
			.ok_or(IDPCreationError::CNightObservationApiUnavailable)?;
		let overestimate_factor: u32 = if api_version >= 2 { 4 } else { 64 };
		let utxo_overestimate = tx_capacity.saturating_mul(overestimate_factor);

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

		let observed_utxos = data_source
			.get_utxos_up_to_capacity(
				&config,
				&cardano_position_start,
				mc_hash,
				tx_capacity as usize,
				utxo_overestimate as usize,
			)
			.await
			.map_err(IDPCreationError::DataSourceError)?;

		// At CNightObservationApi v3+, drop registration UTXOs whose DustPublicKey
		// payload is structurally valid (length ≤ 33) but out of range for the
		// Bls12-381 Fr scalar field. The check is consensus-affecting because the
		// inherent payload changes, so it is gated on the runtime API version —
		// matching the v1 → v2 over-fetch-factor gate above. At v2 and earlier the
		// legacy pass-through is preserved so old-runtime + new-binary pairings
		// stay consensus-equivalent across the upgrade window.
		let mut utxos = observed_utxos.utxos;
		filter_invalid_dust_public_key_registrations(&mut utxos, api_version);

		Ok(Self { utxos, next_cardano_position: observed_utxos.end })
	}
}

/// Drops registration UTXOs whose DustPublicKey payload is out of range for the
/// Bls12-381 Fr scalar field, gated on `api_version >= 3`.
///
/// Extracted as a free function so the consensus-affecting filter can be unit-
/// tested without instantiating a full `ProvideRuntimeApi<Block>` mock.
/// Non-registration UTXOs (AssetCreate, AssetSpend, Deregistration) always
/// pass through unchanged. Ordering is preserved via `Vec::retain`.
fn filter_invalid_dust_public_key_registrations(utxos: &mut Vec<ObservedUtxo>, api_version: u32) {
	if api_version < 3 {
		return;
	}
	utxos.retain(|utxo| match &utxo.data {
		ObservedUtxoData::Registration(reg) => {
			let valid = dust_public_key_is_valid(&reg.dust_public_key.0);
			if !valid {
				log::debug!(
					"Dropping registration with out-of-Fr-range DustPublicKey: \
					 cardano_reward_address={} dust_public_key_bytes={}",
					hex::encode(reg.cardano_reward_address.0),
					hex::encode(reg.dust_public_key.0.as_slice()),
				);
			}
			valid
		},
		_ => true,
	});
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

#[cfg(test)]
mod tests {
	//! Unit tests for the IDP-level DustPublicKey filter. The filter logic is
	//! factored out as a free function so these tests do not need a full
	//! `ProvideRuntimeApi<Block>` mock — the runtime-API-version gate is the
	//! only parameter that matters for the filter's decision.
	//!
	//! The "valid" fixture takes the canonical path the production code uses:
	//! the zero scalar is in Fr and round-trips through
	//! `<DustPublicKey as Deserializable>::deserialize` cleanly. The fixture
	//! is asserted against the production validator so any future encoding
	//! change surfaces here as a loud test failure rather than silently

	use super::filter_invalid_dust_public_key_registrations;
	use crate::{ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, UtxoIndexInTx};
	use midnight_node_ledger::latest::api::dust_public_key_is_valid;
	use midnight_primitives_cnight_observation::{
		CARDANO_REWARD_ADDRESS_LENGTH, CardanoPosition, CardanoRewardAddressBytes, CreateData,
		DustPublicKeyBytes, RegistrationData, SpendData, TimestampUnixMillis,
	};
	use sidechain_domain::{McBlockHash, McTxHash};

	fn header(index: u16) -> ObservedUtxoHeader {
		ObservedUtxoHeader {
			tx_position: CardanoPosition {
				block_number: 1,
				block_hash: McBlockHash([0u8; 32]),
				block_timestamp: TimestampUnixMillis(0),
				tx_index_in_block: index as u32,
			},
			tx_hash: McTxHash([index as u8; 32]),
			utxo_tx_hash: McTxHash([index as u8; 32]),
			utxo_index: UtxoIndexInTx(index),
		}
	}

	fn registration(idx: u16, dust_public_key: DustPublicKeyBytes) -> ObservedUtxo {
		ObservedUtxo {
			header: header(idx),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_reward_address: CardanoRewardAddressBytes(
					[idx as u8; CARDANO_REWARD_ADDRESS_LENGTH],
				),
				dust_public_key,
			}),
		}
	}

	fn asset_create(idx: u16) -> ObservedUtxo {
		ObservedUtxo {
			header: header(idx),
			data: ObservedUtxoData::AssetCreate(CreateData {
				value: 100,
				owner: CardanoRewardAddressBytes([idx as u8; CARDANO_REWARD_ADDRESS_LENGTH]),
				utxo_tx_hash: McTxHash([idx as u8; 32]),
				utxo_tx_index: idx,
			}),
		}
	}

	fn asset_spend(idx: u16) -> ObservedUtxo {
		ObservedUtxo {
			header: header(idx),
			data: ObservedUtxoData::AssetSpend(SpendData {
				value: 100,
				owner: CardanoRewardAddressBytes([idx as u8; CARDANO_REWARD_ADDRESS_LENGTH]),
				utxo_tx_hash: McTxHash([idx as u8; 32]),
				utxo_tx_index: idx,
				spending_tx_hash: McTxHash([0xaa; 32]),
			}),
		}
	}

	/// 33-byte vector with leading byte 0xff — value above the Bls12-381 Fr
	/// modulus, so `dust_public_key_is_valid` rejects it.
	fn invalid_dust_public_key() -> DustPublicKeyBytes {
		let bytes = vec![0xffu8; 33];
		debug_assert!(!dust_public_key_is_valid(&bytes));
		DustPublicKeyBytes(bytes.try_into().unwrap())
	}

	/// Valid DustPublicKey byte vector. The zero scalar is in Fr and rounds
	/// through `<DustPublicKey as Deserializable>::deserialize` cleanly; the
	/// helper asserts the fixture is accepted by the production validator so
	/// any future encoding change surfaces here as a loud test failure.
	fn valid_dust_public_key() -> DustPublicKeyBytes {
		let bytes = vec![0u8; 32];
		assert!(
			dust_public_key_is_valid(&bytes),
			"fixture must be a valid DustPublicKey — update encoding if this regresses"
		);
		DustPublicKeyBytes(bytes.try_into().unwrap())
	}

	#[test]
	fn filter_passes_through_at_api_version_2() {
		let mut utxos = vec![
			registration(1, valid_dust_public_key()),
			registration(2, invalid_dust_public_key()),
			asset_create(3),
		];
		let before_len = utxos.len();
		filter_invalid_dust_public_key_registrations(&mut utxos, 2);
		assert_eq!(utxos.len(), before_len, "v2 must preserve legacy pass-through");
	}

	#[test]
	fn filter_drops_invalid_registration_at_api_version_3() {
		let mut utxos = vec![
			registration(1, valid_dust_public_key()),
			registration(2, invalid_dust_public_key()),
			registration(3, valid_dust_public_key()),
		];
		filter_invalid_dust_public_key_registrations(&mut utxos, 3);
		assert_eq!(utxos.len(), 2, "invalid registration must be dropped");
		assert!(matches!(utxos[0].data, ObservedUtxoData::Registration(_)));
		assert!(matches!(utxos[1].data, ObservedUtxoData::Registration(_)));
	}

	#[test]
	fn filter_is_per_utxo_and_variant_scoped() {
		let mut utxos =
			vec![asset_spend(1), registration(2, invalid_dust_public_key()), asset_create(3)];
		filter_invalid_dust_public_key_registrations(&mut utxos, 3);
		assert_eq!(utxos.len(), 2, "only the registration must be dropped");
		assert!(matches!(utxos[0].data, ObservedUtxoData::AssetSpend(_)));
		assert!(matches!(utxos[1].data, ObservedUtxoData::AssetCreate(_)));
	}

	#[test]
	fn filter_preserves_ordering() {
		let mut utxos = vec![
			registration(1, valid_dust_public_key()),
			registration(2, valid_dust_public_key()),
			registration(3, valid_dust_public_key()),
			registration(4, invalid_dust_public_key()),
			registration(5, valid_dust_public_key()),
			registration(6, valid_dust_public_key()),
			registration(7, valid_dust_public_key()),
		];
		filter_invalid_dust_public_key_registrations(&mut utxos, 3);
		assert_eq!(utxos.len(), 6, "exactly one registration must be dropped");
		// Original positions 1..3 and 5..7 — confirm the cardano reward
		// address byte (set to the index in the helper) still increases
		// monotonically with one gap at the removed entry.
		let mut last: i32 = -1;
		for utxo in &utxos {
			if let ObservedUtxoData::Registration(reg) = &utxo.data {
				let idx = reg.cardano_reward_address.0[0] as i32;
				assert!(idx > last, "ordering not preserved");
				last = idx;
			}
		}
	}
}

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

#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(feature = "std")]
pub use inherent_provider::*;

use parity_scale_codec::{Decode, Encode};
use sidechain_domain::*;
use sp_inherents::*;
use sp_runtime::{scale_info::TypeInfo, traits::Block as BlockT};

#[cfg(feature = "std")]
use cardano_serialization_lib::Address;

#[cfg(feature = "std")]
use {
	crate::MidnightNativeTokenObservationDataSource,
	midnight_primitives_native_token_management::NativeTokenObservationApi,
	std::string::FromUtf8Error,
};

#[cfg(feature = "std")]
use crate::db_model::PolicyUtxoRow;

pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"ntobsrve";

#[derive(Decode, Encode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct MidnightRuntimeUtxoRepresentation {
	quantity: u64,
	pub holder_address: Vec<u8>,
}

impl MidnightRuntimeUtxoRepresentation {
	pub fn new(quantity: u64, holder_address: Vec<u8>) -> MidnightRuntimeUtxoRepresentation {
		MidnightRuntimeUtxoRepresentation { quantity, holder_address }
	}
}

#[derive(Decode, Encode, Clone)]
pub struct MidnightObservationTokenMovement {
	// Plutus data bytes, without std types, for onchain consumption
	pub new_registrations: Vec<(Vec<u8>, Vec<u8>)>,
	pub registrations_to_remove: Vec<(Vec<u8>, Vec<u8>)>,
	pub utxos: Vec<MidnightRuntimeUtxoRepresentation>,
	pub latest_block: i64,
}

#[derive(Encode, Debug, PartialEq)]
#[cfg_attr(feature = "std", derive(Decode, thiserror::Error))]
pub enum InherentError {
	#[cfg_attr(feature = "std", error("Unexpected error"))]
	UnexpectedTokenObserveInherent(Option<Vec<Vec<u8>>>, Option<Vec<Vec<u8>>>),
	#[cfg_attr(feature = "std", error("Other unexpected inherent error"))]
	Other,
}

impl IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}

// Removes all duplicate elements, independent of order. This removes original elements, if they are duplicated. The last point is what differentiates this from a typical
// de-duplicating method
#[cfg(feature = "std")]
pub mod inherent_provider {
	use super::*;
	use sp_api::{ApiError, Core, ProvideRuntimeApi, RuntimeApiInfo};
	use sp_blockchain::HeaderBackend;
	use std::{error::Error, sync::Arc};

	pub struct MidnightNativeTokenManagementInherentDataProvider {
		pub new_registrations: Vec<(Vec<u8>, Vec<u8>)>,
		pub invalid_registrations: Vec<(Vec<u8>, Vec<u8>)>,
		pub registrations_to_remove: Vec<(Vec<u8>, Vec<u8>)>,
		pub currency_utxos_data: Vec<PolicyUtxoRow>,
		pub latest_block: i64,
	}

	#[derive(thiserror::Error, sp_runtime::RuntimeDebug)]
	pub enum IDPCreationError {
		#[error("Failed to read native token data from data source: {0:?}")]
		DataSourceError(Box<dyn Error + Send + Sync>),
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

	impl MidnightNativeTokenManagementInherentDataProvider {
		/// Creates inherent data provider only if the pallet is present in the runtime.
		/// Returns empty data if not.
		pub async fn new_if_pallet_present<Block, C>(
			client: Arc<C>,
			data_source: &(dyn MidnightNativeTokenObservationDataSource + Send + Sync),
			parent_hash: <Block as BlockT>::Hash,
		) -> Result<Self, IDPCreationError>
		where
			Block: BlockT,
			C: HeaderBackend<Block>,
			C: ProvideRuntimeApi<Block> + Send + Sync,
			C::Api: NativeTokenObservationApi<Block>,
		{
			let version = client.runtime_api().version(parent_hash)?;

			if version.has_api_with(&<dyn NativeTokenObservationApi<Block>>::ID, |_| true) {
				Self::new(client, data_source, parent_hash).await
			} else {
				Ok(Self {
					new_registrations: vec![],
					invalid_registrations: vec![],
					currency_utxos_data: vec![],
					registrations_to_remove: vec![],
					latest_block: 0,
				})
			}
		}

		pub async fn new<Block, C>(
			client: Arc<C>,
			data_source: &(dyn MidnightNativeTokenObservationDataSource + Send + Sync),
			parent_hash: <Block as BlockT>::Hash,
		) -> Result<Self, IDPCreationError>
		where
			Block: BlockT,
			C: HeaderBackend<Block>,
			C: ProvideRuntimeApi<Block> + Send + Sync,
			C::Api: NativeTokenObservationApi<Block>,
		{
			let api = client.runtime_api();
			let registrants_list_contract = api.get_mapping_validator_address(parent_hash)?;
			let registrants_list_contract = String::from_utf8(registrants_list_contract)?;

			let (block_min, block_max) = api.get_next_block_range(parent_hash)?;
			let incoming_registration_utxos = data_source
				.get_night_generates_dust_registrants_datum(
					&registrants_list_contract,
					block_min,
					block_max,
				)
				.await
				.map_err(IDPCreationError::DataSourceError)?;

			//  New invalid registrations which must be flagged to users
			let new_invalid_registrations = vec![];
			let registrations_to_remove = vec![];

			let (policy_id, asset_name) = api.get_native_token_identifier(parent_hash)?;

			let policy_id = hex::encode(policy_id.clone());

			let asset_name = String::from_utf8(asset_name.clone())
				.map_err(IDPCreationError::InvalidOnchainState)?;

			let currency_utxos_data = data_source
				.get_spends_for_asset_in_block_range(&policy_id, &asset_name, block_min, block_max)
				.await
				.map_err(IDPCreationError::DataSourceError)?;

			// Get latest safe block
			let latest_block = data_source
				.get_latest_block_no()
				.await
				.map_err(IDPCreationError::DataSourceError)?;

			Ok(Self {
				// All original incoming utxos
				new_registrations: incoming_registration_utxos,
				invalid_registrations: new_invalid_registrations,
				currency_utxos_data,
				registrations_to_remove,
				latest_block,
			})
		}
	}

	#[async_trait::async_trait]
	impl InherentDataProvider for MidnightNativeTokenManagementInherentDataProvider {
		async fn provide_inherent_data(
			&self,
			inherent_data: &mut InherentData,
		) -> Result<(), sp_inherents::Error> {
			let onchain_utxo_representations: Vec<MidnightRuntimeUtxoRepresentation> = self
				.currency_utxos_data
				.iter()
				.map(|utxo| {
					let addr = Address::from_bech32(&utxo.holder_address).unwrap();
					let holder_address =
						addr.payment_cred().unwrap().to_keyhash().unwrap().to_bytes();
					Ok(MidnightRuntimeUtxoRepresentation {
						quantity: utxo
							.quantity
							.try_into()
							.map_err(|e| sp_inherents::Error::Application(Box::new(e)))?,
						holder_address,
					})
				})
				.collect::<Result<_, sp_inherents::Error>>()?;

			inherent_data.put_data(
				INHERENT_IDENTIFIER,
				&MidnightObservationTokenMovement {
					new_registrations: self.new_registrations.clone(),
					registrations_to_remove: self.registrations_to_remove.clone(),
					utxos: onchain_utxo_representations,
					latest_block: self.latest_block,
				},
			)
		}

		async fn try_handle_error(
			&self,
			identifier: &InherentIdentifier,
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
	pub mod mock {}
}

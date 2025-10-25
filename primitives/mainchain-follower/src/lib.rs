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

pub mod idp;

pub use midnight_primitives_cnight_observation::{
	CreateData, DeregistrationData, MidnightObservationTokenMovement, ObservedUtxo,
	ObservedUtxoData, ObservedUtxoHeader, RedemptionCreateData, RedemptionSpendData,
	RegistrationData, SpendData, UtxoIndexInTx,
};

#[cfg(feature = "std")]
pub mod db;

#[cfg(feature = "std")]
pub mod data_source;

#[cfg(feature = "std")]
pub use {
	data_source::{
		CNightObservationDataSourceMock, FederatedAuthorityObservationDataSourceImpl,
		FederatedAuthorityObservationDataSourceMock, MidnightCNightObservationDataSourceImpl,
	},
	inherent_provider::*,
	partner_chains_db_sync_data_sources,
	sp_std::boxed::Box,
};

#[cfg(feature = "std")]
pub mod inherent_provider {
	use super::*;
	use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
	use midnight_primitives_federated_authority_observation::FederatedAuthorityData;
	use sidechain_domain::McBlockHash;

	#[async_trait::async_trait]
	// Simple wrapper trait for native token observation
	pub trait MidnightCNightObservationDataSource {
		// TODO: Change the error type to something explicit
		async fn get_utxos_up_to_capacity(
			&self,
			config: &CNightAddresses,
			start_position: CardanoPosition,
			current_tip: McBlockHash,
			capacity: usize,
		) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>>;
	}

	#[async_trait::async_trait]
	pub trait FederatedAuthorityObservationDataSource<FA = ()>: Send + Sync {
		async fn get_federated_authority_data(
			&self,
			mc_block_hash: &McBlockHash,
		) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>>;
	}

	#[derive(Clone, Debug)]
	// Extended mainchain scripts
	pub struct MidnightMainChainScripts {
		pub registrants_list_contract: String,
	}
}

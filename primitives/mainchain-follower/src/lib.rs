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

#![cfg_attr(not(feature = "std"), no_std)]

pub mod idp;

pub use midnight_primitives_cnight_observation::{
	CreateData, DeregistrationData, MidnightObservationTokenMovement, ObservedUtxo,
	ObservedUtxoData, ObservedUtxoHeader, RegistrationData, SpendData, UtxoIndexInTx,
};

#[cfg(feature = "std")]
pub mod db;

#[cfg(feature = "std")]
pub mod data_source;

#[cfg(feature = "std")]
pub use {
	data_source::{
		CNightObservationDataSourceMock, CandidateDataSourceCached, CandidatesDataSourceImpl,
		FederatedAuthorityObservationDataSourceImpl, FederatedAuthorityObservationDataSourceMock,
		MidnightCNightObservationDataSourceImpl, get_epoch_for_block_hash,
		metrics::MidnightDataSourceMetrics,
	},
	inherent_provider::*,
	partner_chains_db_sync_data_sources,
};

#[cfg(feature = "std")]
pub mod inherent_provider {
	use midnight_primitives_cnight_observation::{
		CNightAddresses, CardanoPosition, ObservationCostModel, ObservedUtxos,
	};
	use midnight_primitives_federated_authority_observation::{
		FederatedAuthorityData, FederatedAuthorityObservationConfig,
	};
	use sidechain_domain::McBlockHash;

	/// How one block's cNIGHT observation batch is bounded.
	#[derive(Clone, Debug, PartialEq, Eq)]
	pub enum ObservationBound {
		/// Fewer than this many Cardano transactions, with a fixed row cap per UTXO kind.
		/// For API version 1 runtimes and genesis construction; must build the same
		/// batch as nodes without the cost model, off-by-one included.
		Count(usize),
		/// Pessimistic price within the runtime's budget. `utxo_gas` must be positive:
		/// it sets the fetch row cap.
		Cost(ObservationCostModel),
	}

	#[async_trait::async_trait]
	// Simple wrapper trait for native token observation
	pub trait MidnightCNightObservationDataSource {
		// TODO: Change the error type to something explicit
		/// Observed UTXOs from `start_position` towards `current_tip`, cut by `bound` at
		/// a Cardano-transaction boundary.
		async fn get_utxos_up_to_capacity(
			&self,
			config: &CNightAddresses,
			start_position: &CardanoPosition,
			current_tip: McBlockHash,
			bound: &ObservationBound,
			spec_version: u32,
		) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>>;
	}

	#[async_trait::async_trait]
	pub trait FederatedAuthorityObservationDataSource {
		async fn get_federated_authority_data(
			&self,
			config: &FederatedAuthorityObservationConfig,
			mc_block_hash: &McBlockHash,
		) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>>;
	}

	#[derive(Clone, Debug)]
	// Extended mainchain scripts
	pub struct MidnightMainChainScripts {
		pub registrants_list_contract: String,
	}
}

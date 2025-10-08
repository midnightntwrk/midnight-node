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
use midnight_primitives_federated_authority_observation::{
	AuthorityMemberPublicKey, FederatedAuthorityData,
};
use midnight_primitives_native_token_observation::TokenObservationConfig;

use super::{ObservedUtxoData, ObservedUtxos, RegistrationData};
use crate::data_source::UtxoIndexInTx;
#[cfg(feature = "std")]
use crate::{
	FederatedAuthoritySelectionDataSource, MidnightNativeTokenObservationDataSource,
	data_source::{ObservedUtxo, ObservedUtxoHeader},
};
#[cfg(feature = "std")]
use {
	async_trait::async_trait,
	midnight_primitives_native_token_observation::CardanoPosition,
	sidechain_domain::{McBlockHash, McTxHash},
};

pub struct NativeTokenObservationDataSourceMock;

impl Default for NativeTokenObservationDataSourceMock {
	fn default() -> Self {
		Self::new()
	}
}

impl NativeTokenObservationDataSourceMock {
	pub fn new() -> Self {
		Self
	}
}

// Mock datum of expected registered user json datum
pub fn mock_utxos(start: &CardanoPosition) -> Vec<ObservedUtxo> {
	vec![ObservedUtxo {
		header: ObservedUtxoHeader {
			tx_position: CardanoPosition {
				block_number: start.block_number,
				block_hash: start.block_hash,
				tx_index_in_block: 1,
			},
			tx_hash: McTxHash(rand::random()),
			utxo_tx_hash: McTxHash(rand::random()),
			utxo_index: UtxoIndexInTx(1),
		},
		data: ObservedUtxoData::Registration(RegistrationData {
			cardano_address: rand::random::<[u8; 32]>().to_vec(),
			dust_address: rand::random::<[u8; 32]>().to_vec(),
		}),
	}]
}

#[async_trait]
impl MidnightNativeTokenObservationDataSource for NativeTokenObservationDataSourceMock {
	async fn get_utxos_up_to_capacity(
		&self,
		_config: &TokenObservationConfig,
		start: CardanoPosition,
		_current_tip: McBlockHash,
		_capacity: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		let mut end = start;
		end.block_number += 1;
		end.block_hash = rand::random();

		let utxos =
			if start.block_number.is_multiple_of(5) { mock_utxos(&start) } else { Vec::new() };

		Ok(ObservedUtxos { start, end, utxos })
	}
}

/// TODO: federated-authority-observation
/// Mock data source that returns empty authority list
#[derive(Clone, Debug, Default)]
pub struct FederatedAuthoritySelectionDataSourceMock;

impl FederatedAuthoritySelectionDataSourceMock {
	pub fn new() -> Self {
		Self
	}
}

use sp_core::sr25519::Public;
use sp_keyring::Sr25519Keyring;

#[async_trait::async_trait]
impl FederatedAuthoritySelectionDataSource for FederatedAuthoritySelectionDataSourceMock {
	async fn get_federated_authority_data(
		&self,
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
		// TODO: federated-authority-observation
		// Return placeholder data with empty authorities list
		let alice_public: Public = Sr25519Keyring::Alice.public();
		let alice = AuthorityMemberPublicKey(alice_public.0.to_vec());

		Ok(FederatedAuthorityData {
			council_authorities: vec![alice],
			technical_committee_authorities: vec![],
			mc_block_hash: mc_block_hash.clone(),
		})
	}
}

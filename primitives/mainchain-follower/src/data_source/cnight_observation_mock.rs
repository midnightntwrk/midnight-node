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

use crate::{
	MidnightCNightObservationDataSource, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
	RegistrationData, UtxoIndexInTx,
};
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use sidechain_domain::{McBlockHash, McTxHash};

pub struct CNightObservationDataSourceMock;

impl Default for CNightObservationDataSourceMock {
	fn default() -> Self {
		Self::new()
	}
}

impl CNightObservationDataSourceMock {
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
				block_timestamp: start.block_timestamp,
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

#[async_trait::async_trait]
impl MidnightCNightObservationDataSource for CNightObservationDataSourceMock {
	async fn get_utxos_up_to_capacity(
		&self,
		_config: &CNightAddresses,
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

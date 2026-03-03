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

use crate::{
	MidnightCNightObservationDataSource, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
	RegistrationData, UtxoIndexInTx,
};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, CardanoRewardAddressBytes, ObservedUtxos,
};
use rand::rngs::StdRng;
use rand::seq::IteratorRandom;
use rand::{Rng, SeedableRng};
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

// Generates deterministic bytes from a seed to ensure all nodes produce identical inherent data.
// Uses block number as seed so hashes vary across blocks but remain consistent across nodes,
// preventing block verification failures due to inherent data mismatches.
fn bytes_from_seed<const N: usize>(seed: u64, offset: u8) -> [u8; N] {
	rng_from_seed(seed, offset).random()
}

fn rng_from_seed(seed: u64, offset: u8) -> StdRng {
	StdRng::seed_from_u64(seed.wrapping_add(offset as u64))
}

// Mock datum of expected registered user json datum
pub fn mock_utxos(start: &CardanoPosition) -> Vec<ObservedUtxo> {
	let seed = u64::from(start.block_number);

	let dust_pk: [u8; 33] = bytes_from_seed(seed, 0);
	let mut rng = rng_from_seed(seed, 1);

	let (dust_pk, _) = dust_pk.split_at((0..33).choose(&mut rng).unwrap());

	vec![ObservedUtxo {
		header: ObservedUtxoHeader {
			tx_position: CardanoPosition {
				block_number: start.block_number,
				block_hash: start.block_hash.clone(),
				block_timestamp: start.block_timestamp,
				tx_index_in_block: 1,
			},
			tx_hash: McTxHash(bytes_from_seed(seed, 1)),
			utxo_tx_hash: McTxHash(bytes_from_seed(seed, 2)),
			utxo_index: UtxoIndexInTx(1),
		},
		data: ObservedUtxoData::Registration(RegistrationData {
			cardano_reward_address: CardanoRewardAddressBytes(bytes_from_seed(seed, 3)),
			dust_public_key: dust_pk.try_into().unwrap(),
		}),
	}]
}

#[async_trait::async_trait]
impl MidnightCNightObservationDataSource for CNightObservationDataSourceMock {
	async fn get_utxos_up_to_capacity(
		&self,
		_config: &CNightAddresses,
		start: &CardanoPosition,
		_current_tip: McBlockHash,
		_capacity: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		let mut end = start.clone();
		end.block_number += 1;
		end.block_hash = McBlockHash(bytes_from_seed(u64::from(start.block_number), 10));

		let utxos =
			if start.block_number.is_multiple_of(5) { mock_utxos(start) } else { Vec::new() };

		Ok(ObservedUtxos { start: start.clone(), end, utxos })
	}
}

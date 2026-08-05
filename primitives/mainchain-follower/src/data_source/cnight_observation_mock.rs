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
	CNightAddresses, CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, ObservedUtxos,
};
use sidechain_domain::{McBlockHash, McTxHash};

/// Mock data source for CNight observations in development mode.
///
/// This mock ensures all nodes in a multi-node network return identical data,
/// preventing inherent data mismatches that would cause block verification failures.
///
/// # Multi-node Compatibility
///
/// In a multi-node setup, each node independently generates inherent data when
/// producing blocks and verifies inherent data when importing blocks from peers.
/// If nodes generate different data, block verification fails with inherent mismatch.
///
/// This mock solves this by using the block number as a seed to generate
/// deterministic "random" data - all nodes with the same block number will
/// generate identical UTXOs.
pub struct CNightObservationDataSourceMock;

impl Default for CNightObservationDataSourceMock {
	fn default() -> Self {
		Self::new()
	}
}

impl CNightObservationDataSourceMock {
	/// Create a new mock that returns deterministic UTXOs based on block number
	pub fn new() -> Self {
		Self
	}

	/// Generate a deterministic hash based on block number
	/// All nodes with the same block number will generate the same hash
	fn deterministic_hash(block_number: u32, salt: u8) -> [u8; 32] {
		let mut hash = [0u8; 32];
		// Use block number and salt to create a deterministic but unique hash
		hash[0..4].copy_from_slice(&block_number.to_le_bytes());
		hash[4] = salt;
		// Fill rest with a deterministic pattern based on block number
		for (i, byte) in hash.iter_mut().enumerate().skip(5) {
			*byte = ((block_number as u64 * (i as u64 + 1) * 31) % 256) as u8;
		}
		hash
	}

	/// Generate deterministic mock UTXOs based on block number
	fn deterministic_utxos(start: &CardanoPosition) -> Vec<ObservedUtxo> {
		let block_num = start.block_number;

		// Generate deterministic values based on block number
		let tx_hash = Self::deterministic_hash(block_num, 1);
		let utxo_tx_hash = Self::deterministic_hash(block_num, 2);

		// Deterministic reward address (29 bytes)
		let mut reward_addr = [0u8; 29];
		for (i, byte) in reward_addr.iter_mut().enumerate() {
			*byte = ((block_num as u64 * (i as u64 + 7) * 17) % 256) as u8;
		}

		// Deterministic dust public key (33 bytes compressed)
		let mut dust_pk = [0u8; 33];
		dust_pk[0] = 0x02; // Compressed public key prefix
		for (i, byte) in dust_pk.iter_mut().enumerate().skip(1) {
			*byte = ((block_num as u64 * (i as u64 + 3) * 23) % 256) as u8;
		}

		vec![ObservedUtxo {
			header: ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_number: block_num,
					block_hash: start.block_hash.clone(),
					block_timestamp: start.block_timestamp,
					tx_index_in_block: 1,
				},
				tx_hash: McTxHash(tx_hash),
				utxo_tx_hash: McTxHash(utxo_tx_hash),
				utxo_index: UtxoIndexInTx(1),
			},
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_reward_address: CardanoRewardAddressBytes(reward_addr),
				dust_public_key: DustPublicKeyBytes::try_from(dust_pk.to_vec())
					.expect("33 bytes is valid for DustPublicKeyBytes"),
			}),
		}]
	}
}

#[async_trait::async_trait]
impl MidnightCNightObservationDataSource for CNightObservationDataSourceMock {
	async fn get_utxos_up_to_capacity(
		&self,
		_config: &CNightAddresses,
		start: &CardanoPosition,
		_current_tip: McBlockHash,
		_tx_capacity: usize,
		_utxo_overestimate: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		// Calculate deterministic end position
		let mut end = start.clone();
		end.block_number += 1;
		// Use deterministic block hash based on block number for consistency across nodes
		end.block_hash = McBlockHash(Self::deterministic_hash(end.block_number, 0));

		// Return deterministic UTXOs - same data on all nodes for the same block
		let utxos = Self::deterministic_utxos(start);

		Ok(ObservedUtxos { start: start.clone(), end, utxos })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_primitives_cnight_observation::TimestampUnixMillis;

	#[test]
	fn deterministic_hash_is_consistent() {
		// Same inputs should always produce same outputs
		let hash1 = CNightObservationDataSourceMock::deterministic_hash(100, 1);
		let hash2 = CNightObservationDataSourceMock::deterministic_hash(100, 1);
		assert_eq!(hash1, hash2);

		// Different block numbers should produce different hashes
		let hash3 = CNightObservationDataSourceMock::deterministic_hash(101, 1);
		assert_ne!(hash1, hash3);

		// Different salts should produce different hashes
		let hash4 = CNightObservationDataSourceMock::deterministic_hash(100, 2);
		assert_ne!(hash1, hash4);
	}

	#[test]
	fn deterministic_utxos_are_consistent() {
		let position = CardanoPosition {
			block_number: 42,
			block_hash: McBlockHash([0u8; 32]),
			block_timestamp: TimestampUnixMillis(1234567890),
			tx_index_in_block: 0,
		};

		let utxos1 = CNightObservationDataSourceMock::deterministic_utxos(&position);
		let utxos2 = CNightObservationDataSourceMock::deterministic_utxos(&position);

		assert_eq!(utxos1.len(), utxos2.len());
		assert_eq!(utxos1[0].header.tx_hash.0, utxos2[0].header.tx_hash.0);
		assert_eq!(utxos1[0].header.utxo_tx_hash.0, utxos2[0].header.utxo_tx_hash.0);
	}

	#[tokio::test]
	async fn mock_returns_deterministic_utxos() {
		let mock = CNightObservationDataSourceMock::new();
		let config = CNightAddresses::default();
		let start = CardanoPosition {
			block_number: 10,
			block_hash: McBlockHash([0u8; 32]),
			block_timestamp: TimestampUnixMillis(0),
			tx_index_in_block: 0,
		};

		let result1 = mock
			.get_utxos_up_to_capacity(&config, &start, McBlockHash([0u8; 32]), 100, 100)
			.await
			.unwrap();

		let result2 = mock
			.get_utxos_up_to_capacity(&config, &start, McBlockHash([0u8; 32]), 100, 100)
			.await
			.unwrap();

		// Should return UTXOs
		assert_eq!(result1.utxos.len(), 1);
		// Should be consistent
		assert_eq!(result1.utxos.len(), result2.utxos.len());
		assert_eq!(result1.end.block_hash.0, result2.end.block_hash.0);
		assert_eq!(result1.utxos[0].header.tx_hash.0, result2.utxos[0].header.tx_hash.0);
	}

	#[tokio::test]
	async fn different_blocks_produce_different_utxos() {
		let mock = CNightObservationDataSourceMock::new();
		let config = CNightAddresses::default();

		let start1 = CardanoPosition {
			block_number: 10,
			block_hash: McBlockHash([0u8; 32]),
			block_timestamp: TimestampUnixMillis(0),
			tx_index_in_block: 0,
		};

		let start2 = CardanoPosition {
			block_number: 20,
			block_hash: McBlockHash([0u8; 32]),
			block_timestamp: TimestampUnixMillis(0),
			tx_index_in_block: 0,
		};

		let result1 = mock
			.get_utxos_up_to_capacity(&config, &start1, McBlockHash([0u8; 32]), 100, 100)
			.await
			.unwrap();

		let result2 = mock
			.get_utxos_up_to_capacity(&config, &start2, McBlockHash([0u8; 32]), 100, 100)
			.await
			.unwrap();

		// Different block numbers should produce different UTXOs
		assert_ne!(result1.utxos[0].header.tx_hash.0, result2.utxos[0].header.tx_hash.0);
	}
}

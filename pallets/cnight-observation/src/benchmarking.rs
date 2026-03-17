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

//! Benchmarking setup for pallet-cnight-observation
//!
//! Uses Registration UTXOs as the benchmark input type. Registration handlers
//! exercise the storage-dominant path (2R + 1W + events per UTXO) without
//! requiring LedgerApi, making them suitable for deterministic benchmarking.
//! See planning assumption PL12 for the cost modelling rationale.

use super::*;
use alloc::vec::Vec;

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, TimestampUnixMillis,
	UtxoIndexInTx,
};
use midnight_primitives_mainchain_follower::{
	ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, RegistrationData,
};
use sidechain_domain::{McBlockHash, McTxHash};

/// Generate `count` synthetic Registration UTXOs with unique addresses.
fn generate_registration_utxos(count: u32) -> Vec<ObservedUtxo> {
	(0..count)
		.map(|i| {
			let mut addr_bytes = [0u8; 29];
			addr_bytes[0..4].copy_from_slice(&i.to_be_bytes());

			let mut dust_bytes = [0u8; 33];
			dust_bytes[0..4].copy_from_slice(&i.to_be_bytes());

			let mut tx_hash_bytes = [0u8; 32];
			tx_hash_bytes[0..4].copy_from_slice(&i.to_be_bytes());

			ObservedUtxo {
				header: ObservedUtxoHeader {
					tx_position: CardanoPosition {
						block_hash: McBlockHash([0u8; 32]),
						block_number: 1,
						block_timestamp: TimestampUnixMillis(20_000),
						tx_index_in_block: i,
					},
					tx_hash: McTxHash(tx_hash_bytes),
					utxo_tx_hash: McTxHash(tx_hash_bytes),
					utxo_index: UtxoIndexInTx(0),
				},
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_reward_address: CardanoRewardAddressBytes(addr_bytes),
					dust_public_key: DustPublicKeyBytes::try_from(&dust_bytes[..])
						.expect("33 bytes fits DustPublicKeyBytes bound"),
				}),
			}
		})
		.collect()
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark `process_tokens` with `n` Registration UTXOs.
	///
	/// Component `n`: number of observed UTXOs (0..MAX_UTXO_COUNT).
	#[benchmark]
	fn process_tokens(n: Linear<0, MAX_UTXO_COUNT>) {
		let utxos = generate_registration_utxos(n);

		let next_position = CardanoPosition {
			block_hash: McBlockHash([1u8; 32]),
			block_number: 2,
			block_timestamp: TimestampUnixMillis(40_000),
			tx_index_in_block: 0,
		};

		#[extrinsic_call]
		process_tokens(RawOrigin::None, utxos, next_position);

		assert_eq!(NextCardanoPosition::<T>::get().block_number, 2);
	}

	// Benchmark smoke tests run via the runtime crate (not the pallet crate),
	// because the external mock crate cannot propagate the `runtime-benchmarks`
	// feature without a circular dependency. Use:
	//   cargo test -p midnight-node-runtime --features runtime-benchmarks
}

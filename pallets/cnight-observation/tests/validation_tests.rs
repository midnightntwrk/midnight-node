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

//! Validation tests for `process_tokens` UTXO guard and weight reporting.
//!
//! Uses the lightweight `mock_with_capture` runtime (no LedgerApi dependency).
//! Covers: PM-19778 TC-01, TC-02, TC-06b.

use frame_support::{assert_noop, assert_ok, dispatch::Pays, sp_runtime::traits::Dispatchable};
use frame_system::RawOrigin;
use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, TimestampUnixMillis,
	UtxoIndexInTx,
};
use midnight_primitives_mainchain_follower::{
	ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, RegistrationData,
};
use pallet_cnight_observation::*;
use pallet_cnight_observation_mock::mock_with_capture::{RuntimeCall, Test, new_test_ext};
use sidechain_domain::{McBlockHash, McTxHash};

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

fn next_position() -> CardanoPosition {
	CardanoPosition {
		block_hash: McBlockHash([1u8; 32]),
		block_number: 2,
		block_timestamp: TimestampUnixMillis(40_000),
		tx_index_in_block: 0,
	}
}

/// PM-19778 TC-01: process_tokens rejects UTXO count exceeding capacity bound.
///
/// With CardanoTxCapacityPerBlock=2, the bound is 2*64=128. Submitting 129 UTXOs
/// must fail with TooManyUtxos and leave no state modifications.
#[test]
fn test_process_tokens_rejects_too_many_utxos() {
	new_test_ext().execute_with(|| {
		let capacity = 2u32;
		CardanoTxCapacityPerBlock::<Test>::put(capacity);
		let bound = capacity * UTXO_PER_TX_OVERESTIMATE;
		let utxos = generate_registration_utxos(bound + 1);

		let call = Call::<Test>::process_tokens { utxos, next_cardano_position: next_position() };

		assert_noop!(
			RuntimeCall::CNightObservation(call).dispatch(RawOrigin::None.into()),
			Error::<Test>::TooManyUtxos
		);

		// Verify no state was modified
		assert_eq!(
			NextCardanoPosition::<Test>::get().block_number,
			0,
			"NextCardanoPosition should not be updated on rejection"
		);
	});
}

/// PM-19778 TC-02: process_tokens accepts UTXO count at exactly the capacity bound.
///
/// With CardanoTxCapacityPerBlock=2, submitting exactly 128 UTXOs must succeed.
#[test]
fn test_process_tokens_accepts_max_utxos() {
	new_test_ext().execute_with(|| {
		let capacity = 2u32;
		CardanoTxCapacityPerBlock::<Test>::put(capacity);
		let bound = capacity * UTXO_PER_TX_OVERESTIMATE;
		let utxos = generate_registration_utxos(bound);

		let call = Call::<Test>::process_tokens { utxos, next_cardano_position: next_position() };

		assert_ok!(RuntimeCall::CNightObservation(call).dispatch(RawOrigin::None.into()));

		assert_eq!(
			NextCardanoPosition::<Test>::get().block_number,
			2,
			"NextCardanoPosition should be updated on success"
		);
	});
}

/// PM-19778 TC-06b: process_tokens returns PostDispatchInfo with actual weight
/// based on UTXO count and pays_fee == Pays::No.
#[test]
fn test_process_tokens_returns_actual_weight() {
	new_test_ext().execute_with(|| {
		let n = 5u32;
		let utxos = generate_registration_utxos(n);

		let call = Call::<Test>::process_tokens { utxos, next_cardano_position: next_position() };

		let result = RuntimeCall::CNightObservation(call).dispatch(RawOrigin::None.into());
		let post_info = result.expect("dispatch should succeed");

		let expected_weight =
			<() as pallet_cnight_observation::weights::WeightInfo>::process_tokens(n);
		assert_eq!(
			post_info.actual_weight,
			Some(expected_weight),
			"actual_weight should reflect WeightInfo::process_tokens(N)"
		);
		assert_eq!(post_info.pays_fee, Pays::No, "inherent extrinsic should not charge fees");
	});
}

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

//! Validation tests for `process_tokens` admission, weight reporting and the
//! observation budget setter.
//!
//! Uses the lightweight `mock_with_capture` runtime (no LedgerApi dependency).
//! Covers: PM-19778 TC-06b.

use frame_support::{
	assert_noop, assert_ok,
	dispatch::Pays,
	sp_runtime::{DispatchError, traits::Dispatchable},
};
use frame_system::RawOrigin;
use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, DATA_ORDERED_UTXOS_SPEC_VERSION,
	DustPublicKeyBytes, ObservationBatch, TimestampUnixMillis, UtxoIndexInTx,
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

/// The pallet accepts batches above the count bound. A failing Mandatory inherent is
/// `BadMandatory` and halts the chain, so bounding the batch is the inherent data
/// provider's job, not a runtime check.
#[test]
fn test_process_tokens_accepts_utxos_above_old_count_bound() {
	new_test_ext().execute_with(|| {
		let utxos = generate_registration_utxos(200 * 64 + 1);

		let utxos = ObservationBatch::new(utxos, DATA_ORDERED_UTXOS_SPEC_VERSION);
		let call = Call::<Test>::process_tokens { utxos, next_cardano_position: next_position() };

		assert_ok!(RuntimeCall::CNightObservation(call).dispatch(RawOrigin::None.into()));

		assert_eq!(
			NextCardanoCursor::<Test>::get().position.block_number,
			2,
			"NextCardanoCursor should be updated on success"
		);
	});
}

#[test]
fn set_observation_budget_percent_is_root_only_and_capped_at_100() {
	new_test_ext().execute_with(|| {
		assert_eq!(ObservationBudgetPercent::<Test>::get(), DEFAULT_OBSERVATION_BUDGET_PERCENT);

		assert_noop!(
			Pallet::<Test>::set_observation_budget_percent(RawOrigin::None.into(), 30),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Pallet::<Test>::set_observation_budget_percent(RawOrigin::Root.into(), 101),
			Error::<Test>::BudgetPercentOutOfRange
		);

		assert_ok!(Pallet::<Test>::set_observation_budget_percent(RawOrigin::Root.into(), 100));
		assert_eq!(ObservationBudgetPercent::<Test>::get(), 100);
	});
}

/// PM-19778 TC-06b: process_tokens returns PostDispatchInfo with actual weight
/// based on UTXO count and pays_fee == Pays::No.
#[test]
fn test_process_tokens_returns_actual_weight() {
	new_test_ext().execute_with(|| {
		let n = 5u32;
		let utxos = generate_registration_utxos(n);

		let expected_weight = Pallet::<Test>::process_tokens_weight(&utxos);
		let utxos = ObservationBatch::new(utxos, DATA_ORDERED_UTXOS_SPEC_VERSION);
		let call = Call::<Test>::process_tokens { utxos, next_cardano_position: next_position() };

		let result = RuntimeCall::CNightObservation(call).dispatch(RawOrigin::None.into());
		let post_info = result.expect("dispatch should succeed");

		assert_eq!(
			post_info.actual_weight,
			Some(expected_weight),
			"actual_weight should reflect process_tokens_weight(utxos)"
		);
		assert_eq!(post_info.pays_fee, Pays::No, "inherent extrinsic should not charge fees");
	});
}

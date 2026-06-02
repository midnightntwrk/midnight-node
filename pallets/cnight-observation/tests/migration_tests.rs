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

//! v0 -> v1 storage migration tests.
//!
//! Drives `SteppedMigration::step` directly on the mock runtime; the MBM
//! framework is not exercised here. Uses `mock_with_capture` to avoid the
//! ledger dependency — the migration only touches pallet storage.

use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	storage_alias,
	weights::{RuntimeDbWeight, WeightMeter},
};
use midnight_primitives_cnight_observation::{CardanoRewardAddressBytes, DustPublicKeyBytes};
use pallet_cnight_observation::{
	Config, Mapping, MappingEntry, Pallet,
	migrations::v1::{MAX_ENTRIES_PER_ADDR, MigrateV0ToV1},
};
use pallet_cnight_observation_mock::mock_with_capture::{Test, new_test_ext};
use sidechain_domain::UtxoId;

/// Matches the legacy pre-migration `Mappings` storage. Kept in a sub-module
/// so the `storage_alias` item name is literally `Mappings`, matching the
/// actual on-chain storage prefix used before the v0 -> v1 migration.
mod legacy {
	use super::*;

	#[storage_alias]
	pub type Mappings<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		CardanoRewardAddressBytes,
		Vec<MappingEntry>,
		ValueQuery,
	>;
}

fn addr(byte: u8) -> CardanoRewardAddressBytes {
	CardanoRewardAddressBytes([byte; 29])
}

fn dust(byte: u8) -> DustPublicKeyBytes {
	DustPublicKeyBytes(vec![byte; 33].try_into().unwrap())
}

fn entry(a: CardanoRewardAddressBytes, d: DustPublicKeyBytes, tx: u8, ix: u16) -> MappingEntry {
	MappingEntry {
		cardano_reward_address: a,
		dust_public_key: d,
		utxo_id: UtxoId::new([tx; 32], ix),
	}
}

/// Drive the migration to completion under an unbounded weight meter.
fn run_to_completion() {
	let mut cursor = None;
	loop {
		let mut meter = WeightMeter::new();
		cursor = MigrateV0ToV1::<Test>::step(cursor, &mut meter)
			.expect("step must not fail under unlimited weight");
		if cursor.is_none() {
			break;
		}
	}
}

#[test]
fn drives_v0_into_v1_double_map() {
	new_test_ext().execute_with(|| {
		let alice = addr(0xAA);
		let alice_dust = dust(0x01);
		let bob = addr(0xBB);
		let bob_d1 = dust(0x02);
		let bob_d2 = dust(0x03);

		legacy::Mappings::<Test>::insert(alice, vec![entry(alice, alice_dust.clone(), 1, 0)]);
		legacy::Mappings::<Test>::insert(
			bob,
			vec![entry(bob, bob_d1.clone(), 2, 0), entry(bob, bob_d2.clone(), 2, 1)],
		);

		StorageVersion::new(0).put::<Pallet<Test>>();
		run_to_completion();

		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
		assert!(
			legacy::Mappings::<Test>::iter().next().is_none(),
			"legacy storage should be drained",
		);
		assert_eq!(Mapping::<Test>::iter_prefix_values(alice).count(), 1);
		assert_eq!(Mapping::<Test>::iter_prefix_values(bob).count(), 2);
		assert_eq!(Mapping::<Test>::get(alice, UtxoId::new([1; 32], 0)), Some(alice_dust));
		assert_eq!(Mapping::<Test>::get(bob, UtxoId::new([2; 32], 0)), Some(bob_d1));
		assert_eq!(Mapping::<Test>::get(bob, UtxoId::new([2; 32], 1)), Some(bob_d2));
	});
}

#[test]
fn step_on_empty_storage_completes_immediately() {
	new_test_ext().execute_with(|| {
		let mut meter = WeightMeter::new();
		let cursor = MigrateV0ToV1::<Test>::step(None, &mut meter).unwrap();
		assert!(cursor.is_none());
		assert!(Mapping::<Test>::iter().next().is_none());
	});
}

/// Worst case writes per step: drain the legacy row plus
/// `MAX_ENTRIES_PER_ADDR` inserts. Mirrors the migration's own internal
/// calculation.
const PER_STEP_WRITES: u64 = 1 + MAX_ENTRIES_PER_ADDR;

fn per_step_weight() -> Weight {
	<<Test as frame_system::Config>::DbWeight as Get<RuntimeDbWeight>>::get()
		.reads_writes(1, PER_STEP_WRITES)
}

#[test]
fn insufficient_meter_returns_error() {
	new_test_ext().execute_with(|| {
		let alice = addr(0xAA);
		legacy::Mappings::<Test>::insert(alice, vec![entry(alice, dust(0x01), 1, 0)]);

		let mut meter = WeightMeter::with_limit(Weight::zero());
		let result = MigrateV0ToV1::<Test>::step(None, &mut meter);

		assert!(
			matches!(result, Err(SteppedMigrationError::InsufficientWeight { .. })),
			"empty meter must surface InsufficientWeight, got {result:?}",
		);
	});
}

#[test]
fn returns_cursor_to_resume_when_meter_exhausts_mid_migration() {
	new_test_ext().execute_with(|| {
		for i in 0..5u8 {
			let a = addr(0x10 + i);
			legacy::Mappings::<Test>::insert(a, vec![entry(a, dust(0x55), 1, 0)]);
		}

		// Budget exactly one row's worth of work: the inner loop migrates one
		// row, the next `try_consume` fails, and step returns a `Some(_)`
		// cursor for the next call.
		let mut meter = WeightMeter::with_limit(per_step_weight());
		let cursor = MigrateV0ToV1::<Test>::step(None, &mut meter).unwrap();
		assert!(cursor.is_some(), "cursor must be returned when meter exhausts mid-migration");
		assert_eq!(legacy::Mappings::<Test>::iter().count(), 4, "exactly one row migrated");

		// Resume with unlimited budget: drain the remaining rows.
		let mut meter = WeightMeter::new();
		let cursor = MigrateV0ToV1::<Test>::step(cursor, &mut meter).unwrap();
		assert!(cursor.is_none());
		assert!(legacy::Mappings::<Test>::iter().next().is_none());
	});
}

#[test]
fn step_resumes_strictly_past_provided_cursor() {
	new_test_ext().execute_with(|| {
		let a = addr(0x10);
		let b = addr(0x20);
		let c = addr(0x30);
		legacy::Mappings::<Test>::insert(a, vec![entry(a, dust(0x55), 1, 0)]);
		legacy::Mappings::<Test>::insert(b, vec![entry(b, dust(0x55), 2, 0)]);
		legacy::Mappings::<Test>::insert(c, vec![entry(c, dust(0x55), 3, 0)]);

		let mut meter = WeightMeter::new();
		let cursor = MigrateV0ToV1::<Test>::step(Some(a), &mut meter).unwrap();

		assert!(cursor.is_none(), "rest of the rows must drain in one call");
		assert!(legacy::Mappings::<Test>::contains_key(a), "row at cursor must be left untouched");
		assert!(!legacy::Mappings::<Test>::contains_key(b));
		assert!(!legacy::Mappings::<Test>::contains_key(c));
		assert_eq!(Mapping::<Test>::iter_prefix_values(a).count(), 0);
		assert_eq!(Mapping::<Test>::iter_prefix_values(b).count(), 1);
		assert_eq!(Mapping::<Test>::iter_prefix_values(c).count(), 1);
	});
}

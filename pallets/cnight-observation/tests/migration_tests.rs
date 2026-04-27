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
//! Uses `mock_with_capture` to avoid the ledger dependency - the migration
//! only touches pallet storage.

use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};
use midnight_primitives_cnight_observation::{CardanoRewardAddressBytes, DustPublicKeyBytes};
use pallet_cnight_observation::{
	Config, Mapping, MappingCount, MappingEntry, Pallet, migration::MigrateV0ToV1,
};
use pallet_cnight_observation_mock::mock_with_capture::{Test, new_test_ext};
use sidechain_domain::McTxHash;

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
		utxo_tx_hash: McTxHash([tx; 32]),
		utxo_index: ix,
	}
}

#[test]
fn migration_v0_to_v1_translates_entries_and_counts() {
	new_test_ext().execute_with(|| {
		StorageVersion::new(0).put::<Pallet<Test>>();

		let alice = addr(0xAA);
		let alice_dust = dust(0x01);
		let bob = addr(0xBB);
		let bob_dust1 = dust(0x02);
		let bob_dust2 = dust(0x03);

		legacy::Mappings::<Test>::insert(alice, vec![entry(alice, alice_dust.clone(), 1, 0)]);
		legacy::Mappings::<Test>::insert(
			bob,
			vec![entry(bob, bob_dust1.clone(), 2, 0), entry(bob, bob_dust2.clone(), 2, 1)],
		);

		let _ = MigrateV0ToV1::<Test>::on_runtime_upgrade();

		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
		assert!(
			legacy::Mappings::<Test>::iter().next().is_none(),
			"legacy storage should be drained",
		);

		assert_eq!(MappingCount::<Test>::get(alice), 1);
		assert_eq!(MappingCount::<Test>::get(bob), 2);

		assert_eq!(Mapping::<Test>::get(alice, (McTxHash([1; 32]), 0)), Some(alice_dust),);
		assert_eq!(Mapping::<Test>::get(bob, (McTxHash([2; 32]), 0)), Some(bob_dust1),);
		assert_eq!(Mapping::<Test>::get(bob, (McTxHash([2; 32]), 1)), Some(bob_dust2),);
	});
}

#[test]
fn migration_v0_to_v1_is_idempotent_once_applied() {
	new_test_ext().execute_with(|| {
		StorageVersion::new(1).put::<Pallet<Test>>();

		// Seed legacy storage with data that would be migrated if the
		// version guard were missing. After the no-op upgrade the legacy
		// data must remain untouched and `Mapping`/`MappingCount` must
		// stay empty.
		let alice = addr(0xAA);
		let alice_dust = dust(0x01);
		legacy::Mappings::<Test>::insert(alice, vec![entry(alice, alice_dust, 1, 0)]);

		let _ = MigrateV0ToV1::<Test>::on_runtime_upgrade();

		assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
		assert!(
			legacy::Mappings::<Test>::get(alice).len() == 1,
			"legacy storage must not be drained",
		);
		assert_eq!(MappingCount::<Test>::get(alice), 0);
		assert!(Mapping::<Test>::iter_prefix_values(alice).next().is_none());
	});
}

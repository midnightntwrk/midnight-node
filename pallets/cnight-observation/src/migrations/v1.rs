// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage migration v0 → v1.
//!
//! The pre-migration `Mappings` storage held `Vec<MappingEntry>` per Cardano
//! reward address. This migration drains that map and writes each entry into
//! the new `Mapping` double map (keyed by UTXO reference).

extern crate alloc;

use alloc::vec::Vec;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	storage_alias,
	weights::WeightMeter,
};

use super::PALLET_MIGRATIONS_ID;
use crate::{Config, Mapping, MappingEntry, Pallet};
use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;

/// Worst-case number of `MappingEntry` values stored against a single
/// `CardanoRewardAddressBytes` in legacy v0 storage. Drives the per-step
/// weight charge: each step drains one v0 row, doing one read plus
/// `1 + MAX_ENTRIES_PER_ADDR` writes (the row removal plus one insert per
/// migrated entry).
/// At time of writing (2026-04-28 9:10 UTC), this value is 44 for Cardano Preview, 18 for Mainnet
/// + some headroom in case the worst-case increases
pub const MAX_ENTRIES_PER_ADDR: u64 = 100;

mod v0 {
	use super::*;

	/// Legacy `Mappings` storage — a single `Vec<MappingEntry>` per reward
	/// address. Aliased so we can drain it after the pallet renames its new
	/// storage item to `Mapping` (singular).
	#[storage_alias]
	pub type Mappings<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		CardanoRewardAddressBytes,
		Vec<MappingEntry>,
		ValueQuery,
	>;
}

pub struct MigrateV0ToV1<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> SteppedMigration for MigrateV0ToV1<T> {
	type Cursor = CardanoRewardAddressBytes;
	type Identifier = MigrationId<25>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// Manual weight calculation - uses MAX_ENTRIES_PER_ADDR, fetched from Cardano networks at
		// time of writing + some headroom
		let required = T::DbWeight::get().reads_writes(1, 1 + MAX_ENTRIES_PER_ADDR);

		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let mut iter = match cursor {
				Some(last_key) => {
					v0::Mappings::<T>::iter_from(v0::Mappings::<T>::hashed_key_for(last_key))
				},
				None => v0::Mappings::<T>::iter(),
			};

			match iter.next() {
				Some((addr, entries)) => {
					debug_assert!(
						entries.len() as u64 <= MAX_ENTRIES_PER_ADDR,
						"v0 row exceeded MAX_ENTRIES_PER_ADDR — per-step weight under-charged",
					);
					v0::Mappings::<T>::remove(addr);
					for entry in entries {
						Mapping::<T>::insert(addr, entry.utxo_id, entry.dust_public_key);
					}
					cursor = Some(addr);
				},
				None => {
					// MBM doesn't bump the pallet's `StorageVersion`; do it
					// ourselves so `on_chain_storage_version()` reflects the
					// post-migration shape.
					StorageVersion::new(1).put::<Pallet<T>>();
					cursor = None;
					break;
				},
			}
		}

		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// Snapshot the entire legacy state. If we're already at v1 this will
		// be empty, which makes `post_upgrade` a no-op — exactly what we want
		// for an idempotent migration.
		let v0_state: Vec<(CardanoRewardAddressBytes, Vec<MappingEntry>)> =
			v0::Mappings::<T>::iter().collect();
		Ok(v0_state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;

		ensure!(
			Pallet::<T>::on_chain_storage_version() == 1,
			"storage version must be 1 after migration"
		);
		ensure!(
			v0::Mappings::<T>::iter().next().is_none(),
			"legacy v0 Mappings storage must be fully drained"
		);

		let v0_state: Vec<(CardanoRewardAddressBytes, Vec<MappingEntry>)> =
			Decode::decode(&mut state.as_slice()).expect("pre_upgrade snapshot must decode");

		for (addr, entries) in v0_state {
			ensure!(
				Mapping::<T>::iter_prefix_values(addr).count() == entries.len(),
				"v1 Mapping prefix count must equal v0 vec length"
			);
			for entry in entries {
				ensure!(
					Mapping::<T>::get(addr, entry.utxo_id) == Some(entry.dust_public_key),
					"v1 dust key must match v0 entry"
				);
			}
		}

		Ok(())
	}
}

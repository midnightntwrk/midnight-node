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
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};

use crate::{Config, Mapping, MappingEntry, Pallet};
use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;

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

impl<T: Config> OnRuntimeUpgrade for MigrateV0ToV1<T> {
	fn on_runtime_upgrade() -> Weight {
		if Pallet::<T>::on_chain_storage_version() != 0 {
			return T::DbWeight::get().reads(1);
		}

		let mut reads: u64 = 1;
		let mut writes: u64 = 0;

		for (addr, entries) in v0::Mappings::<T>::drain() {
			reads = reads.saturating_add(1);
			// `drain` removes the legacy entry — count that as a write.
			writes = writes.saturating_add(1);
			for entry in entries {
				Mapping::<T>::insert(addr, entry.utxo_id, entry.dust_public_key);
				writes = writes.saturating_add(1);
			}
		}

		StorageVersion::new(1).put::<Pallet<T>>();
		writes = writes.saturating_add(1);

		T::DbWeight::get().reads_writes(reads, writes)
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

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
//! reward address. This migration drains that map and splits each entry into
//! the new `Mapping` double map (keyed by UTXO reference) and `MappingCount`
//! counter.

extern crate alloc;

use alloc::vec::Vec;
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};

use crate::{Config, Mapping, MappingCount, MappingEntry, Pallet};
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
			MappingCount::<T>::insert(addr, entries.len() as u64);
			writes = writes.saturating_add(1);
			for entry in entries {
				Mapping::<T>::insert(
					addr,
					(entry.utxo_tx_hash, entry.utxo_index),
					entry.dust_public_key,
				);
				writes = writes.saturating_add(1);
			}
		}

		StorageVersion::new(1).put::<Pallet<T>>();
		writes = writes.saturating_add(1);

		T::DbWeight::get().reads_writes(reads, writes)
	}
}

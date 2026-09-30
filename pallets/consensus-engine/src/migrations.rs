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

//! Storage migrations for `pallet-consensus-engine`.

/// Activation of the pallet on an upgraded network (storage version 0 → 1).
///
/// From the first block executed by the runtime that adds this pallet, every block must carry a
/// BABE `SecondaryPlain` pre-runtime digest after the AURA one, and `pallet-babe` runs its
/// `on_initialize` before this pallet's — so the very first BABE digest it sees would make it
/// self-initialize its genesis epoch (`GenesisSlot == 0`) and deposit a `NextEpochData` digest
/// into a header that cannot be retracted. Runtime migrations execute before any pallet's
/// `on_initialize`, so pre-seeding `GenesisSlot` here with a non-zero sentinel closes that
/// window; [`crate::Pallet::migrate_to_babe`] overwrites it with the real genesis slot at the
/// flip.
pub mod v1 {
	use crate::{Config, Pallet};
	use core::marker::PhantomData;
	use frame_support::{
		migrations::VersionedMigration,
		traits::{Get, UncheckedOnRuntimeUpgrade},
		weights::Weight,
	};

	/// The unversioned step; wire [`Activate`] instead.
	pub struct InnerActivate<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for InnerActivate<T> {
		fn on_runtime_upgrade() -> Weight {
			Pallet::<T>::activate();
			T::DbWeight::get().writes(1)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			frame_support::ensure!(
				pallet_babe::GenesisSlot::<T>::get() == crate::babe_genesis_slot_sentinel(),
				"pallet-babe GenesisSlot must hold the activation sentinel",
			);
			frame_support::ensure!(
				crate::EngineState::<T>::get() == crate::State::Aura,
				"consensus-engine must activate in state Aura",
			);
			Ok(())
		}
	}

	/// Runs [`InnerActivate`] once, when the on-chain storage version is 0, and sets it to 1.
	pub type Activate<T> = VersionedMigration<
		0,
		1,
		InnerActivate<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

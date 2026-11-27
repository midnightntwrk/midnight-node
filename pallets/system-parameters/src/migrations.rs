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

//! Storage migrations of the system parameters pallet.

use crate::pallet::{CnightWeightStorage, Config, Event, INITIAL_CNIGHT_WEIGHT, Pallet};
use frame_support::{pallet_prelude::*, traits::OnRuntimeUpgrade};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// Sets `cnight_weight` to its activation value and emits `CnightWeightUpdated`, so the value
/// is never missing and its first setting is on record like every later one.
///
/// Keyed on the storage itself, not on the pallet's storage version: a version bumped only
/// with the `decentralization` feature would disagree with the chain once a build without it
/// ran there. Once the value exists, every run is a no-op, whoever set it.
pub struct InitializeCnightWeight<T>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for InitializeCnightWeight<T> {
	fn on_runtime_upgrade() -> Weight {
		if CnightWeightStorage::<T>::exists() {
			return T::DbWeight::get().reads(1);
		}
		let (numerator, denominator) = INITIAL_CNIGHT_WEIGHT;
		CnightWeightStorage::<T>::put((numerator, denominator));
		Pallet::<T>::deposit_event(Event::CnightWeightUpdated { numerator, denominator });
		// The weight write and the event.
		T::DbWeight::get().reads_writes(1, 2)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (_, denominator) = CnightWeightStorage::<T>::get();
		ensure!(CnightWeightStorage::<T>::exists(), "cnight_weight must be set");
		ensure!(denominator > 0, "cnight_weight's denominator must not be 0");
		Ok(())
	}
}

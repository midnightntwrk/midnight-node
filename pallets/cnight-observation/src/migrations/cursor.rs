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

//! Moves `NextCardanoPosition` into [`NextCardanoCursor`] with a zero UTXO offset.
//!
//! Keyed on the old value, not the storage version: the multi-block migrations that
//! raise the version run after this one.

use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};
use midnight_primitives_cnight_observation::{CardanoCursor, CardanoPosition};

use crate::{Config, NextCardanoCursor, Pallet};

#[storage_alias]
pub type NextCardanoPosition<T: Config> = StorageValue<Pallet<T>, CardanoPosition, OptionQuery>;

pub struct MigrateToNextCardanoCursor<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToNextCardanoCursor<T> {
	fn on_runtime_upgrade() -> Weight {
		let Some(position) = NextCardanoPosition::<T>::take() else {
			return T::DbWeight::get().reads(1);
		};
		NextCardanoCursor::<T>::put(CardanoCursor { position, utxo_offset: 0 });
		T::DbWeight::get().reads_writes(1, 2)
	}
}

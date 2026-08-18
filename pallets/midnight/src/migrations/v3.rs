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

//! v2 → v3: re-encode `StateKey<T>` from `Vec<u8>` to `LedgerStateKey`.
//!
//! v3 wraps the chain-tip state key in a typed enum so the Bridge can
//! distinguish Anchored (post-block / genesis — never unpersisted) from
//! Transient (intra-block intermediate — unpersisted by successor) at the
//! type level. The pre-existing on-chain bytes are the post-block tip of the
//! prior block, so they wrap as `LedgerStateKey::Anchored(bytes)`.
//!
//! Ordering: this must run after [`crate::migrations::v2`], which still reads
//! and writes the raw-bytes layout. Both live in the runtime `Migrations`
//! tuple, which applies its members in declaration order, so a chain at
//! storage version 1 translates v8 → v9 first and re-encodes the resulting
//! root here.

use crate::{Config, Pallet, StateKey, migrations::v2::old};
use core::marker::PhantomData;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
	weights::Weight,
};
use midnight_node_ledger::types::LedgerStateKey;

/// The actual migration logic. `VersionedMigration` wraps this with the
/// version gating and storage-version bump.
pub struct InnerMigration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		// `old::StateKey` is the raw-bytes read-side alias v2 also uses
		let bytes = old::StateKey::<T>::get();
		log::info!(
			target: "pallet-midnight",
			"Migrating StateKey to LedgerStateKey::Anchored ({} bytes)",
			bytes.len()
		);
		StateKey::<T>::put(LedgerStateKey::Anchored(bytes));
		T::DbWeight::get().reads_writes(1, 1)
	}
}

pub type Migration<T> =
	VersionedMigration<2, 3, InnerMigration<T>, Pallet<T>, <T as frame_system::Config>::DbWeight>;

// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
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

//! Storage migration from v1 to v2: ledger-v8 -> ledger-v9 state translation.
//!
//! This is the on-chain half of the ledger 8 -> 9 hardfork. The pallet stores
//! only the ledger state's arena root in [`crate::StateKey`]; the `LedgerState`
//! itself lives in the ledger arena (parity-db). This migration hands the v8
//! root to the [`migrate_state_v8_to_v9`](midnight_node_ledger::host_api::migration_8_to_9)
//! host function, which walks the v8 state, translates it into the v9 shape
//! (see [`midnight_node_ledger::state_translation_v8_to_v9`]), re-persists it,
//! and returns the new v9 root — which we write back into `StateKey`.
//!
//! It is a single-block migration: the host call translates the whole state in
//! one shot (a few milliseconds for dev/undeployed-sized state). It is wired
//! into [`crate::Migrations`](../../../runtime/src/lib.rs) via
//! [`VersionedMigration`], so it runs at most once, when a runtime at
//! pallet-midnight storage version 1 upgrades to this runtime (storage version
//! 2). A chain whose genesis is already ledger-9 starts at storage version 2
//! and never runs it.
//!
//! Storage version 1 does not, however, imply the *ledger* state is still v8:
//! the 2.0.0 runtime already ran ledger-9 yet shipped pallet-midnight at storage
//! version 1 (it had no v1->v2 migration), so a network upgrading 2.0.0 -> this
//! runtime triggers this migration over an already-v9 state. The host function
//! detects that case and no-ops (returns the `StateKey` unchanged), so the only
//! effect on that path is the storage-version bump to 2.

#[cfg(feature = "try-runtime")]
extern crate alloc;

use crate::{Pallet, StateKey, pallet::Config};
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
};
use midnight_node_ledger::types::active_ledger_bridge as LedgerApi;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// [`UncheckedOnRuntimeUpgrade`] implementation wrapped by [`MigrateV1ToV2`].
pub struct InnerMigrateV1ToV2<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV1ToV2<T> {
	fn on_runtime_upgrade() -> Weight {
		let state_key = StateKey::<T>::get();

		// The host function reads the v8 arena root, translates the referenced
		// `LedgerState` to v9, re-persists it, and returns the new v9 root
		// together with the synthetic cost (picoseconds) it charged the
		// translation against the ledger's deterministic cost model. If the
		// state is already v9 (e.g. a 2.0.0 -> this-runtime upgrade), it no-ops
		// and returns the `state_key` unchanged with zero cost.
		// A genuine failure here is unrecoverable: the chain would be left
		// pointing at a v8 state a ledger-9 runtime cannot read, so we abort the
		// upgrade.
		let (new_state_key, consumed_cost_ps) = LedgerApi::migrate_state_v8_to_v9(&state_key)
			.expect("FATAL: ledger v8->v9 state migration failed");

		StateKey::<T>::put(new_state_key);
		log::info!(
			target: "midnight::migration",
			"ledger v8->v9 state migration complete; StateKey re-pointed to v9 root ({consumed_cost_ps}ps synthetic cost)"
		);

		// The translation runs natively in the host call, so the pallet-side
		// read/write above is negligible next to it; report the ledger's own
		// synthetic cost as `ref_time` instead. This is the same 1:1 mapping
		// `pallet_midnight::get_tx_weight` uses for ordinary transactions
		// (ledger picoseconds -> `Weight` ref_time), and that cost model
		// already prices the arena reads/writes the translation performs, so
		// no separate `DbWeight` charge is added on top. `proof_size` is 0:
		// this chain doesn't build a PoV. Capped at the block's max weight,
		// since a pathological state could in principle exceed it.
		Weight::from_parts(consumed_cost_ps, 0).min(T::BlockWeights::get().max_block)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			!StateKey::<T>::get().is_empty(),
			"ledger StateKey must be populated before v8->v9 migration"
		);
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			!StateKey::<T>::get().is_empty(),
			"ledger StateKey must remain populated after v8->v9 migration"
		);
		Ok(())
	}
}

/// Translates the ledger state from v8 to v9 and bumps pallet-midnight storage
/// version 1 -> 2. Wired into the runtime `Migrations` tuple.
pub type MigrateV1ToV2<T> = VersionedMigration<
	1,
	2,
	InnerMigrateV1ToV2<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

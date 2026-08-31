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

//! Runtime migrations
//!
//! Fixed, one-shot migrations live in a pallet's `migrations` module and are wired into
//! `SingleBlockMigrations` or [`crate::Migrations`]. Re-usable migrations such as
//! `authority_keys` below are only wired in for the specific upgrade that needs them.

pub mod authority_keys {
	//! Scaffolding for migrating [`crate::opaque::SessionKeys`] with
	//! [`pallet_session_validator_management::migrations::authority_keys::AuthorityKeysMigration`].
	//!
	//! There is no pending `AuthorityKeys` shape change yet (`SessionKeys` is still aura + grandpa),
	//! so nothing here is wired into `SingleBlockMigrations`. When a change lands (e.g. adding beefy):
	//!
	//! 1. Update [`LegacySessionKeys`] and its `From` impl to match the pre-upgrade shape.
	//! 2. Add `authority_keys::AuthorityKeysMigration<Runtime, LegacyCommitteeMember, LegacySessionKeys, FROM, TO>`
	//!    to `SingleBlockMigrations`, with `FROM`/`TO` matching the pallet's on-chain storage
	//!    version **at the moment this migration is wired in** (see
	//!    [`pallet_session_validator_management::pallet::Pallet`]'s `#[pallet::storage_version]`).
	//! 3. After the upgrade that runs this migration has landed on all live networks, remove the
	//!    migration from `SingleBlockMigrations` **before** any genesis reset (devnet/qanet wipe) that
	//!    builds state at the post-migration pallet version with the new `AuthorityKeys` shape. If the
	//!    migration is still wired while on-chain storage remains at `FROM` but genesis already stores
	//!    new-shaped committee bytes, the next upgrade will run `translate::<OldCommitteeInfo, _>(...)`
	//!    and panic.
	use crate::{CrossChainPublic, Runtime, opaque::SessionKeys};
	use alloc::vec::Vec;
	use authority_selection_inherents::CommitteeMember;
	use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
	use pallet_session_validator_management::migrations::v2::{
		UpgradeCommitteeMember, V1ToV2Migration,
	};
	use parity_scale_codec::MaxEncodedLen;
	use sp_runtime::impl_opaque_keys;

	impl_opaque_keys! {
		#[derive(MaxEncodedLen, PartialOrd, Ord)]
		pub struct LegacySessionKeys {
			pub aura: crate::Aura,
			pub grandpa: crate::Grandpa,
		}
	}

	impl From<LegacySessionKeys> for SessionKeys {
		fn from(old: LegacySessionKeys) -> Self {
			SessionKeys { aura: old.aura, grandpa: old.grandpa }
		}
	}

	/// Committee member type using the pre-upgrade [`LegacySessionKeys`]
	pub type LegacyCommitteeMember = CommitteeMember<CrossChainPublic, LegacySessionKeys>;

	impl UpgradeCommitteeMember<Runtime> for LegacyCommitteeMember {
		fn upgrade(
			self,
		) -> <Runtime as pallet_session_validator_management::Config>::CommitteeMember {
			self.map_authority_keys(Into::into)
		}
	}

	const LOG_TARGET: &str = "runtime::migration::v1-to-v2-add-babe-session-keys";

	/// Combined v1-to-v2 migration: committee translation from the pre-BABE
	/// [`LegacySessionKeys`] shape, the `QueuedCommittee` seed, and the `pallet_session` key
	/// upgrade, gated on the pallet's on-chain storage version.
	type Inner = V1ToV2Migration<Runtime, LegacyCommitteeMember, LegacySessionKeys>;

	/// Migrates Current, Queued, and Next Committee storages and writes in consensus-engine
	/// a trace that it was executed.
	pub struct AddBabeToSessionKeysMigration;

	impl OnRuntimeUpgrade for AddBabeToSessionKeysMigration {
		fn on_runtime_upgrade() -> Weight {
			let db = <Runtime as frame_system::Config>::DbWeight::get();
			log::info!(
				target: LOG_TARGET,
				"translating committee & session keys and initializing QueuedCommittee",
			);
			if AddBabeSessionKeysMigrated::<Runtime>::get() {
				log::info!(
					"SessionKeys migration that adds BABE authority keys was already executed. Migration can be removed from the runtime."
				);
				db.reads(1)
			} else {
				let weight = <Inner as OnRuntimeUpgrade>::on_runtime_upgrade();
				AddBabeSessionKeysMigrated::<Runtime>::put(true);
				weight.saturating_add(db.reads_writes(1, 1))
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			// Key-translation correctness is enforced by the inner migrations' own decode asserts;
			// here we confirm the guard the committee decoders depend on is set.
			frame_support::ensure!(
				AddBabeSessionKeysMigrated::<Runtime>::get(),
				"add-babe-session-keys: guard not set after migration",
			);
			Ok(())
		}
	}
}

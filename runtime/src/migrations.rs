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
	//! 1. Update [`LegacySessionKeys`] and its `From` impl to match the pre-upgrade shape.
	//! 2. Add `authority_keys::MigrateAddBabeSessionKeys` to `SingleBlockMigrations`.
	//! 3. After the upgrade that runs this migration has landed on all live networks,
	//!    `MigrateAddBabeSessionKeys` can be removed.
	use crate::{CrossChainPublic, Runtime, opaque::SessionKeys};
	use alloc::vec::Vec;
	use authority_selection_inherents::CommitteeMember;
	use frame_support::{
		traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
		weights::Weight,
	};
	use pallet_consensus_engine::AddBabeSessionKeysMigrated;
	use pallet_session_validator_management::migrations::authority_keys::{
		InnerMigrateAuthorityKeys, UpgradeCommitteeMember,
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
			let babe_from_aura = old.aura.clone().into_inner().into();
			SessionKeys { aura: old.aura, grandpa: old.grandpa, babe: babe_from_aura }
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

	const LOG_TARGET: &str = "runtime::migration::add-babe-session-keys";

	/// The pallet migration logic itself.
	type Inner = InnerMigrateAuthorityKeys<Runtime, LegacyCommitteeMember, LegacySessionKeys>;

	/// Adds the BABE key to the committee's stored `SessionKeys`, exactly once per chain.
	pub struct MigrateAddBabeSessionKeys;

	impl OnRuntimeUpgrade for MigrateAddBabeSessionKeys {
		fn on_runtime_upgrade() -> Weight {
			let db = <Runtime as frame_system::Config>::DbWeight::get();
			if AddBabeSessionKeysMigrated::<Runtime>::get() {
				log::info!(target: LOG_TARGET, "already applied (guard set); skipping");
				return db.reads(1);
			}
			log::info!(target: LOG_TARGET, "adding BABE key to committee SessionKeys");
			let weight = <Inner as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();
			AddBabeSessionKeysMigrated::<Runtime>::put(true);
			weight.saturating_add(db.reads_writes(1, 1))
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			frame_support::ensure!(
				AddBabeSessionKeysMigrated::<Runtime>::get(),
				"add-babe-session-keys: guard not set after migration"
			);
			Ok(())
		}
	}
}

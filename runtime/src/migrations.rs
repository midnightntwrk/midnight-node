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
//! `SingleBlockMigrations` or [`crate::Migrations`]. The session-key cutover scaffolding below is
//! wired only in the runtime where [`crate::opaque::SessionKeys`] changes shape.

pub mod authority_keys {
	//! Runtime-side types for the session-key cutover (BABE added to
	//! [`crate::opaque::SessionKeys`]). [`MigrateV1ToV2AddBabeSessionKeys`] translates committee
	//! and [pallet_session] key storage in one versioned step and runs only on chains at pallet
	//! storage version 1; [`SetAddBabeSessionKeysMigratedFlag`] then records the guard the
	//! node-side committee decoders read. BABE placeholders reuse the validator's aura key; real
	//! keys become effective at a later session rotation from observed Cardano registrations.
	use crate::{CrossChainPublic, Runtime, opaque::SessionKeys};
	use alloc::vec::Vec;
	use authority_selection_inherents::CommitteeMember;
	use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
	use pallet_consensus_engine::AddBabeSessionKeysMigrated;
	use pallet_session_validator_management::migrations::authority_keys::{
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

	/// Combined v1-to-v2 committee and session-key migration for this runtime's cutover.
	pub type MigrateV1ToV2AddBabeSessionKeys =
		V1ToV2Migration<Runtime, LegacyCommitteeMember, LegacySessionKeys>;

	/// Sets [`AddBabeSessionKeysMigrated`], the guard the node-side committee decoders read.
	pub struct SetAddBabeSessionKeysMigratedFlag;

	impl OnRuntimeUpgrade for SetAddBabeSessionKeysMigratedFlag {
		fn on_runtime_upgrade() -> Weight {
			let db = <Runtime as frame_system::Config>::DbWeight::get();
			if AddBabeSessionKeysMigrated::<Runtime>::get() {
				db.reads(1)
			} else {
				AddBabeSessionKeysMigrated::<Runtime>::put(true);
				db.reads_writes(1, 1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			frame_support::ensure!(
				AddBabeSessionKeysMigrated::<Runtime>::get(),
				"add-babe-session-keys: guard not set after migration",
			);
			Ok(())
		}
	}
}

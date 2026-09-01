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
	//! Migrates [`crate::opaque::SessionKeys`], and the committee storages keyed by it, from the
	//! pre-beefy aura + grandpa shape.
	//!
	//! This chain goes from pallet v1 to v2 in a single upgrade, which combines two changes the
	//! toolkit ships separately: the `AuthorityKeys` shape change
	//! ([`pallet_session_validator_management::migrations::authority_keys`]) and v2's new
	//! `QueuedCommittee` ([`pallet_session_validator_management::migrations::v2`]). They cannot
	//! simply be wired one after the other — both are versioned 1 => 2, so the first to run would
	//! bump the version and gate the second out, and the toolkit's v2 migration seeds
	//! `QueuedCommittee` by reading `CurrentCommittee` in the *current* shape, which is only
	//! correct once the keys have been translated. Hence the combined inner below.
	//!
	//! [`AddBeefyToSessionKeysMigration`] is wired into `SingleBlockMigrations`. It is gated on
	//! `pallet_session_validator_management`'s on-chain storage version (1 => 2), so it runs once
	//! and is a no-op afterwards — including on fresh-genesis chains, which already start at 2 with
	//! new-shaped bytes. Drop it from `SingleBlockMigrations` once every live network is past it.
	use crate::{CrossChainPublic, Runtime, opaque::SessionKeys};
	// Used by the `impl_opaque_keys!` expansion below, which is `Vec`-generic in no-std.
	use alloc::vec::Vec;
	use authority_selection_inherents::CommitteeMember;
	use frame_support::migrations::VersionedMigration;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use frame_support::weights::Weight;
	use pallet_session_validator_management::migrations::authority_keys::{
		InnerMigrateAuthorityKeys, UpgradeCommitteeMember,
	};
	use pallet_session_validator_management::{CurrentCommittee, QueuedCommittee};
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
			let aura_raw = old.aura.clone().into_inner().0;
			let mut beefy_raw = [0u8; 33];
			beefy_raw[1..].copy_from_slice(&aura_raw);
			SessionKeys {
				// Invalid SEC1 tag keeps the placeholder distinct from any real key.
				beefy: sp_core::ecdsa::Public::from_raw(beefy_raw).into(),
				aura: old.aura,
				grandpa: old.grandpa,
			}
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

	/// The toolkit's reusable `AuthorityKeys` translation, instantiated for this chain's
	/// pre-beefy shape. Translates `CurrentCommittee`/`QueuedCommittee`/`NextCommittee` and
	/// `pallet_session`'s key storage.
	type TranslateKeys =
		InnerMigrateAuthorityKeys<Runtime, LegacyCommitteeMember, LegacySessionKeys>;

	/// Translates the committee and session keys, then seeds v2's `QueuedCommittee`.
	pub struct InnerAddBeefyToSessionKeys;

	impl UncheckedOnRuntimeUpgrade for InnerAddBeefyToSessionKeys {
		fn on_runtime_upgrade() -> Weight {
			let weight = <TranslateKeys as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			// At v1 `QueuedCommittee` does not exist, so the translation above left it absent.
			// The v1 session integration applied committees immediately, making the current
			// committee both the active and the queued validator set — seed it from the
			// now-translated `CurrentCommittee`.
			QueuedCommittee::<Runtime>::put(CurrentCommittee::<Runtime>::get());

			weight.saturating_add(
				<Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, 1),
			)
		}

		/// Captures the committees in their pre-upgrade shape. `CurrentCommittee`/`NextCommittee`
		/// `.get()` would decode as the post-upgrade `SessionKeys`, so the on-chain bytes are read
		/// through `unhashed` with [`LegacyCommitteeMember`]. `QueuedCommittee` does not exist at
		/// v1, so there is nothing to capture for it.
		///
		/// [`TranslateKeys`]'s own `pre_upgrade`/`post_upgrade` pair is not reused: its
		/// `post_upgrade` asserts `QueuedCommittee` is preserved, which the seed deliberately
		/// breaks.
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			use parity_scale_codec::Encode;

			let current: crate::LegacyCommitteeInfo =
				frame_support::storage::unhashed::get_or_default(
					&CurrentCommittee::<Runtime>::hashed_key(),
				);
			let next: Option<crate::LegacyCommitteeInfo> = frame_support::storage::unhashed::get(
				&pallet_session_validator_management::NextCommittee::<Runtime>::hashed_key(),
			);

			Ok((current, next).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			use frame_support::ensure;
			use parity_scale_codec::{Decode, Encode};
			use sp_session_validator_management::CommitteeMember as _;

			let (old_current, old_next): (
				crate::LegacyCommitteeInfo,
				Option<crate::LegacyCommitteeInfo>,
			) = Decode::decode(&mut state.as_slice()).map_err(|_| {
				sp_runtime::TryRuntimeError::Other("Previously encoded state should be decodable")
			})?;

			// Committee membership is compared by authority id: the keys necessarily differ,
			// since adding beefy is the point of the migration.
			let ids = |committee: &[LegacyCommitteeMember]| -> Vec<CrossChainPublic> {
				committee.iter().map(|m| m.authority_id()).collect()
			};
			let new_ids = |committee: &[<Runtime as pallet_session_validator_management::Config>::CommitteeMember]|
			 -> Vec<CrossChainPublic> {
				committee.iter().map(|m| m.authority_id()).collect()
			};

			let new_current = CurrentCommittee::<Runtime>::get();
			ensure!(old_current.epoch == new_current.epoch, "current epoch should be preserved");
			ensure!(
				ids(&old_current.committee) == new_ids(&new_current.committee),
				"current committee membership should be preserved"
			);

			let new_next = pallet_session_validator_management::NextCommittee::<Runtime>::get();
			ensure!(
				old_next.is_some() == new_next.is_some(),
				"next committee presence should be preserved"
			);
			if let (Some(old_next), Some(new_next)) = (old_next, new_next) {
				ensure!(old_next.epoch == new_next.epoch, "next epoch should be preserved");
				ensure!(
					ids(&old_next.committee) == new_ids(&new_next.committee),
					"next committee membership should be preserved"
				);
			}

			ensure!(
				QueuedCommittee::<Runtime>::get().encode() == new_current.encode(),
				"queued committee should be seeded from the current committee"
			);

			Ok(())
		}
	}

	/// Combined v1-to-v2 migration: committee translation from the pre-beefy
	/// [`LegacySessionKeys`] shape, the `QueuedCommittee` seed, and the `pallet_session` key
	/// upgrade, gated on `pallet_session_validator_management`'s on-chain storage version.
	pub type AddBeefyToSessionKeysMigration = VersionedMigration<
		1,
		2,
		InnerAddBeefyToSessionKeys,
		pallet_session_validator_management::Pallet<Runtime>,
		<Runtime as frame_system::Config>::DbWeight,
	>;
}

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
	use crate::{CrossChainPublic, Runtime, opaque::SessionKeys, upgrade_committee_info};
	// Used by the `impl_opaque_keys!` expansion below, which is `Vec`-generic in no-std.
	use alloc::vec::Vec;
	use authority_selection_inherents::CommitteeMember;
	use frame_support::migrations::VersionedMigration;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use frame_support::weights::Weight;
	use pallet_session_validator_management::migrations::authority_keys::UpgradeCommitteeMember;
	use pallet_session_validator_management::{CurrentCommittee, NextCommittee, QueuedCommittee};
	use parity_scale_codec::MaxEncodedLen;
	use sp_runtime::impl_opaque_keys;
	use sp_session_validator_management::CommitteeMember as _;

	impl_opaque_keys! {
		#[derive(MaxEncodedLen, PartialOrd, Ord)]
		pub struct LegacySessionKeys {
			pub aura: crate::Aura,
			pub grandpa: crate::Grandpa,
		}
	}

	/// Builds the post-upgrade keys for a validator whose cross-chain key is known.
	///
	/// The committee registers each validator's cross-chain key as its beefy key
	/// (`beefy_pub_key == sidechain_pub_key`), and both are ECDSA, so the cross-chain key is the
	/// beefy key this validator actually holds a secret for.
	fn upgrade_keys_with(old: LegacySessionKeys, cross_chain: CrossChainPublic) -> SessionKeys {
		SessionKeys {
			beefy: sp_core::ecdsa::Public::from(cross_chain.into_inner()).into(),
			aura: old.aura,
			grandpa: old.grandpa,
		}
	}

	/// Fallback for a `pallet_session` entry whose validator is in none of the committees, so no
	/// cross-chain key is recoverable — `ValidatorId` is `blake2_256` of it, which is one-way.
	///
	/// Only committee members become BEEFY authorities, so a stale entry's keys are inert; the
	/// aura bytes behind the invalid SEC1 tag `0x00` keep the placeholder distinct from any real
	/// key rather than colliding with one.
	impl From<LegacySessionKeys> for SessionKeys {
		fn from(old: LegacySessionKeys) -> Self {
			let aura_raw = old.aura.clone().into_inner().0;
			let mut beefy_raw = [0u8; 33];
			beefy_raw[1..].copy_from_slice(&aura_raw);
			SessionKeys {
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
			// A committee member carries its own cross-chain key, so no lookup is needed here.
			let cross_chain = self.authority_id();
			self.map_authority_keys(|old| upgrade_keys_with(old, cross_chain.clone()))
		}
	}

	/// Reads the still-legacy-shaped committees and indexes their members' cross-chain keys by the
	/// `pallet_session` validator id.
	///
	/// `pallet_session` keys validators by `AccountId`, which is `blake2_256` of the cross-chain
	/// key and therefore not invertible — so the mapping is rebuilt by hashing each committee
	/// member's `id` forward. The committees are the only on-chain source of these keys.
	fn cross_chain_keys_by_validator() -> Vec<(crate::AccountId, CrossChainPublic)> {
		let committees = [
			frame_support::storage::unhashed::get::<crate::LegacyCommitteeInfo>(
				&CurrentCommittee::<Runtime>::hashed_key(),
			),
			frame_support::storage::unhashed::get::<crate::LegacyCommitteeInfo>(
				&QueuedCommittee::<Runtime>::hashed_key(),
			),
			frame_support::storage::unhashed::get::<crate::LegacyCommitteeInfo>(&NextCommittee::<
				Runtime,
			>::hashed_key(
			)),
		];

		let mut by_validator = Vec::new();
		for member in committees.into_iter().flatten().flat_map(|info| info.committee) {
			let cross_chain = member.authority_id();
			let validator = crate::AccountId::from(cross_chain.clone());
			if !by_validator.iter().any(|(known, _)| known == &validator) {
				by_validator.push((validator, cross_chain));
			}
		}
		by_validator
	}

	/// Translates the committee and session keys, then seeds v2's `QueuedCommittee`.
	pub struct InnerAddBeefyToSessionKeys;

	impl UncheckedOnRuntimeUpgrade for InnerAddBeefyToSessionKeys {
		fn on_runtime_upgrade() -> Weight {
			let db = <Runtime as frame_system::Config>::DbWeight::get();
			// Three committee reads for the index, plus one read per `translate` below.
			let mut weight = db.reads(6);

			let cross_chain_keys = cross_chain_keys_by_validator();

			let mut translate = |translated: Option<_>| {
				if translated.is_some() {
					weight = weight.saturating_add(db.writes(1));
				}
			};
			translate(
				CurrentCommittee::<Runtime>::translate::<crate::LegacyCommitteeInfo, _>(|old| {
					old.map(upgrade_committee_info)
				})
				.expect("Decoding of the old value must succeed"),
			);
			translate(
				QueuedCommittee::<Runtime>::translate::<crate::LegacyCommitteeInfo, _>(|old| {
					old.map(upgrade_committee_info)
				})
				.expect("Decoding of the old value must succeed"),
			);
			translate(
				NextCommittee::<Runtime>::translate::<crate::LegacyCommitteeInfo, _>(|old| {
					old.map(upgrade_committee_info)
				})
				.expect("Decoding of the old value must succeed"),
			);

			// Count `NextKeys` entries via `iter_keys` (no value decode) so the weight is correct
			// while the on-chain bytes still use `LegacySessionKeys`.
			let validators = pallet_session::NextKeys::<Runtime>::iter_keys().count() as u64;
			pallet_session::Pallet::<Runtime>::upgrade_keys::<LegacySessionKeys, _>(
				|validator, old_keys| match cross_chain_keys
					.iter()
					.find(|(known, _)| known == &validator)
				{
					Some((_, cross_chain)) => upgrade_keys_with(old_keys, cross_chain.clone()),
					None => {
						log::warn!(
							target: "runtime::migration::add-beefy-session-keys",
							"No committee member matches session validator {validator:?}; \
							 its beefy key falls back to the aura placeholder. Such a validator \
							 is not a BEEFY authority, so the placeholder is never used to sign.",
						);
						old_keys.into()
					},
				},
			);
			let old_key_types =
				<LegacySessionKeys as sp_runtime::traits::OpaqueKeys>::key_ids().len() as u64;
			let new_key_types =
				<SessionKeys as sp_runtime::traits::OpaqueKeys>::key_ids().len() as u64;
			weight = weight.saturating_add(db.reads_writes(
				// One read per entry to count, then one per entry again during `translate`, plus
				// `QueuedKeys`.
				2 * validators + 1,
				validators * (1 + old_key_types + new_key_types) + 1,
			));

			// At v1 `QueuedCommittee` does not exist, so the translation above left it absent.
			// The v1 session integration applied committees immediately, making the current
			// committee both the active and the queued validator set — seed it from the
			// now-translated `CurrentCommittee`.
			QueuedCommittee::<Runtime>::put(CurrentCommittee::<Runtime>::get());

			weight.saturating_add(db.reads_writes(1, 1))
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

#[cfg(test)]
mod tests {
	use super::authority_keys::*;
	use crate::mock::{alice, bob, new_test_ext};
	use crate::{AccountId, LegacyCommitteeInfo, Runtime};
	use authority_selection_inherents::CommitteeMember;
	use frame_support::BoundedVec;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use pallet_session_validator_management::{CurrentCommittee, QueuedCommittee};
	use sidechain_domain::ScEpochNumber;
	use sp_core::Pair;
	use sp_session_validator_management::CommitteeMember as _;

	fn legacy_member(keys: &crate::mock::TestKeys) -> LegacyCommitteeMember {
		CommitteeMember::permissioned(
			keys.cross_chain.public(),
			LegacySessionKeys { aura: keys.aura.public(), grandpa: keys.grandpa.public() },
		)
	}

	#[test]
	fn migration_writes_the_cross_chain_key_as_the_beefy_key() {
		new_test_ext().execute_with(|| {
			let a = alice();
			let cross_chain = a.cross_chain.public();
			let validator = AccountId::from(cross_chain.clone());

			// Pre-upgrade state: legacy-shaped committee and session keys.
			let legacy = LegacyCommitteeInfo {
				epoch: ScEpochNumber(7),
				committee: BoundedVec::truncate_from(vec![legacy_member(&a)]),
			};
			frame_support::storage::unhashed::put(
				&CurrentCommittee::<Runtime>::hashed_key(),
				&legacy,
			);
			frame_support::storage::unhashed::put(
				&pallet_session::NextKeys::<Runtime>::hashed_key_for(&validator),
				&LegacySessionKeys { aura: a.aura.public(), grandpa: a.grandpa.public() },
			);

			InnerAddBeefyToSessionKeys::on_runtime_upgrade();

			let want: sp_consensus_beefy::ecdsa_crypto::AuthorityId =
				sp_core::ecdsa::Public::from(cross_chain.clone().into_inner()).into();

			let member = &CurrentCommittee::<Runtime>::get().committee[0];
			assert_eq!(member.authority_keys().beefy, want, "committee member beefy key");
			assert_eq!(member.authority_keys().aura, a.aura.public(), "aura preserved");

			let session_keys = pallet_session::NextKeys::<Runtime>::get(&validator)
				.expect("session keys translated");
			assert_eq!(session_keys.beefy, want, "pallet_session beefy key");

			assert_eq!(
				QueuedCommittee::<Runtime>::get().committee,
				CurrentCommittee::<Runtime>::get().committee,
				"queued seeded from current"
			);
		});
	}

	#[test]
	fn unresolvable_session_validator_keeps_the_placeholder() {
		new_test_ext().execute_with(|| {
			let a = alice();
			let stale = bob();
			let stale_validator = AccountId::from(stale.cross_chain.public());

			// Committee contains only alice; bob is a stale `NextKeys` entry. The genesis in
			// `new_test_ext` seeds alice *and* bob, so the other two committee storages are
			// cleared — otherwise bob stays resolvable through them.
			frame_support::storage::unhashed::put(
				&CurrentCommittee::<Runtime>::hashed_key(),
				&LegacyCommitteeInfo {
					epoch: ScEpochNumber(7),
					committee: BoundedVec::truncate_from(vec![legacy_member(&a)]),
				},
			);
			frame_support::storage::unhashed::kill(&QueuedCommittee::<Runtime>::hashed_key());
			frame_support::storage::unhashed::kill(
				&pallet_session_validator_management::NextCommittee::<Runtime>::hashed_key(),
			);
			frame_support::storage::unhashed::put(
				&pallet_session::NextKeys::<Runtime>::hashed_key_for(&stale_validator),
				&LegacySessionKeys { aura: stale.aura.public(), grandpa: stale.grandpa.public() },
			);

			InnerAddBeefyToSessionKeys::on_runtime_upgrade();

			let keys = pallet_session::NextKeys::<Runtime>::get(&stale_validator)
				.expect("stale entry still translated");
			let raw = keys.beefy.clone().into_inner().0;
			assert_eq!(raw[0], 0, "placeholder keeps the invalid SEC1 tag");
			assert_eq!(&raw[1..], &stale.aura.public().into_inner().0, "placeholder is aura bytes");
		});
	}
}

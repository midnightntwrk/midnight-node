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
//! Fixed, one-shot migrations usually live in a pallet's own `migrations` module and are wired
//! into `SingleBlockMigrations` or [`crate::Migrations`]. `authority_keys` below is the
//! exception: it is custom, runtime-specific logic for one combined translation that does not
//! belong in the generic pallet, and is only wired in for as long as that upgrade is relevant.

use frame_support::{
	migrations::{FailedMigrationHandler, FailedMigrationHandling, FreezeChainOnFailedMigration},
	traits::SafeMode as SafeModeTrait,
};

/// On a failed multi-block migration: enter safe mode indefinitely and force-unstuck the
/// migration cursor, so the chain keeps producing blocks (with user calls filtered) and
/// governance can ship a fixed runtime and `force_exit` safe mode.
///
/// Upstream's `FreezeChainOnFailedMigration` (and `EnterSafeModeOnFailedMigration`, which
/// falls back to `KeepStuck` — see paritytech/polkadot-sdk#12921) leave the cursor `Stuck`:
/// `MultiBlockMigrator::ongoing()` stays true forever, Executive admits only inherents, and
/// `frame_system::can_set_code` rejects upgrades — a permanent liveness failure with no
/// on-chain recovery on a standalone chain. Hence this custom handler.
pub struct EnterSafeModeAndUnstuckOnFailedMigration;
impl FailedMigrationHandler for EnterSafeModeAndUnstuckOnFailedMigration {
	fn failed(migration: Option<u32>) -> FailedMigrationHandling {
		// `enter(MAX)` saturates `EnteredUntil` to `BlockNumber::MAX`: safe mode never
		// auto-exits, only governance's `force_exit` lifts it.
		let entered = if crate::SafeMode::is_entered() {
			<crate::SafeMode as SafeModeTrait>::extend(crate::BlockNumber::MAX)
		} else {
			<crate::SafeMode as SafeModeTrait>::enter(crate::BlockNumber::MAX)
		};
		if entered.is_err() {
			// Fail closed: freezing is still safer than running unfiltered on half-migrated state.
			return FreezeChainOnFailedMigration::failed(migration);
		}
		log::error!(
			"Multi-block migration {migration:?} failed; entered safe mode and unstuck the \
			 cursor. Governance must ship a fixed runtime and force_exit safe mode."
		);
		FailedMigrationHandling::ForceUnstuck
	}
}

pub mod authority_keys {
	use crate::{CrossChainPublic, Runtime, opaque::SessionKeys};
	use alloc::vec::Vec;
	use authority_selection_inherents::CommitteeMember;
	use frame_support::{
		migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
	};
	use pallet_session_validator_management::migrations::authority_keys::UpgradeCommitteeMember;
	use pallet_session_validator_management::{
		CommitteeInfo, CurrentCommittee, NextCommittee, QueuedCommittee,
	};
	use parity_scale_codec::MaxEncodedLen;
	use sp_runtime::{impl_opaque_keys, traits::OpaqueKeys};
	use sp_session_validator_management::CommitteeMember as _;

	impl_opaque_keys! {
		#[derive(MaxEncodedLen, PartialOrd, Ord)]
		pub struct PreUpgradeSessionKeys {
			pub aura: crate::Aura,
			pub grandpa: crate::Grandpa,
		}
	}

	/// Fallback translation for a `pallet_session` entry whose validator is in none of the committees, so no
	/// cross-chain key is recoverable from AccountId.
	///
	/// Aura bytes behind the invalid SEC1 tag `0x00` keep the placeholder distinct from any real
	///
	/// key rather than colliding with one.
	/// BABE key is copied from AURA key.
	impl From<PreUpgradeSessionKeys> for SessionKeys {
		fn from(old: PreUpgradeSessionKeys) -> Self {
			let aura_raw = old.aura.clone().into_inner().0;
			let mut beefy_raw = [0u8; 33];
			beefy_raw[1..].copy_from_slice(&aura_raw);
			let beefy_from_aura = sp_core::ecdsa::Public::from_raw(beefy_raw).into();
			let babe_from_aura = old.aura.clone().into_inner().into();
			SessionKeys {
				aura: old.aura,
				babe: babe_from_aura,
				beefy: beefy_from_aura,
				grandpa: old.grandpa,
			}
		}
	}

	/// Builds the post-upgrade keys for a validator whose cross-chain key is known.
	///
	/// The committee registers each validator's cross-chain key as its beefy key
	/// (`beefy_pub_key == sidechain_pub_key`), and both are ECDSA, so the cross-chain key is the
	/// beefy key this validator actually holds a secret for.
	/// BABE key is copied from AURA key.
	fn upgrade_with_cross_chain(
		old: PreUpgradeSessionKeys,
		cross_chain: CrossChainPublic,
	) -> SessionKeys {
		let babe_from_aura = old.aura.clone().into_inner().into();
		SessionKeys {
			aura: old.aura,
			babe: babe_from_aura,
			beefy: sp_core::ecdsa::Public::from(cross_chain.into_inner()).into(),
			grandpa: old.grandpa,
		}
	}

	pub(crate) type PreUpgradeCommitteeMember =
		CommitteeMember<CrossChainPublic, PreUpgradeSessionKeys>;

	pub(crate) type PreUpgradeCommitteeInfo = CommitteeInfo<
		<Runtime as pallet_session_validator_management::Config>::ScEpochNumber,
		PreUpgradeCommitteeMember,
		<Runtime as pallet_session_validator_management::Config>::MaxValidators,
	>;

	fn upgrade_committee_info(
		old: PreUpgradeCommitteeInfo,
	) -> CommitteeInfo<
		<Runtime as pallet_session_validator_management::Config>::ScEpochNumber,
		<Runtime as pallet_session_validator_management::Config>::CommitteeMember,
		<Runtime as pallet_session_validator_management::Config>::MaxValidators,
	> {
		CommitteeInfo {
			epoch: old.epoch,
			committee: sp_runtime::BoundedVec::truncate_from(
				old.committee.into_iter().map(|m| m.upgrade()).collect(),
			),
		}
	}

	impl UpgradeCommitteeMember<Runtime> for PreUpgradeCommitteeMember {
		fn upgrade(
			self,
		) -> <Runtime as pallet_session_validator_management::Config>::CommitteeMember {
			// A committee member carries its own cross-chain key, so no lookup is needed here.
			let cross_chain = self.authority_id();
			self.map_authority_keys(|old| upgrade_with_cross_chain(old, cross_chain.clone()))
		}
	}

	/// Reads the still-legacy-shaped committees and indexes their members' cross-chain keys by the
	/// `pallet_session` validator id.
	///
	/// `pallet_session` keys validators by `AccountId`, which is `blake2_256` of the cross-chain
	/// key and therefore not invertible — so the mapping is rebuilt by hashing each committee
	/// member's `id` forward. The committees are the only on-chain source of these keys.
	fn cross_chain_keys_by_validator() -> Vec<(crate::AccountId, CrossChainPublic)> {
		cross_chain_keys_of(
			[
				frame_support::storage::unhashed::get::<PreUpgradeCommitteeInfo>(
					&CurrentCommittee::<Runtime>::hashed_key(),
				),
				frame_support::storage::unhashed::get::<PreUpgradeCommitteeInfo>(&NextCommittee::<
					Runtime,
				>::hashed_key(
				)),
			]
			.into_iter()
			.flatten(),
		)
	}

	/// Indexes the members' cross-chain keys of the given committees by `pallet_session` validator
	/// id. Shared by the migration and its try-runtime check so both translate the same way.
	fn cross_chain_keys_of(
		committees: impl IntoIterator<Item = PreUpgradeCommitteeInfo>,
	) -> Vec<(crate::AccountId, CrossChainPublic)> {
		let mut by_validator = Vec::new();
		for member in committees.into_iter().flat_map(|info| info.committee) {
			let cross_chain = member.authority_id();
			let validator = crate::AccountId::from(cross_chain.clone());
			if !by_validator.iter().any(|(known, _)| known == &validator) {
				by_validator.push((validator, cross_chain));
			}
		}
		by_validator
	}

	/// Translates one `pallet_session` entry: the cross-chain key becomes the beefy key when the
	/// validator is a known committee member, otherwise the placeholder from `From` applies.
	fn upgrade_session_keys(
		validator: &crate::AccountId,
		old_keys: PreUpgradeSessionKeys,
		cross_chain_keys: &[(crate::AccountId, CrossChainPublic)],
	) -> SessionKeys {
		match cross_chain_keys.iter().find(|(known, _)| known == validator) {
			Some((_, cross_chain)) => upgrade_with_cross_chain(old_keys, cross_chain.clone()),
			None => {
				log::warn!(
					target: "runtime::migration::add-beefy-session-keys",
					"No committee member matches session validator {validator:?}; \
					 its beefy key falls back to the aura placeholder. Such a validator \
					 is not a BEEFY authority, so the placeholder is never used to sign.",
				);
				old_keys.into()
			},
		}
	}

	/// `pallet-consensus-engine` by pre-seeding `pallet_babe::GenesisSlot`.
	///
	/// The activation lives here, gated by `pallet-session-validator-management`'s storage version
	/// (1 → 2), rather than in a versioned migration of `pallet-consensus-engine` itself: FRAME's
	/// `before_all_runtime_migrations` initializes a brand-new pallet's on-chain storage version to
	/// its in-code version before any migration runs, so a `VersionedMigration` keyed on the new
	/// pallet never fires. The committee pallet exists on every chain being upgraded, so its
	/// version transition is what identifies this upgrade exactly once.
	pub struct InnerMigrateV1ToV2AddBabeAndBeefySessionKeys;

	impl UncheckedOnRuntimeUpgrade for InnerMigrateV1ToV2AddBabeAndBeefySessionKeys {
		fn on_runtime_upgrade() -> Weight {
			log::info!("translating committee & session keys and initializing QueuedCommittee");
			let db = <Runtime as frame_system::Config>::DbWeight::get();
			let mut weight = db.reads_writes(3, 1);

			// Must happen before any `on_initialize` of this block: migration-aware authors already
			// attach the BABE pre-digest, and pallet-babe would otherwise self-initialize its genesis
			// epoch from it. Migrations run before all hooks, so this is early enough.
			pallet_consensus_engine::Pallet::<Runtime>::activate();
			weight = weight.saturating_add(db.writes(1));

			let cross_chain_keys = cross_chain_keys_by_validator();

			// `CurrentCommittee`/`NextCommittee` must be translated before anything reads them
			// typed as the post-BABE `CommitteeMember` — reading them with the new type first
			// would try to decode still-legacy bytes as the new shape and silently come back
			// empty (a `ValueQuery`/`OptionQuery` decode failure looks the same as "absent").
			let current_translated =
				CurrentCommittee::<Runtime>::translate::<PreUpgradeCommitteeInfo, _>(
					|old: Option<PreUpgradeCommitteeInfo>| old.map(upgrade_committee_info),
				)
				.expect("Decoding of the pre-upgrade CurrentCommittee must succeed");
			if current_translated.is_some() {
				weight = weight.saturating_add(db.writes(1));
			}

			let next_translated =
				NextCommittee::<Runtime>::translate::<PreUpgradeCommitteeInfo, _>(
					|old: Option<PreUpgradeCommitteeInfo>| old.map(upgrade_committee_info),
				)
				.expect("Decoding of the pre-upgrade NextCommittee must succeed");
			if next_translated.is_some() {
				weight = weight.saturating_add(db.writes(1));
			}

			// V1 chains never wrote `QueuedCommittee` (the v1 session integration applied
			// committees immediately, so `CurrentCommittee` was both the active and the queued
			// validator set). Seed it from the just-translated `CurrentCommittee` rather than
			// translating whatever old-shaped bytes might be there.
			QueuedCommittee::<Runtime>::put(CurrentCommittee::<Runtime>::get());

			// `upgrade_keys` translates the entire `NextKeys` map (1 read + 1 write per entry) and
			// rewrites `KeyOwner` for every old/new key type per entry (pure writes). `QueuedKeys`
			// is a single `StorageValue`, translated once.
			//
			// Count `NextKeys` entries via `iter_keys` (no value decode) so the weight is correct
			// even when on-chain bytes still use the `PreUpgradeSessionKeys` shape.
			// `register_committee_keys` only adds keys for committee members and never removes
			// them when a validator rotates out, so the map may contain stale entries beyond the
			// current/next committee union.
			let validators = pallet_session::NextKeys::<Runtime>::iter_keys().count() as u64;
			pallet_session::Pallet::<Runtime>::upgrade_keys::<PreUpgradeSessionKeys, _>(
				|validator, old_keys| upgrade_session_keys(&validator, old_keys, &cross_chain_keys),
			);
			let old_key_types = PreUpgradeSessionKeys::key_ids().len() as u64;
			let new_key_types = SessionKeys::key_ids().len() as u64;
			weight = weight.saturating_add(db.reads_writes(
				// One read per entry to count, then one per entry again during `translate`, plus
				// `QueuedKeys`.
				2 * validators + 1,
				validators * (1 + old_key_types + new_key_types) + 1,
			));

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			use parity_scale_codec::Encode;

			// `CurrentCommittee`/`NextCommittee` `.get()` decodes as the post-upgrade
			// `CommitteeMember` shape, but the on-chain bytes are still `LegacyCommitteeMember` —
			// so read them through `unhashed` with the old types. The same applies to
			// `pallet_session`'s `NextKeys`/`QueuedKeys`, which still hold `LegacySessionKeys`
			// bytes. `QueuedCommittee` does not exist at v1.
			let current: PreUpgradeCommitteeInfo = frame_support::storage::unhashed::get_or_default(
				&CurrentCommittee::<Runtime>::hashed_key(),
			);
			let next: Option<PreUpgradeCommitteeInfo> =
				frame_support::storage::unhashed::get(&NextCommittee::<Runtime>::hashed_key());

			let next_keys: Vec<(
				<Runtime as pallet_session::Config>::ValidatorId,
				PreUpgradeSessionKeys,
			)> = pallet_session::NextKeys::<Runtime>::iter_keys()
				.map(|validator| {
					let old_keys: PreUpgradeSessionKeys = frame_support::storage::unhashed::get(
						&pallet_session::NextKeys::<Runtime>::hashed_key_for(&validator),
					)
					.ok_or(sp_runtime::TryRuntimeError::Other(
						"session NextKeys entries must decode with the old keys type",
					))?;
					Ok((validator, old_keys))
				})
				.collect::<Result<_, sp_runtime::TryRuntimeError>>()?;

			// `QueuedKeys` is a `ValueQuery` storage: absent means empty.
			let queued_keys: Vec<(
				<Runtime as pallet_session::Config>::ValidatorId,
				PreUpgradeSessionKeys,
			)> =
				frame_support::storage::unhashed::get_or_default(&pallet_session::QueuedKeys::<
					Runtime,
				>::hashed_key());

			Ok((current, next, next_keys, queued_keys).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			use frame_support::ensure;
			use parity_scale_codec::{Decode, Encode};

			type ValidatorIdToPreUpgradeSessionKeys =
				Vec<(<Runtime as pallet_session::Config>::ValidatorId, PreUpgradeSessionKeys)>;

			let (old_current, old_next, old_next_keys, old_queued_keys): (
				PreUpgradeCommitteeInfo,
				Option<PreUpgradeCommitteeInfo>,
				ValidatorIdToPreUpgradeSessionKeys,
				ValidatorIdToPreUpgradeSessionKeys,
			) = Decode::decode(&mut state.as_slice()).map_err(|_| {
				sp_runtime::TryRuntimeError::Other("Previously encoded state should be decodable")
			})?;

			// The migration gives committee validators their cross-chain key as beefy key and only
			// falls back to the placeholder for validators in neither committee; the expected keys
			// below must follow the same rule, derived from the same pre-upgrade committees.
			let cross_chain_keys =
				cross_chain_keys_of(core::iter::once(old_current.clone()).chain(old_next.clone()));

			let new_current = CurrentCommittee::<Runtime>::get();
			ensure!(old_current.epoch == new_current.epoch, "current epoch should be preserved");
			ensure!(
				upgrade_committee_info(old_current).encode() == new_current.encode(),
				"current committee membership should be preserved"
			);

			let new_queued = QueuedCommittee::<Runtime>::get();
			ensure!(
				new_queued.encode() == new_current.encode(),
				"queued committee should be seeded from current committee"
			);

			let new_next = NextCommittee::<Runtime>::get();
			ensure!(
				old_next.is_some() == new_next.is_some(),
				"next committee presence should be preserved"
			);
			if let (Some(old_next), Some(new_next)) = (old_next, new_next) {
				ensure!(old_next.epoch == new_next.epoch, "next epoch should be preserved");
				ensure!(
					upgrade_committee_info(old_next).encode() == new_next.encode(),
					"next committee membership should be preserved"
				);
			}

			ensure!(
				pallet_session::NextKeys::<Runtime>::iter_keys().count() == old_next_keys.len(),
				"session NextKeys entry count should be preserved"
			);
			for (validator, old_keys) in old_next_keys {
				let expected_keys = upgrade_session_keys(&validator, old_keys, &cross_chain_keys);
				ensure!(
					pallet_session::NextKeys::<Runtime>::get(&validator)
						== Some(expected_keys.clone()),
					"session NextKeys should be upgraded in place"
				);
				// Covers every key type, BABE included; a key shared between validators leaves
				// `KeyOwner` pointing at only one of them.
				for key_type in SessionKeys::key_ids() {
					ensure!(
						pallet_session::KeyOwner::<Runtime>::get((
							*key_type,
							expected_keys.get_raw(*key_type).to_vec()
						)) == Some(validator.clone()),
						"KeyOwner should map each upgraded key back to its validator"
					);
				}
			}

			let expected_queued_keys: Vec<_> = old_queued_keys
				.into_iter()
				.map(|(v, keys)| {
					let upgraded = upgrade_session_keys(&v, keys, &cross_chain_keys);
					(v, upgraded)
				})
				.collect();
			ensure!(
				pallet_session::QueuedKeys::<Runtime>::get() == expected_queued_keys,
				"session QueuedKeys should be upgraded in place"
			);

			ensure!(
				frame_support::traits::StorageVersion::get::<
					pallet_session_validator_management::Pallet<Runtime>,
				>() == frame_support::traits::StorageVersion::new(2),
				"on-chain storage version should be 2"
			);

			ensure!(
				pallet_babe::GenesisSlot::<Runtime>::get()
					== pallet_consensus_engine::babe_genesis_slot_sentinel(),
				"pallet-babe GenesisSlot should hold the consensus-engine activation sentinel"
			);
			ensure!(
				pallet_consensus_engine::EngineState::<Runtime>::get()
					== pallet_consensus_engine::State::Aura,
				"consensus-engine should activate in state Aura"
			);

			Ok(())
		}
	}

	pub type MigrateV1ToV2AddBabeAndBeefySessionKeys = VersionedMigration<
		1,
		2,
		InnerMigrateV1ToV2AddBabeAndBeefySessionKeys,
		pallet_session_validator_management::Pallet<Runtime>,
		<Runtime as frame_system::Config>::DbWeight,
	>;
}

#[cfg(test)]
mod tests {
	use super::authority_keys::*;
	use crate::mock::{alice, bob, new_test_ext};
	use crate::{AccountId, Runtime, SessionCommitteeManagement};
	use authority_selection_inherents::CommitteeMember;
	use frame_support::BoundedVec;
	use frame_support::traits::{
		BeforeAllRuntimeMigrations, GetStorageVersion, OnRuntimeUpgrade, StorageVersion,
		UncheckedOnRuntimeUpgrade,
	};
	use pallet_consensus_engine::babe_genesis_slot_sentinel;
	use pallet_session_validator_management::{CurrentCommittee, QueuedCommittee};
	use sidechain_domain::ScEpochNumber;
	use sp_consensus_slots::Slot;
	use sp_core::Pair;
	use sp_session_validator_management::CommitteeMember as _;

	/// State of a chain about to take the upgrade: the committee pallet at storage version 1,
	/// `pallet-babe` and `pallet-consensus-engine` not present at all.
	fn pre_upgrade_ext() -> sp_io::TestExternalities {
		let mut ext = sp_io::TestExternalities::default();
		ext.execute_with(|| StorageVersion::new(1).put::<SessionCommitteeManagement>());
		ext
	}

	/// The activation must survive what FRAME does to a pallet that is new to the runtime: its
	/// on-chain storage version is initialized to the in-code one *before* migrations run, which
	/// is why a version-gated migration on `pallet-consensus-engine` itself could never fire.
	#[test]
	fn upgrade_from_v1_pre_seeds_the_babe_genesis_slot_sentinel() {
		pre_upgrade_ext().execute_with(|| {
			assert_eq!(pallet_babe::GenesisSlot::<Runtime>::get(), Slot::from(0));

			<crate::AllPalletsWithSystem as BeforeAllRuntimeMigrations>::before_all_runtime_migrations();
			assert_eq!(
				crate::ConsensusEngine::on_chain_storage_version(),
				crate::ConsensusEngine::in_code_storage_version(),
				"FRAME initializes the new pallet's version before any migration runs"
			);
			MigrateV1ToV2AddBabeAndBeefySessionKeys::on_runtime_upgrade();

			assert_eq!(pallet_babe::GenesisSlot::<Runtime>::get(), babe_genesis_slot_sentinel());
			assert_eq!(
				SessionCommitteeManagement::on_chain_storage_version(),
				StorageVersion::new(2)
			);
		});
	}

	fn pre_upgrade_member(keys: &crate::mock::TestKeys) -> PreUpgradeCommitteeMember {
		CommitteeMember::permissioned(
			keys.cross_chain.public(),
			PreUpgradeSessionKeys { aura: keys.aura.public(), grandpa: keys.grandpa.public() },
		)
	}

	#[test]
	fn migration_writes_the_cross_chain_key_as_the_beefy_key() {
		new_test_ext().execute_with(|| {
			let a = alice();
			let cross_chain = a.cross_chain.public();
			let validator = AccountId::from(cross_chain.clone());

			// Pre-upgrade state: committee and session keys.
			let legacy = PreUpgradeCommitteeInfo {
				epoch: ScEpochNumber(7),
				committee: BoundedVec::truncate_from(vec![pre_upgrade_member(&a)]),
			};
			frame_support::storage::unhashed::put(
				&CurrentCommittee::<Runtime>::hashed_key(),
				&legacy,
			);
			frame_support::storage::unhashed::put(
				&pallet_session::NextKeys::<Runtime>::hashed_key_for(&validator),
				&PreUpgradeSessionKeys { aura: a.aura.public(), grandpa: a.grandpa.public() },
			);

			InnerMigrateV1ToV2AddBabeAndBeefySessionKeys::on_runtime_upgrade();

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

	/// A chain that already has the committee pallet at v2 (started from genesis with this
	/// runtime, or already upgraded) is left alone: the migration does not run, so whatever
	/// `GenesisSlot` pallet-babe holds stays.
	#[test]
	fn upgrade_is_a_no_op_once_the_committee_pallet_is_at_v2() {
		sp_io::TestExternalities::default().execute_with(|| {
			StorageVersion::new(2).put::<SessionCommitteeManagement>();
			pallet_babe::GenesisSlot::<Runtime>::put(Slot::from(1500));

			MigrateV1ToV2AddBabeAndBeefySessionKeys::on_runtime_upgrade();

			assert_eq!(pallet_babe::GenesisSlot::<Runtime>::get(), Slot::from(1500));
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
				&PreUpgradeCommitteeInfo {
					epoch: ScEpochNumber(7),
					committee: BoundedVec::truncate_from(vec![pre_upgrade_member(&a)]),
				},
			);
			frame_support::storage::unhashed::kill(&QueuedCommittee::<Runtime>::hashed_key());
			frame_support::storage::unhashed::kill(
				&pallet_session_validator_management::NextCommittee::<Runtime>::hashed_key(),
			);
			frame_support::storage::unhashed::put(
				&pallet_session::NextKeys::<Runtime>::hashed_key_for(&stale_validator),
				&PreUpgradeSessionKeys {
					aura: stale.aura.public(),
					grandpa: stale.grandpa.public(),
				},
			);

			InnerMigrateV1ToV2AddBabeAndBeefySessionKeys::on_runtime_upgrade();

			let keys = pallet_session::NextKeys::<Runtime>::get(&stale_validator)
				.expect("stale entry still translated");
			let raw = keys.beefy.clone().into_inner().0;
			assert_eq!(raw[0], 0, "placeholder keeps the invalid SEC1 tag");
			assert_eq!(&raw[1..], &stale.aura.public().into_inner().0, "placeholder is aura bytes");
		});
	}

	/// The try-runtime checks must expect exactly what the migration writes: the cross-chain key
	/// as beefy key for committee validators, the placeholder only for validators in neither
	/// committee. A pre/post round trip over both kinds of entry pins that down.
	#[cfg(feature = "try-runtime")]
	#[test]
	fn try_runtime_checks_accept_the_migrated_state() {
		pre_upgrade_ext().execute_with(|| {
			let a = alice();
			let stale = bob();
			let a_validator = AccountId::from(a.cross_chain.public());
			let stale_validator = AccountId::from(stale.cross_chain.public());
			let a_keys =
				PreUpgradeSessionKeys { aura: a.aura.public(), grandpa: a.grandpa.public() };
			let stale_keys = PreUpgradeSessionKeys {
				aura: stale.aura.public(),
				grandpa: stale.grandpa.public(),
			};

			// Alice sits in the committee and is queued; bob is only a stale `NextKeys` entry.
			frame_support::storage::unhashed::put(
				&CurrentCommittee::<Runtime>::hashed_key(),
				&PreUpgradeCommitteeInfo {
					epoch: ScEpochNumber(7),
					committee: BoundedVec::truncate_from(vec![pre_upgrade_member(&a)]),
				},
			);
			frame_support::storage::unhashed::put(
				&pallet_session::NextKeys::<Runtime>::hashed_key_for(&a_validator),
				&a_keys,
			);
			frame_support::storage::unhashed::put(
				&pallet_session::NextKeys::<Runtime>::hashed_key_for(&stale_validator),
				&stale_keys,
			);
			frame_support::storage::unhashed::put(
				&pallet_session::QueuedKeys::<Runtime>::hashed_key(),
				&vec![(a_validator.clone(), a_keys)],
			);

			<crate::AllPalletsWithSystem as BeforeAllRuntimeMigrations>::before_all_runtime_migrations();
			let state = MigrateV1ToV2AddBabeAndBeefySessionKeys::pre_upgrade()
				.expect("pre_upgrade reads the legacy state");
			MigrateV1ToV2AddBabeAndBeefySessionKeys::on_runtime_upgrade();
			MigrateV1ToV2AddBabeAndBeefySessionKeys::post_upgrade(state)
				.expect("post_upgrade must accept what on_runtime_upgrade wrote");
		});
	}
}

pub mod beefy_genesis {
	//! One-shot reset of the BEEFY genesis block to `None`, which disables BEEFY.
	//!
	//! BEEFY has never produced a commitment on the live networks: the voter is stuck on the
	//! mandatory block of the genesis session (block 1), which can only be signed by the genesis
	//! authority set. Until this upgrade `beefy` was not a session key, so `pallet_beefy` never
	//! received session changes and `pallet_beefy::Authorities` stayed at the chain-spec set,
	//! whose keys were never in the validators' keystores. The gadget finalizes sessions strictly
	//! in order, so that block has to be signed before anything else can be.
	//!
	//! This upgrade also adds `beefy` to `SessionKeys` (see `authority_keys`), so from the next
	//! session rotation on, `pallet_beefy::Authorities` tracks the committee and every validator
	//! holds the key it is listed under. That does not unstick block 1 though: the session that
	//! starts at the old genesis still belongs to the chain-spec set.
	//!
	//! Clearing `pallet_beefy::GenesisBlock` makes `BeefyApi::beefy_genesis` return `None`. The
	//! client gadget treats that as "pallet not available": `wait_for_runtime_pallet` keeps
	//! waiting and the voter never starts. BEEFY stays disabled until governance re-enables it
	//! with `pallet_beefy::Pallet::set_new_genesis`, which places a fresh genesis in the future
	//! and lets the voters start a first session there with the validator set active at that
	//! block. Do that only after at least one session rotation has followed this upgrade, so the
	//! set active at the new genesis is the committee and not the chain-spec set.
	//!
	//! Note: a voter that is already running with persisted state only detects a `ConsensusReset`
	//! when the genesis changes to a *different* `Some` value; `None` is ignored by
	//! `handle_finality_notification`. Such a voter keeps its stuck state until the node restarts,
	//! after which it idles like a fresh one. No commitment can be produced either way.
	//!
	//! The migration is a no-op once the value is `None`. Remove it from [`crate::Migrations`]
	//! once the upgrade has landed on all live networks, and in any case before BEEFY is
	//! re-enabled with `set_new_genesis`, otherwise the next upgrade disables it again.
	use crate::Runtime;
	use frame_support::{pallet_prelude::*, traits::OnRuntimeUpgrade};

	#[cfg(feature = "try-runtime")]
	use alloc::vec::Vec;
	#[cfg(feature = "try-runtime")]
	use parity_scale_codec::Encode;

	pub struct ResetBeefyGenesis;

	impl OnRuntimeUpgrade for ResetBeefyGenesis {
		fn on_runtime_upgrade() -> Weight {
			let current = pallet_beefy::GenesisBlock::<Runtime>::get();
			if current.is_none() {
				log::info!("BEEFY genesis is already unset, leaving it untouched");
				return <Runtime as frame_system::Config>::DbWeight::get().reads(1);
			}

			pallet_beefy::GenesisBlock::<Runtime>::put(None::<crate::BlockNumber>);
			log::info!("BEEFY genesis reset from {current:?} to None, BEEFY is disabled");

			<Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, 1)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok(pallet_beefy::GenesisBlock::<Runtime>::get().encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			frame_support::ensure!(
				pallet_beefy::GenesisBlock::<Runtime>::get().is_none(),
				"BEEFY genesis must be None after the upgrade"
			);
			Ok(())
		}
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use crate::BlockNumber;

		fn db_weight() -> frame_support::weights::RuntimeDbWeight {
			<Runtime as frame_system::Config>::DbWeight::get()
		}

		#[test]
		fn resets_the_chain_spec_genesis_to_none() {
			sp_io::TestExternalities::default().execute_with(|| {
				pallet_beefy::GenesisBlock::<Runtime>::put(Some(1));

				let weight = ResetBeefyGenesis::on_runtime_upgrade();

				assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
				assert_eq!(weight, db_weight().reads_writes(1, 1));
			});
		}

		#[test]
		fn resets_a_previously_moved_genesis_to_none() {
			sp_io::TestExternalities::default().execute_with(|| {
				pallet_beefy::GenesisBlock::<Runtime>::put(Some(1600));
				ResetBeefyGenesis::on_runtime_upgrade();
				assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
			});
		}

		#[test]
		fn leaves_an_unset_genesis_untouched() {
			sp_io::TestExternalities::default().execute_with(|| {
				pallet_beefy::GenesisBlock::<Runtime>::put(None::<BlockNumber>);

				let weight = ResetBeefyGenesis::on_runtime_upgrade();

				assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
				assert_eq!(weight, db_weight().reads(1));
			});
		}
	}
}

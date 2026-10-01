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
	use pallet_session_validator_management::{
		CommitteeInfo, CurrentCommittee, NextCommittee, QueuedCommittee,
	};
	use parity_scale_codec::MaxEncodedLen;
	use sp_runtime::{impl_opaque_keys, traits::OpaqueKeys};

	impl_opaque_keys! {
		#[derive(MaxEncodedLen, PartialOrd, Ord)]
		pub struct PreUpgradeSessionKeys {
			pub aura: crate::Aura,
			pub grandpa: crate::Grandpa,
		}
	}

	impl From<PreUpgradeSessionKeys> for SessionKeys {
		fn from(old: PreUpgradeSessionKeys) -> Self {
			// BEEFY logic will go in here and it will look into storages to dig out matching key
			let babe_from_aura = old.aura.clone().into_inner().into();
			SessionKeys { aura: old.aura, grandpa: old.grandpa, babe: babe_from_aura }
		}
	}

	type PreUpgradeCommitteeMember = CommitteeMember<CrossChainPublic, PreUpgradeSessionKeys>;

	type PreUpgradeCommitteeInfo = CommitteeInfo<
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
				old.committee.into_iter().map(|m| m.map_authority_keys(Into::into)).collect(),
			),
		}
	}

	/// The one-shot "introduce BABE" step of the runtime upgrade: translates the committee and
	/// session keys to the shape that includes the BABE key, and activates
	/// `pallet-consensus-engine` by pre-seeding `pallet_babe::GenesisSlot`.
	///
	/// The activation lives here, gated by `pallet-session-validator-management`'s storage version
	/// (1 → 2), rather than in a versioned migration of `pallet-consensus-engine` itself: FRAME's
	/// `before_all_runtime_migrations` initializes a brand-new pallet's on-chain storage version to
	/// its in-code version before any migration runs, so a `VersionedMigration` keyed on the new
	/// pallet never fires. The committee pallet exists on every chain being upgraded, so its
	/// version transition is what identifies this upgrade exactly once.
	pub struct InnerMigrateV1ToV2AddBabeSessionKeys;

	impl UncheckedOnRuntimeUpgrade for InnerMigrateV1ToV2AddBabeSessionKeys {
		fn on_runtime_upgrade() -> Weight {
			log::info!("translating committee & session keys and initializing QueuedCommittee");
			let db = <Runtime as frame_system::Config>::DbWeight::get();
			let mut weight = db.reads_writes(3, 1);

			// Must happen before any `on_initialize` of this block: migration-aware authors already
			// attach the BABE pre-digest, and pallet-babe would otherwise self-initialize its genesis
			// epoch from it. Migrations run before all hooks, so this is early enough.
			pallet_consensus_engine::Pallet::<Runtime>::activate();
			weight = weight.saturating_add(db.writes(1));

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
			pallet_session::Pallet::<Runtime>::upgrade_keys(
				|_id, old_keys: PreUpgradeSessionKeys| old_keys.into(),
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
				let expected_keys: SessionKeys = old_keys.into();
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
				.map(|(v, keys)| (v, SessionKeys::from(keys)))
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

	pub type MigrateV1ToV2AddBabeSessionKeys = VersionedMigration<
		1,
		2,
		InnerMigrateV1ToV2AddBabeSessionKeys,
		pallet_session_validator_management::Pallet<Runtime>,
		<Runtime as frame_system::Config>::DbWeight,
	>;
}

#[cfg(test)]
mod tests {
	use super::authority_keys::MigrateV1ToV2AddBabeSessionKeys;
	use crate::{Runtime, SessionCommitteeManagement};
	use frame_support::traits::{
		BeforeAllRuntimeMigrations, GetStorageVersion, OnRuntimeUpgrade, StorageVersion,
	};
	use pallet_consensus_engine::babe_genesis_slot_sentinel;
	use sp_consensus_slots::Slot;

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
			MigrateV1ToV2AddBabeSessionKeys::on_runtime_upgrade();

			assert_eq!(pallet_babe::GenesisSlot::<Runtime>::get(), babe_genesis_slot_sentinel());
			assert_eq!(
				SessionCommitteeManagement::on_chain_storage_version(),
				StorageVersion::new(2)
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

			MigrateV1ToV2AddBabeSessionKeys::on_runtime_upgrade();

			assert_eq!(pallet_babe::GenesisSlot::<Runtime>::get(), Slot::from(1500));
		});
	}
}

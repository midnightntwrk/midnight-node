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

pub mod session_pallet_swap {
	//! The swap to stock `pallet_session` (#1800, #1802) left the `Session` prefix at
	//! storage version 0 while the pallet declares 1. Remove once the upgrade carrying
	//! this has landed everywhere.
	//!
	//! `pallet_session::historical`, new in the same swap, needs no counterpart: its
	//! prefix holds no keys on any live network, so `BeforeAllRuntimeMigrations`
	//! initializes its version for us.

	use crate::Runtime;

	/// Converts `DisabledValidators` to the v1 layout — a pure version bump here,
	/// where the list is unset on every live network.
	pub type SessionV0ToV1 = pallet_session::migrations::v1::MigrateV0ToV1<
		Runtime,
		pallet_session::migrations::v1::InitOffenceSeverity<Runtime>,
	>;
}

pub mod babe_epoch_config {
	//! Chains that gained pallet-babe by upgrade (#1865) rather than at genesis have no
	//! `EpochConfig`: `genesis_build` writes it, nothing else does. That fails babe's
	//! `try_state` and would panic the pallet on the AURA->BABE flip.

	use frame_support::traits::OnRuntimeUpgrade;
	use frame_support::weights::Weight;

	use crate::{BABE_GENESIS_EPOCH_CONFIG, Runtime};

	pub struct InitBabeEpochConfig;

	impl OnRuntimeUpgrade for InitBabeEpochConfig {
		fn on_runtime_upgrade() -> Weight {
			if pallet_babe::EpochConfig::<Runtime>::get().is_none() {
				pallet_babe::EpochConfig::<Runtime>::put(BABE_GENESIS_EPOCH_CONFIG);
				log::info!("🚚 Babe::EpochConfig initialized to BABE_GENESIS_EPOCH_CONFIG");
				<Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, 1)
			} else {
				<Runtime as frame_system::Config>::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			frame_support::ensure!(
				pallet_babe::EpochConfig::<Runtime>::get().is_some(),
				"Babe::EpochConfig must be set after the upgrade"
			);
			Ok(())
		}
	}
}

pub mod bridge_reserve_validator {
	//! #1513 added `reserve_validator_address` to `MainChainScripts` without a migration,
	//! so the stored 3-field value no longer decodes — silently, since the item is
	//! `OptionQuery`. Re-encode it with the new field empty; the real address is then set
	//! via `set_main_chain_scripts`. Remove once this has landed everywhere.
	//!
	//! The bridge pallet has no storage version, so `VersionedMigration` would push the
	//! on-chain version past the in-code 0. Idempotency comes from `decode_all` instead:
	//! legacy bytes run out early for the new type, new bytes leave a remainder for the
	//! legacy one, so each decodes as exactly one layout.

	use frame_support::traits::OnRuntimeUpgrade;
	use frame_support::weights::Weight;
	use parity_scale_codec::{Decode, DecodeAll, Encode};
	use sidechain_domain::{AssetName, MainchainAddress, PolicyId};
	use sp_partner_chains_bridge::MainChainScripts;

	use crate::Runtime;

	type StoredScripts = pallet_partner_chains_bridge::MainChainScriptsConfiguration<Runtime>;

	/// Raw access, because the point is to read bytes the storage item's own type can
	/// no longer decode.
	pub(crate) fn stored_scripts_key() -> alloc::vec::Vec<u8> {
		use frame_support::storage::generator::StorageValue as _;
		StoredScripts::storage_value_final_key().to_vec()
	}

	/// `MainChainScripts` as encoded before #1513.
	#[derive(Decode)]
	struct LegacyMainChainScripts {
		token_policy_id: PolicyId,
		token_asset_name: AssetName,
		illiquid_circulation_supply_validator_address: MainchainAddress,
	}

	impl From<LegacyMainChainScripts> for MainChainScripts {
		fn from(old: LegacyMainChainScripts) -> Self {
			MainChainScripts {
				token_policy_id: old.token_policy_id,
				token_asset_name: old.token_asset_name,
				illiquid_circulation_supply_validator_address: old
					.illiquid_circulation_supply_validator_address,
				reserve_validator_address: MainchainAddress::default(),
			}
		}
	}

	pub struct MigrateMainChainScripts;

	impl OnRuntimeUpgrade for MigrateMainChainScripts {
		fn on_runtime_upgrade() -> Weight {
			use frame_support::storage::unhashed;

			let key = stored_scripts_key();
			let Some(raw) = unhashed::get_raw(&key) else {
				return <Runtime as frame_system::Config>::DbWeight::get().reads(1);
			};
			if MainChainScripts::decode_all(&mut &raw[..]).is_ok() {
				// Already in the post-#1513 layout.
				return <Runtime as frame_system::Config>::DbWeight::get().reads(1);
			}
			match LegacyMainChainScripts::decode_all(&mut &raw[..]) {
				Ok(legacy) => {
					unhashed::put_raw(&key, &MainChainScripts::from(legacy).encode());
					log::info!(
						"🚚 Bridge::MainChainScriptsConfiguration migrated to the post-#1513 layout"
					);
					<Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, 1)
				},
				Err(_) => {
					log::error!(
						"Bridge::MainChainScriptsConfiguration is neither legacy nor current layout; \
						 leaving untouched"
					);
					<Runtime as frame_system::Config>::DbWeight::get().reads(1)
				},
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			use frame_support::storage::unhashed;

			let key = stored_scripts_key();
			if let Some(raw) = unhashed::get_raw(&key) {
				frame_support::ensure!(
					MainChainScripts::decode_all(&mut &raw[..]).is_ok(),
					"Bridge::MainChainScriptsConfiguration must decode as MainChainScripts after migration"
				);
			}
			Ok(())
		}
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

#[cfg(test)]
mod tests {
	use frame_support::storage::unhashed;
	use frame_support::traits::OnRuntimeUpgrade;
	use parity_scale_codec::{DecodeAll, Encode};
	use sidechain_domain::{AssetName, MainchainAddress, PolicyId};
	use sp_partner_chains_bridge::MainChainScripts;
	use std::str::FromStr;

	use super::{babe_epoch_config::InitBabeEpochConfig, bridge_reserve_validator};
	use crate::{BABE_GENESIS_EPOCH_CONFIG, Runtime};

	/// The value mainnet actually holds: policy id, `NIGHT`, illiquid supply validator
	/// address, and no fourth field.
	const MAINNET_LEGACY_SCRIPTS: &str = concat!(
		"0691b2fecca1ac4f53cb6dfb00b7013e561d1f34403b957cbb5af1fa144e49474854e861646472317779637a66707866",
		"6e663568767033366d726e3635357965346b3263776c75766c657a36706878386a7834366b3673327474646171",
	);

	fn legacy_bytes() -> Vec<u8> {
		(0..MAINNET_LEGACY_SCRIPTS.len())
			.step_by(2)
			.map(|i| u8::from_str_radix(&MAINNET_LEGACY_SCRIPTS[i..i + 2], 16).unwrap())
			.collect()
	}

	fn scripts_key() -> Vec<u8> {
		bridge_reserve_validator::stored_scripts_key()
	}

	#[test]
	fn legacy_scripts_do_not_decode_as_the_current_type() {
		assert!(MainChainScripts::decode_all(&mut &legacy_bytes()[..]).is_err());
	}

	#[test]
	fn bridge_scripts_migration_defaults_the_new_field() {
		sp_io::TestExternalities::default().execute_with(|| {
			unhashed::put_raw(&scripts_key(), &legacy_bytes());

			bridge_reserve_validator::MigrateMainChainScripts::on_runtime_upgrade();

			let raw = unhashed::get_raw(&scripts_key()).unwrap();
			let migrated = MainChainScripts::decode_all(&mut &raw[..]).unwrap();
			assert_eq!(migrated.token_asset_name, AssetName(b"NIGHT".to_vec().try_into().unwrap()));
			assert_eq!(migrated.token_policy_id.0.len(), 28);
			assert_eq!(
				migrated.illiquid_circulation_supply_validator_address,
				MainchainAddress::from_str(
					"addr1wyczfpxfnf5hvp36mrn655ye4k2cwluvlez6phx8jx46k6s2ttdaq"
				)
				.unwrap()
			);
			assert_eq!(migrated.reserve_validator_address, MainchainAddress::default());
		});
	}

	#[test]
	fn bridge_scripts_migration_is_idempotent() {
		sp_io::TestExternalities::default().execute_with(|| {
			unhashed::put_raw(&scripts_key(), &legacy_bytes());
			bridge_reserve_validator::MigrateMainChainScripts::on_runtime_upgrade();
			let once = unhashed::get_raw(&scripts_key()).unwrap();

			bridge_reserve_validator::MigrateMainChainScripts::on_runtime_upgrade();
			assert_eq!(unhashed::get_raw(&scripts_key()).unwrap(), once);
		});
	}

	#[test]
	fn bridge_scripts_migration_leaves_a_current_layout_value_alone() {
		sp_io::TestExternalities::default().execute_with(|| {
			let current = MainChainScripts {
				token_policy_id: PolicyId([7u8; 28]),
				token_asset_name: AssetName::empty(),
				illiquid_circulation_supply_validator_address: MainchainAddress::from_str("addr1a")
					.unwrap(),
				reserve_validator_address: MainchainAddress::from_str("addr1b").unwrap(),
			};
			unhashed::put_raw(&scripts_key(), &current.encode());

			bridge_reserve_validator::MigrateMainChainScripts::on_runtime_upgrade();

			let raw = unhashed::get_raw(&scripts_key()).unwrap();
			assert_eq!(MainChainScripts::decode_all(&mut &raw[..]).unwrap(), current);
		});
	}

	#[test]
	fn bridge_scripts_migration_is_a_noop_when_unset() {
		sp_io::TestExternalities::default().execute_with(|| {
			bridge_reserve_validator::MigrateMainChainScripts::on_runtime_upgrade();
			assert!(unhashed::get_raw(&scripts_key()).is_none());
		});
	}

	#[test]
	fn babe_epoch_config_is_initialized_once() {
		sp_io::TestExternalities::default().execute_with(|| {
			assert!(pallet_babe::EpochConfig::<Runtime>::get().is_none());

			InitBabeEpochConfig::on_runtime_upgrade();
			assert_eq!(pallet_babe::EpochConfig::<Runtime>::get(), Some(BABE_GENESIS_EPOCH_CONFIG));

			// A chain initialized at genesis keeps what it has.
			let custom = sp_consensus_babe::BabeEpochConfiguration {
				c: (3, 4),
				allowed_slots: sp_consensus_babe::AllowedSlots::PrimarySlots,
			};
			pallet_babe::EpochConfig::<Runtime>::put(custom.clone());
			InitBabeEpochConfig::on_runtime_upgrade();
			assert_eq!(pallet_babe::EpochConfig::<Runtime>::get(), Some(custom));
		});
	}

	#[test]
	fn scripts_key_matches_the_on_chain_key() {
		let expected = [
			sp_io::hashing::twox_128(b"Bridge"),
			sp_io::hashing::twox_128(b"MainChainScriptsConfiguration"),
		]
		.concat();
		assert_eq!(scripts_key(), expected);
	}
}

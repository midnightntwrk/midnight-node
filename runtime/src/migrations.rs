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
	use pallet_session_validator_management::migrations::authority_keys::{
		AuthorityKeysMigration, UpgradeCommitteeMember,
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

	// Trait bounds are not enforced on type aliases, so instantiating a bounded function is
	// needed to actually prove at compile time that the scaffolding above satisfies the
	// migration's requirements (`Keys = AuthorityKeys`, key types convertible, etc.).
	#[allow(dead_code)]
	fn assert_migration_is_wirable() {
		fn assert_impls_on_runtime_upgrade<M: frame_support::traits::OnRuntimeUpgrade>() {}
		assert_impls_on_runtime_upgrade::<
			AuthorityKeysMigration<Runtime, LegacyCommitteeMember, LegacySessionKeys, 2, 3>,
		>();
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

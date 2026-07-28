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

pub mod session_pallet_swap {
	//! Storage-version alignment for the swap from `pallet-partner-chains-session` to stock
	//! `pallet_session` + `pallet_session::historical` (#1800, #1802).
	//!
	//! The old partner-chains session pallet left the `Session` prefix at storage version 0,
	//! and `Historical` did not exist on-chain at all. Stock `pallet_session` declares in-code
	//! version 1 (v1 changed the `DisabledValidators` layout) and `pallet_session::historical`
	//! declares in-code version 1 (v1 moved its storage out of the `Session` prefix). Without
	//! these migrations the first upgrade to a runtime containing the stock pallets leaves the
	//! on-chain versions behind the in-code ones (caught by `try-runtime` dry-runs, and would
	//! silently mislead any future version-gated migration).
	//!
	//! Neither migration moves data on Midnight networks: `Session::DisabledValidators` is
	//! empty on all live chains, and there never was any `Session`-prefixed historical data.
	//! Both are `VersionedMigration`s, so they no-op once the versions match. Remove after the
	//! upgrade carrying them has landed on all live networks.

	use frame_support::migrations::VersionedMigration;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;

	use crate::Runtime;

	/// Bumps `Session` from 0 to 1, converting `DisabledValidators` to the v1 layout
	/// (a pure version bump on Midnight networks, where the list is empty).
	pub type SessionV0ToV1 = pallet_session::migrations::v1::MigrateV0ToV1<
		Runtime,
		pallet_session::migrations::v1::InitOffenceSeverity<Runtime>,
	>;

	/// Inner no-op for [`HistoricalInitV1`]; all trait methods keep their defaults.
	pub struct NoopMigration;
	impl UncheckedOnRuntimeUpgrade for NoopMigration {}

	/// Initializes `Historical`'s storage version to its in-code value (1). The pallet is new
	/// to this runtime, so there is nothing to migrate — upstream v1 only moved data out of
	/// the `Session` prefix, which Midnight chains never wrote.
	pub type HistoricalInitV1 = VersionedMigration<
		0,
		1,
		NoopMigration,
		crate::Historical,
		<Runtime as frame_system::Config>::DbWeight,
	>;
}

pub mod babe_epoch_config {
	//! Initializes `Babe::EpochConfig` on chains that gained pallet-babe via runtime
	//! upgrade (#1865) rather than at genesis.
	//!
	//! `genesis_build` writes `EpochConfig` for new chains, but nothing does so on upgrade,
	//! leaving it `None` — which fails babe's `try_state` and would panic the pallet if BABE
	//! ever activated (AURA→BABE flip). Write [`crate::BABE_GENESIS_EPOCH_CONFIG`], exactly
	//! what genesis would have stored. Idempotent: only writes when the value is unset, and
	//! is a no-op on chains initialized at genesis.

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
	//! One-shot migration for `Bridge::MainChainScriptsConfiguration` (#1513).
	//!
	//! #1513 added `reserve_validator_address` to `MainChainScripts` without a storage
	//! migration, so values written by earlier runtimes fail to decode (caught by the
	//! `try-runtime` dry-run on every live network). This migration re-encodes the legacy
	//! 3-field value with the new field defaulted to the empty address. The real reserve
	//! validator address must then be set via the `set_main_chain_scripts` extrinsic; until
	//! then the observation layer sees no reserve UTXOs, matching pre-#1513 behaviour.
	//!
	//! The bridge pallet declares no storage version, so `VersionedMigration` would leave
	//! the on-chain version (bumped) ahead of the in-code one (0) and trip the try-runtime
	//! version assert. Idempotency comes from `decode_all` instead: the legacy layout is a
	//! strict prefix of the new one, so a value in either layout `decode_all`s exclusively
	//! as that layout (legacy bytes exhaust too early for the new type; new bytes leave a
	//! remainder for the legacy type). Remove once the upgrade has landed on all live
	//! networks.

	use frame_support::traits::OnRuntimeUpgrade;
	use frame_support::weights::Weight;
	use parity_scale_codec::{Decode, DecodeAll, Encode};
	use sidechain_domain::{AssetName, MainchainAddress, PolicyId};
	use sp_partner_chains_bridge::MainChainScripts;

	use crate::Runtime;

	type StoredScripts = pallet_partner_chains_bridge::MainChainScriptsConfiguration<Runtime>;

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
			use frame_support::storage::{generator::StorageValue as _, unhashed};

			let key = StoredScripts::storage_value_final_key();
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
			use frame_support::storage::{generator::StorageValue as _, unhashed};

			let key = StoredScripts::storage_value_final_key();
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

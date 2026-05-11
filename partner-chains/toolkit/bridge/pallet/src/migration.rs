//! Storage migrations for the bridge pallet.
//!
//! Migration `v0 -> v1` adds the `reserve_validator_address` field to
//! [`sp_partner_chains_bridge::MainChainScripts`]. Existing on-chain values are
//! encoded against the pre-migration shape and would silently decode to `None`
//! without translation; this module re-decodes them as the legacy struct and
//! re-encodes them with `reserve_validator_address` set to
//! [`MainchainAddress::default()`]. The actual reserve address is expected to be
//! populated later via a runtime call rather than at upgrade time.

use crate::{MainChainScriptsConfiguration, Pallet, pallet::Config};
use frame_support::{pallet_prelude::*, traits::OnRuntimeUpgrade};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sidechain_domain::{AssetName, MainchainAddress, PolicyId};
use sp_partner_chains_bridge::MainChainScripts;

/// On-chain `MainChainScripts` shape as it existed at storage version 0,
/// kept here purely to drive the v0 -> v1 decode step of the migration.
#[derive(Decode, Encode, TypeInfo)]
struct MainChainScriptsV0 {
	token_policy_id: PolicyId,
	token_asset_name: AssetName,
	illiquid_circulation_supply_validator_address: MainchainAddress,
}

/// Migrates [`MainChainScriptsConfiguration`] from the v0 shape (3 fields) to
/// the v1 shape (4 fields), filling `reserve_validator_address` from the
/// runtime-supplied `ReserveValidatorAddress` getter.
///
/// If on-chain storage is already at version >= 1, the migration is a no-op.
pub struct MigrateMainChainScriptsToV1<T>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateMainChainScriptsToV1<T> {
	fn on_runtime_upgrade() -> Weight {
		let on_chain = Pallet::<T>::on_chain_storage_version();
		if on_chain >= 1 {
			return T::DbWeight::get().reads(1);
		}

		let _ =
			MainChainScriptsConfiguration::<T>::translate::<MainChainScriptsV0, _>(|maybe_old| {
				maybe_old.map(|old| MainChainScripts {
					token_policy_id: old.token_policy_id,
					token_asset_name: old.token_asset_name,
					illiquid_circulation_supply_validator_address: old
						.illiquid_circulation_supply_validator_address,
					reserve_validator_address: MainchainAddress::default(),
				})
			});

		StorageVersion::new(1).put::<Pallet<T>>();

		// 1 read for the version check + 1 read of the legacy value + 1 write of the migrated
		// value + 1 write of the new storage version.
		T::DbWeight::get().reads_writes(2, 2)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{Test, new_test_ext};
	use core::str::FromStr;
	use frame_support::storage::{storage_prefix, unhashed};
	use sidechain_domain::{AssetName, PolicyId};
	use sp_core::bounded_vec;

	type Migration = MigrateMainChainScriptsToV1<Test>;

	fn legacy_scripts() -> MainChainScriptsV0 {
		MainChainScriptsV0 {
			token_policy_id: PolicyId([7; 28]),
			token_asset_name: AssetName(bounded_vec![3; 4]),
			illiquid_circulation_supply_validator_address: MainchainAddress::from_str("ics addr")
				.unwrap(),
		}
	}

	fn write_v0(value: &MainChainScriptsV0) {
		// Use the same final key the pallet storage uses for `MainChainScriptsConfiguration`,
		// then write raw v0-encoded bytes.
		let key = storage_prefix(b"Bridge", b"MainChainScriptsConfiguration");
		unhashed::put_raw(&key, &value.encode());
	}

	#[test]
	fn migrates_existing_v0_value_to_v1_filling_reserve_address() {
		new_test_ext().execute_with(|| {
			StorageVersion::new(0).put::<Pallet<Test>>();
			let v0 = legacy_scripts();
			write_v0(&v0);

			let _ = Migration::on_runtime_upgrade();

			let migrated = MainChainScriptsConfiguration::<Test>::get()
				.expect("scripts should still be present");
			assert_eq!(migrated.token_policy_id, v0.token_policy_id);
			assert_eq!(migrated.token_asset_name, v0.token_asset_name);
			assert_eq!(
				migrated.illiquid_circulation_supply_validator_address,
				v0.illiquid_circulation_supply_validator_address,
			);
			assert_eq!(migrated.reserve_validator_address, MainchainAddress::default());
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(1));
		});
	}

	#[test]
	fn migration_is_noop_when_storage_unset() {
		new_test_ext().execute_with(|| {
			StorageVersion::new(0).put::<Pallet<Test>>();

			let _ = Migration::on_runtime_upgrade();

			assert!(MainChainScriptsConfiguration::<Test>::get().is_none());
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(1));
		});
	}

	#[test]
	fn migration_is_noop_when_already_at_v1() {
		new_test_ext().execute_with(|| {
			StorageVersion::new(1).put::<Pallet<Test>>();
			let v0 = legacy_scripts();
			write_v0(&v0);

			let _ = Migration::on_runtime_upgrade();

			// Storage still holds v0-encoded bytes; reading as v1 fails -> None.
			assert!(MainChainScriptsConfiguration::<Test>::get().is_none());
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(1));
		});
	}

	// Reference: without the migration, an existing v0 value silently decodes to None.
	#[test]
	fn without_migration_existing_v0_value_decodes_to_none() {
		new_test_ext().execute_with(|| {
			write_v0(&legacy_scripts());
			assert!(MainChainScriptsConfiguration::<Test>::get().is_none());
		});
	}
}

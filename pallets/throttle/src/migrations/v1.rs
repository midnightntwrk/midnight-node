// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage migration from v0 to v1.
//!
//! Clears `AccountUsage` after its schema changed from a 2-field tuple to the
//! 3-field `UsageStats` struct. The data is rolling throttle state, so it does
//! not need to be preserved across the upgrade.

#[cfg(feature = "try-runtime")]
extern crate alloc;

use crate::{AccountUsage, Pallet, pallet::Config};
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// [`UncheckedOnRuntimeUpgrade`] implementation wrapped by [`MigrateV0ToV1`].
pub struct InnerMigrateV0ToV1<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T> {
	fn on_runtime_upgrade() -> Weight {
		let result = AccountUsage::<T>::clear(u32::MAX, None);

		T::DbWeight::get().reads_writes(result.unique as u64, result.unique as u64)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			AccountUsage::<T>::iter().next().is_none(),
			"throttle account usage must be cleared"
		);
		Ok(())
	}
}

/// Clears legacy throttle account usage and bumps pallet storage v0 to v1.
pub type MigrateV0ToV1<T> = VersionedMigration<
	0,
	1,
	InnerMigrateV0ToV1<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(any(all(feature = "try-runtime", test), doc))]
mod test {
	use super::*;
	use crate::mock::{Test, new_test_ext};
	use frame_support::{assert_ok, weights::RuntimeDbWeight};
	use parity_scale_codec::Encode;

	#[test]
	fn clears_legacy_account_usage() {
		new_test_ext().execute_with(|| {
			let key = AccountUsage::<Test>::hashed_key_for(1u64);
			// Legacy v0 encoded AccountUsage as `(bytes_used, window_start)`.
			sp_io::storage::set(&key, &(801u64, 508_767u32).encode());

			let bytes =
				InnerMigrateV0ToV1::<Test>::pre_upgrade().expect("pre_upgrade should succeed");
			let weight = InnerMigrateV0ToV1::<Test>::on_runtime_upgrade();

			assert_ok!(InnerMigrateV0ToV1::<Test>::post_upgrade(bytes));
			assert_eq!(
				weight,
				<<Test as frame_system::Config>::DbWeight as Get<RuntimeDbWeight>>::get()
					.reads_writes(1, 1)
			);
			assert!(sp_io::storage::get(&key).is_none());
		});
	}
}

// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use frame_support::{
	traits::{Get, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// Migration to increment sufficients for the sudo account
pub struct IncrementSudoSufficients<T>(PhantomData<T>);

impl<T> OnRuntimeUpgrade for IncrementSudoSufficients<T>
where
	T: frame_system::Config + pallet_sudo::Config,
{
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔄 Running migration: IncrementSudoSufficients");

		// Get the sudo account from storage
		if let Some(sudo_account) = pallet_sudo::Key::<T>::get() {
			let account_info = frame_system::Pallet::<T>::account(&sudo_account);
			let current_sufficients = account_info.sufficients;

			// Increment sufficients until it reaches 2
			if current_sufficients < 2 {
				let increments_needed = 2 - current_sufficients;

				for _ in 0..increments_needed {
					frame_system::Pallet::<T>::inc_sufficients(&sudo_account);
				}

				log::info!(
					"✅ Incremented sufficients for sudo account: {:?} (from {} to 2)",
					sudo_account,
					current_sufficients
				);

				// Weight: 1 read (sudo key) + 1 read (account info) + N writes (account info)
				// Each inc_sufficients does a read+write, so we account for all of them
				T::DbWeight::get().reads_writes(2, increments_needed as u64)
			} else {
				log::info!(
					"ℹ️ Sudo account {:?} already has {} sufficients (>= 2), no increment needed",
					sudo_account,
					current_sufficients
				);

				// Weight: 1 read (sudo key) + 1 read (account info)
				T::DbWeight::get().reads(2)
			}
		} else {
			log::warn!("⚠️ No sudo account found, skipping migration");
			T::DbWeight::get().reads(1)
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, TryRuntimeError> {
		use parity_scale_codec::Encode;

		if let Some(sudo_account) = pallet_sudo::Key::<T>::get() {
			let account_info = frame_system::Pallet::<T>::account(&sudo_account);
			let sufficients = account_info.sufficients;

			log::info!(
				"Pre-upgrade: Sudo account {:?} has {} sufficients",
				sudo_account,
				sufficients
			);

			// Return the current sufficients count to verify in post_upgrade
			Ok(sufficients.encode())
		} else {
			log::warn!("Pre-upgrade: No sudo account found");
			Ok(sp_std::vec::Vec::new())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), TryRuntimeError> {
		use parity_scale_codec::Decode;

		if let Some(sudo_account) = pallet_sudo::Key::<T>::get() {
			let account_info = frame_system::Pallet::<T>::account(&sudo_account);
			let new_sufficients = account_info.sufficients;

			if !state.is_empty() {
				let old_sufficients = u32::decode(&mut &state[..])
					.map_err(|_| TryRuntimeError::Other("Failed to decode old sufficients"))?;

				// Verify that sufficients is now at least 2
				if new_sufficients < 2 {
					return Err(TryRuntimeError::Other(
						"Sufficients did not reach 2 after migration",
					));
				}

				// If it was already >= 2, it should not have changed
				if old_sufficients >= 2 && new_sufficients != old_sufficients {
					return Err(TryRuntimeError::Other(
						"Sufficients changed when it was already >= 2",
					));
				}

				// If it was < 2, it should now be exactly 2
				if old_sufficients < 2 && new_sufficients != 2 {
					return Err(TryRuntimeError::Other(
						"Sufficients should be exactly 2 after migration",
					));
				}

				log::info!(
					"Post-upgrade: Sudo account {:?} sufficients: {} -> {} ✅",
					sudo_account,
					old_sufficients,
					new_sufficients
				);
			}
		}

		Ok(())
	}
}

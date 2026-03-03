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

//! Pallet providing storage and a `TransactionExtension` for signed transaction throttling.
//!
//! Tracks per-account byte usage within a rolling block window and rejects
//! transactions that exceed the configured limit.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod check_throttle;
pub use check_throttle::CheckThrottle;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Maximum bytes a single account can submit within a throttle window.
		#[pallet::constant]
		type MaxBytes: Get<u64>;

		/// Number of blocks that define a throttle window.
		#[pallet::constant]
		type WindowSize: Get<u32>;
	}

	/// Tracks (bytes_used, window_start_block) per account for throttling signed transactions.
	#[pallet::storage]
	pub type AccountUsage<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, (u64, BlockNumberFor<T>), ValueQuery>;
}

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

//! Benchmarking for pallet-version.
//!
//! The pallet has no extrinsics; its only cost is the `on_initialize` hook,
//! which appends the runtime's `spec_version` to the block digest on every
//! block. That write happens on the critical path of every block, so it is
//! measured rather than estimated.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize() {
		let block = frame_system::Pallet::<T>::block_number();

		#[block]
		{
			Pallet::<T>::on_initialize(block);
		}

		// The digest log the hook exists to deposit is present.
		let digest = frame_system::Pallet::<T>::digest();
		assert!(digest.logs().iter().any(|log| Pallet::<T>::decode_version(log).is_some()));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

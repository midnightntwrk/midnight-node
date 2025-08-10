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

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{Weight, constants::ParityDbWeight},
};

/// Weight functions needed for `pallet_version`.
pub trait WeightInfo {
	fn propose() -> Weight;
	fn on_finalize() -> Weight;
}

/// Weights for `pallet_timestamp` using the Substrate node and recommended hardware.
pub struct UpgradeWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for UpgradeWeight<T> {
	fn propose() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `???`
		//  Estimated: `???`
		// Minimum execution time: 8_356_000 picoseconds.
		// TODO: Specifiy the correct version::set() weights
		Weight::from_parts(8_684_000, 1493)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn on_finalize() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `194`
		//  Estimated: `0`
		// Minimum execution time: 3_886_000 picoseconds.
		// TODO: Specifiy the correct version::on_finalize() weights
		Weight::from_parts(4_118_000, 0)
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn propose() -> Weight {
		Weight::from_parts(8_684_000, 1493)
			.saturating_add(ParityDbWeight::get().reads(2_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	fn on_finalize() -> Weight {
		Weight::from_parts(4_118_000, 0)
	}
}

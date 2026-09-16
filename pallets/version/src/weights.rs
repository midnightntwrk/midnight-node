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

use core::marker::PhantomData;
use frame_support::weights::{Weight, constants::ParityDbWeight};

/// Weight functions needed for `pallet_version`.
pub trait WeightInfo {
	fn on_initialize() -> Weight;
}

/// Fallback weights for `pallet_version`, used until the benchmarked weights in
/// `runtime/src/weights/pallet_version.rs` are wired in.
///
/// `on_initialize` appends one digest log per block, which is a single storage
/// write. Previously this returned `Weight::zero()`, which under-reported the
/// hook on every block.
pub struct VersionWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for VersionWeight<T> {
	fn on_initialize() -> Weight {
		Weight::zero().saturating_add(ParityDbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn on_initialize() -> Weight {
		Weight::zero().saturating_add(ParityDbWeight::get().writes(1_u64))
	}
}

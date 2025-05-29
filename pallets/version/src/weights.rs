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
use frame_support::weights::{Weight, constants::ParityDbWeight};

/// Weight functions needed for `pallet_version`.
pub trait WeightInfo {
	fn on_initialize() -> Weight;
}

/// Weights for `pallet_timestamp` using the Substrate node and recommended hardware.
pub struct VersionWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for VersionWeight<T> {
	fn on_initialize() -> Weight {
		// TODO: Specifiy the correct version::on_initialize() weights
		Weight::zero()
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn on_initialize() -> Weight {
		Weight::zero().saturating_add(ParityDbWeight::get().writes(1_u64))
	}
}

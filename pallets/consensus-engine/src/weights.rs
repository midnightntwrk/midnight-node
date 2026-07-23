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

//! Weight definitions for `pallet-consensus-engine`.
//!
//! No benchmarks exist yet; the runtime wires the unit implementation (`()`), which reports
//! zero weight. Replace with generated weights once the extrinsics are benchmarked.

use frame_support::weights::Weight;

/// Weight functions needed for `pallet-consensus-engine`.
pub trait WeightInfo {
	/// Weight of the `arm_babe` extrinsic.
	fn arm_babe() -> Weight;
	/// Weight of the `schedule_flip` extrinsic.
	fn schedule_flip() -> Weight;
	/// Weight of the per-block `on_initialize` hook driving the automatic flip transitions.
	fn on_initialize() -> Weight;
}

impl WeightInfo for () {
	fn arm_babe() -> Weight {
		Weight::zero()
	}

	fn schedule_flip() -> Weight {
		Weight::zero()
	}

	fn on_initialize() -> Weight {
		Weight::zero()
	}
}

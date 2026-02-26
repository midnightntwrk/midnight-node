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

//! Weights for `pallet_cnight_observation`
//!
//! THIS FILE IS A PLACEHOLDER. Run benchmarks to generate production weights:
//! ```text
//! ./target/release/midnight-node benchmark pallet \
//!     --runtime ./target/release/wbuild/midnight-node-runtime/midnight_node_runtime.wasm \
//!     --genesis-builder=spec \
//!     --wasm-execution=compiled \
//!     --pallet=pallet_cnight_observation \
//!     --extrinsic=* \
//!     --steps 50 \
//!     --repeat 20 \
//!     --output pallets/cnight-observation/src/weights.rs \
//!     --template=./res/weights-template.hbs
//! ```

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::ParityDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_cnight_observation`.
pub trait WeightInfo {
	fn process_tokens(n: u32) -> Weight;
}

/// Weights for `pallet_cnight_observation` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Per-UTXO cost: ~2 storage reads + 1 storage write + event deposit.
	/// Placeholder until benchmarks are run — uses conservative estimates.
	/// The range of component `n` is `[0, 12800]`.
	fn process_tokens(n: u32) -> Weight {
		// Fixed cost: InherentExecutedThisBlock (1R + 1W), NextCardanoPosition (1W),
		// CardanoTxCapacityPerBlock (1R for validation)
		Weight::from_parts(15_000_000, 1500)
			.saturating_add(Weight::from_parts(5_000_000, 200).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().reads(2_u64.saturating_mul(n.into())))
			.saturating_add(T::DbWeight::get().writes(2_u64))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn process_tokens(_n: u32) -> Weight {
		Weight::zero()
	}
}

// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// Copyright (C) Parity Technologies (UK) Ltd. (weights adapted from `pallet-utility`)
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

//! Weights for `pallet_batch`.
//!
//! These numbers are inherited from upstream `pallet_utility` (`polkadot-stable2603`)
//! and have not yet been re-benchmarked against this runtime. They are conservative
//! upper bounds for the two calls we vendored (`batch`, `batch_all`).

#![allow(unused_parens)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{Weight, constants::RocksDbWeight},
};

/// Weight functions needed for `pallet_batch`.
pub trait WeightInfo {
	fn batch(c: u32) -> Weight;
	fn batch_all(c: u32) -> Weight;
}

/// Weights for `pallet_batch` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn batch(c: u32) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `3997`
		// Minimum execution time: 3_972_000 picoseconds.
		Weight::from_parts(4_034_000, 3997)
			// Standard Error: 2_323
			.saturating_add(Weight::from_parts(4_914_560, 0).saturating_mul(c.into()))
			.saturating_add(T::DbWeight::get().reads(2_u64))
	}
	fn batch_all(c: u32) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `3997`
		// Minimum execution time: 3_983_000 picoseconds.
		Weight::from_parts(4_075_000, 3997)
			// Standard Error: 2_176
			.saturating_add(Weight::from_parts(5_127_263, 0).saturating_mul(c.into()))
			.saturating_add(T::DbWeight::get().reads(2_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn batch(c: u32) -> Weight {
		Weight::from_parts(4_034_000, 3997)
			.saturating_add(Weight::from_parts(4_914_560, 0).saturating_mul(c.into()))
			.saturating_add(RocksDbWeight::get().reads(2_u64))
	}
	fn batch_all(c: u32) -> Weight {
		Weight::from_parts(4_075_000, 3997)
			.saturating_add(Weight::from_parts(5_127_263, 0).saturating_mul(c.into()))
			.saturating_add(RocksDbWeight::get().reads(2_u64))
	}
}

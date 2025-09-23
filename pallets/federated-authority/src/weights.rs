// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{Weight, constants::RocksDbWeight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_federated_authority.
pub trait WeightInfo {
	fn motion_approve(a: u32) -> Weight;
	fn motion_approve_new() -> Weight;
	fn motion_approve_ended() -> Weight;
	fn motion_approve_already_approved(a: u32) -> Weight;
	fn motion_approve_exceeds_bounds(a: u32) -> Weight;
	fn motion_revoke(a: u32) -> Weight;
	fn motion_revoke_ended() -> Weight;
	fn motion_revoke_not_found() -> Weight;
	fn motion_revoke_approval_missing(a: u32) -> Weight;
	fn motion_revoke_remove() -> Weight;
	fn motion_close_still_ongoing() -> Weight;
	fn motion_close_expired() -> Weight;
	fn motion_close_approved() -> Weight;
}

/// Weights for pallet_federated_authority using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: FederatedAuthority Motions (r:1 w:1)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_approve(a: u32) -> Weight {
		// Minimum execution time: 15_000 nanoseconds.
		Weight::from_parts(16_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(50_000, 0).saturating_mul(a.into()))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:1)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_approve_new() -> Weight {
		// Minimum execution time: 18_000 nanoseconds.
		Weight::from_parts(19_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_approve_ended() -> Weight {
		// Minimum execution time: 12_000 nanoseconds.
		Weight::from_parts(13_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_approve_already_approved(a: u32) -> Weight {
		// Minimum execution time: 13_000 nanoseconds.
		Weight::from_parts(14_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(40_000, 0).saturating_mul(a.into()))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_approve_exceeds_bounds(a: u32) -> Weight {
		// Minimum execution time: 14_000 nanoseconds.
		Weight::from_parts(15_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(45_000, 0).saturating_mul(a.into()))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:1)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_revoke(a: u32) -> Weight {
		// Minimum execution time: 16_000 nanoseconds.
		Weight::from_parts(17_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(45_000, 0).saturating_mul(a.into()))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_revoke_ended() -> Weight {
		// Minimum execution time: 12_000 nanoseconds.
		Weight::from_parts(13_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_revoke_not_found() -> Weight {
		// Minimum execution time: 10_000 nanoseconds.
		Weight::from_parts(11_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_revoke_approval_missing(a: u32) -> Weight {
		// Minimum execution time: 13_000 nanoseconds.
		Weight::from_parts(14_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(40_000, 0).saturating_mul(a.into()))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:1)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_revoke_remove() -> Weight {
		// Minimum execution time: 17_000 nanoseconds.
		// This includes removing the motion from storage
		Weight::from_parts(18_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:0)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_close_still_ongoing() -> Weight {
		// Minimum execution time: 10_000 nanoseconds.
		Weight::from_parts(11_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:1)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	fn motion_close_expired() -> Weight {
		// Minimum execution time: 14_000 nanoseconds.
		Weight::from_parts(15_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	// Storage: FederatedAuthority Motions (r:1 w:1)
	// Proof: FederatedAuthority Motions (max_values: None, max_size: Some(1028), added: 3503, mode: MaxEncodedLen)
	// Plus the weight of the dispatched call
	fn motion_close_approved() -> Weight {
		// Minimum execution time: 25_000 nanoseconds.
		Weight::from_parts(26_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn motion_approve(a: u32) -> Weight {
		// Minimum execution time: 15_000 nanoseconds.
		Weight::from_parts(16_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(50_000, 0).saturating_mul(a.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}

	fn motion_approve_new() -> Weight {
		// Minimum execution time: 18_000 nanoseconds.
		Weight::from_parts(19_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}

	fn motion_approve_ended() -> Weight {
		// Minimum execution time: 12_000 nanoseconds.
		Weight::from_parts(13_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_approve_already_approved(a: u32) -> Weight {
		// Minimum execution time: 13_000 nanoseconds.
		Weight::from_parts(14_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(40_000, 0).saturating_mul(a.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_approve_exceeds_bounds(a: u32) -> Weight {
		// Minimum execution time: 14_000 nanoseconds.
		Weight::from_parts(15_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(45_000, 0).saturating_mul(a.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_revoke(a: u32) -> Weight {
		// Minimum execution time: 16_000 nanoseconds.
		Weight::from_parts(17_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(45_000, 0).saturating_mul(a.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}

	fn motion_revoke_ended() -> Weight {
		// Minimum execution time: 12_000 nanoseconds.
		Weight::from_parts(13_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_revoke_not_found() -> Weight {
		// Minimum execution time: 10_000 nanoseconds.
		Weight::from_parts(11_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_revoke_approval_missing(a: u32) -> Weight {
		// Minimum execution time: 13_000 nanoseconds.
		Weight::from_parts(14_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			// Standard Error: 1_000
			.saturating_add(Weight::from_parts(40_000, 0).saturating_mul(a.into()))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_revoke_remove() -> Weight {
		// Minimum execution time: 17_000 nanoseconds.
		// This includes removing the motion from storage
		Weight::from_parts(18_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}

	fn motion_close_still_ongoing() -> Weight {
		// Minimum execution time: 10_000 nanoseconds.
		Weight::from_parts(11_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
	}

	fn motion_close_expired() -> Weight {
		// Minimum execution time: 14_000 nanoseconds.
		Weight::from_parts(15_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}

	fn motion_close_approved() -> Weight {
		// Minimum execution time: 25_000 nanoseconds.
		Weight::from_parts(26_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3503))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
}

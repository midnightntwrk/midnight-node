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

//! Benchmarking for system-parameters pallet

#![allow(clippy::unwrap_in_result)]

use super::*;
use alloc::{vec, vec::Vec};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_terms_and_conditions() -> Result<(), BenchmarkError> {
		// Create a maximally sized URL for worst case
		let url: Vec<u8> = vec![b'x'; pallet::MAX_URL_SIZE as usize];
		let hash = T::Hash::default();

		#[extrinsic_call]
		_(RawOrigin::Root, hash, url.clone());

		// Verify the storage was updated
		let stored = pallet::TermsAndConditionsStorage::<T>::get().expect("Terms should be stored");
		assert_eq!(stored.hash, hash);
		assert_eq!(stored.url.to_vec(), url);

		Ok(())
	}

	#[benchmark]
	fn update_d_parameter() -> Result<(), BenchmarkError> {
		let num_permissioned: u16 = 100;
		let num_registered: u16 = 50;

		#[extrinsic_call]
		_(RawOrigin::Root, num_permissioned, num_registered);

		// Verify the storage was updated
		let (stored_permissioned, stored_registered) = pallet::DParameterStorage::<T>::get();
		assert_eq!(stored_permissioned, num_permissioned);
		assert_eq!(stored_registered, num_registered);

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

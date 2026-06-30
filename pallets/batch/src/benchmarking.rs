// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// Copyright (C) Parity Technologies (UK) Ltd. (benchmarks adapted from `pallet-utility`)
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

//! Benchmarks for the Batch Pallet.

#![cfg(feature = "runtime-benchmarks")]

use alloc::vec;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

use crate::*;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks]
mod benchmark {
	use super::*;

	#[benchmark]
	fn batch(c: Linear<0, 1000>) {
		let calls = vec![frame_system::Call::remark { remark: vec![] }.into(); c as usize];

		#[extrinsic_call]
		_(RawOrigin::Root, calls);

		assert_last_event::<T>(Event::BatchCompleted.into());
	}

	#[benchmark]
	fn batch_all(c: Linear<0, 1000>) {
		let calls = vec![frame_system::Call::remark { remark: vec![] }.into(); c as usize];

		#[extrinsic_call]
		_(RawOrigin::Root, calls);

		assert_last_event::<T>(Event::BatchCompleted.into());
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}

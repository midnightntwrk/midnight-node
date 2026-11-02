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

//! Benchmarking setup for `pallet_c2m_bridge`.

#![allow(clippy::unwrap_in_result)]

use super::*;
use alloc::vec::Vec;
use frame_benchmarking::v2::*;
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sidechain_domain::McTxHash;
use sp_partner_chains_bridge::SubminimalTransfersConfig;

/// Builds a `BoundedVec` of `n` distinct `McTxHash` values.
fn build_hashes(n: u32) -> BoundedVec<McTxHash, ConstU32<MAX_APPROVALS_PER_BATCH>> {
	let hashes: Vec<McTxHash> = (0..n)
		.map(|i| {
			let mut bytes = [0u8; 32];
			bytes[0..4].copy_from_slice(&i.to_be_bytes());
			McTxHash(bytes)
		})
		.collect();
	BoundedVec::try_from(hashes).expect("n <= MAX_APPROVALS_PER_BATCH")
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark `set_subminimal_transfers_config`: a single storage write.
	#[benchmark]
	fn set_subminimal_transfers_config() -> Result<(), BenchmarkError> {
		let config = SubminimalTransfersConfig { subminimal_transfers_flush_threshold: 1_000_000 };

		#[extrinsic_call]
		_(RawOrigin::Root, config.clone());

		assert_eq!(SubminimalTransfersConfiguration::<T>::get(), config);
		Ok(())
	}

	/// Benchmark `add_approved_mc_tx_hashes` parameterised over batch size `n`.
	///
	/// Component `n`: number of hashes in the batch (1..=`MAX_APPROVALS_PER_BATCH`).
	/// Per-hash cost is one storage write into `ApprovedMcTxHashes`.
	#[benchmark]
	fn add_approved_mc_tx_hashes(
		n: Linear<1, MAX_APPROVALS_PER_BATCH>,
	) -> Result<(), BenchmarkError> {
		let hashes = build_hashes(n);

		#[extrinsic_call]
		_(RawOrigin::Root, hashes.clone());

		for hash in hashes.iter() {
			assert!(ApprovedMcTxHashes::<T>::contains_key(hash));
		}
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

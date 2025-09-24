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

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Get};
use frame_system::RawOrigin;
use parity_scale_codec::Decode;
#[benchmarks]
mod benchmarks {
	use super::*;

	// Helper function to create a motion with a specific number of approvals
	fn create_motion_with_approvals<T: Config>(num_approvals: u32) -> (T::Hash, T::MotionCall) {
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		// Create motion with approvals
		let mut approvals = BoundedBTreeSet::new();
		for i in 1..num_approvals {
			approvals.try_insert(i as u8).unwrap();
		}

		let ends_block = frame_system::Pallet::<T>::block_number() + T::MotionDuration::get();

		Motions::<T>::insert(
			motion_hash,
			MotionInfo::<T> { approvals, ends_block, call: call.clone() },
		);

		(motion_hash, call)
	}

	// Helper function to create an ended motion with a specific number of approvals
	fn create_ended_motion_with_approvals<T: Config>(
		num_approvals: u32,
	) -> (T::Hash, T::MotionCall) {
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		// Create motion with approvals
		let mut approvals = BoundedBTreeSet::new();
		for i in 0..num_approvals {
			approvals.try_insert(i as u8).unwrap();
		}

		// Set ends_block to current block to make it expired
		let ends_block = frame_system::Pallet::<T>::block_number();

		Motions::<T>::insert(
			motion_hash,
			MotionInfo::<T> { approvals, ends_block, call: call.clone() },
		);

		(motion_hash, call)
	}

	#[benchmark]
	fn motion_approve(
		a: Linear<1, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Create a motion with `a` existing approvals (leaving room for one more)
		let (motion_hash, call) = create_motion_with_approvals::<T>(a);

		// Get a valid origin for the next authority
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, Box::new(call));

		// Verify the motion has one more approval
		let motion = Motions::<T>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len() as u32, a + 1);

		Ok(())
	}

	#[benchmark]
	fn motion_approve_new() -> Result<(), BenchmarkError> {
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		// Get a valid origin
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_approve(origin as T::RuntimeOrigin, Box::new(call));

		// Verify the motion was created with one approval
		let motion = Motions::<T>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len(), 1);

		Ok(())
	}

	#[benchmark]
	fn motion_approve_ended() -> Result<(), BenchmarkError> {
		// Create an ended motion with arbitrary number of existing approvals (e.g., 1)
		// The actual number doesn't matter since we don't modify the set
		let (_motion_hash, call) = create_ended_motion_with_approvals::<T>(1);

		// Get a valid origin
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_approve(origin as T::RuntimeOrigin, Box::new(call));

		// The call should fail with MotionHasEnded error
		Ok(())
	}

	#[benchmark]
	fn motion_approve_already_approved(
		a: Linear<1, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Create a motion with `a` existing approvals, including approval from auth_id 0
		let (motion_hash, call) = create_motion_with_approvals::<T>(a);

		// Get the same origin that already approved (auth_id 0 should be included)
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_approve(origin as T::RuntimeOrigin, Box::new(call));

		// The call should fail with MotionAlreadyApproved error
		// Verify the motion still has the same number of approvals
		let motion = Motions::<T>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len() as u32, a);

		Ok(())
	}

	#[benchmark]
	fn motion_approve_exceeds_bounds(
		a: Linear<{ T::MaxAuthorityBodies::get() }, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Create a motion with maximum approvals already (a should equal MaxAuthorityBodies)
		let max_approvals = T::MaxAuthorityBodies::get();
		let (_motion_hash, call) = create_motion_with_approvals::<T>(max_approvals);

		// Try to add one more approval (should fail)
		// We need to ensure the origin returns an auth_id that's not already in the set
		// For this benchmark, we'll need to manipulate the motion to not include our origin's auth_id
		let motion_hash = T::Hashing::hash_of(&call);

		// Update the motion to remove one approval and add a different one to keep it at max
		Motions::<T>::mutate(motion_hash, |maybe_motion| {
			if let Some(motion) = maybe_motion {
				// Remove auth_id 0 and add max_approvals as auth_id instead
				motion.approvals.remove(&0);
				motion.approvals.try_insert(max_approvals as u8).ok();
			}
		});

		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_approve(origin as T::RuntimeOrigin, Box::new(call));

		// The call should fail with MotionApprovalExceedsBounds error
		Ok(())
	}

	#[benchmark]
	fn motion_revoke(a: Linear<1, { T::MaxAuthorityBodies::get() }>) -> Result<(), BenchmarkError> {
		// Create a motion with `a` existing approvals
		let (motion_hash, _call) = create_motion_with_approvals::<T>(a);

		// Get a valid origin (we'll use authority 0 which should exist)
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, motion_hash);

		// Verify the motion has one less approval if there were more than 1
		if a > 1 {
			let motion = Motions::<T>::get(motion_hash).unwrap();
			assert_eq!(motion.approvals.len() as u32, a - 1);
		} else {
			// Motion should be removed if last approval was revoked
			assert!(Motions::<T>::get(motion_hash).is_none());
		}

		Ok(())
	}

	#[benchmark]
	fn motion_revoke_ended() -> Result<(), BenchmarkError> {
		// Create an ended motion with arbitrary number of existing approvals (e.g., 2)
		// The actual number doesn't matter since we don't modify the set
		let (motion_hash, _call) = create_ended_motion_with_approvals::<T>(2);

		// Get a valid origin
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_revoke(origin as T::RuntimeOrigin, motion_hash);

		// The call should fail with MotionHasEnded error
		Ok(())
	}

	#[benchmark]
	fn motion_revoke_not_found() -> Result<(), BenchmarkError> {
		// Try to revoke from a non-existent motion
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		// Get a valid origin
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_revoke(origin as T::RuntimeOrigin, motion_hash);

		// The call should fail with MotionNotFound error
		Ok(())
	}

	#[benchmark]
	fn motion_revoke_approval_missing(
		a: Linear<1, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Create a motion with `a` approvals, but NOT from auth_id 0 (our origin)
		let (motion_hash, _call) = create_motion_with_approvals::<T>(a);

		// Remove auth_id 1 (if exists) and ensure auth_id 0 is not in the set
		Motions::<T>::mutate(motion_hash, |maybe_motion| {
			if let Some(motion) = maybe_motion {
				motion.approvals.remove(&1);
				// Ensure we don't have auth_id 0
				motion.approvals.remove(&0);
			}
		});

		// Get a valid origin (should be auth_id 0)
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_revoke(origin as T::RuntimeOrigin, motion_hash);

		// The call should fail with MotionApprovalMissing error
		Ok(())
	}

	#[benchmark]
	fn motion_revoke_remove() -> Result<(), BenchmarkError> {
		// Create a motion with exactly 1 approval from auth_id 0
		// When we revoke it, the motion should be removed
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		let mut approvals = BoundedBTreeSet::new();
		approvals.try_insert(0).unwrap(); // Only auth_id 0

		let ends_block = frame_system::Pallet::<T>::block_number() + T::MotionDuration::get();

		Motions::<T>::insert(
			motion_hash,
			MotionInfo::<T> { approvals, ends_block, call: call.clone() },
		);

		// Get a valid origin (should be auth_id 0)
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		motion_revoke(origin as T::RuntimeOrigin, motion_hash);

		// Verify the motion was removed
		assert!(Motions::<T>::get(motion_hash).is_none());

		Ok(())
	}

	#[benchmark]
	fn motion_close_still_ongoing() -> Result<(), BenchmarkError> {
		// Create a motion that is not approved and not ended
		let (motion_hash, _call) = create_motion_with_approvals::<T>(0);

		let account = T::AccountId::decode(&mut &[1u8; 32][..]).unwrap_or_else(|_| {
			T::AccountId::decode(&mut &[0u8; 32][..]).expect("32 bytes should decode")
		});
		let origin = RawOrigin::Signed(account);

		#[extrinsic_call]
		motion_close(origin, motion_hash);

		// The call should fail with MotionNotEnded error
		Ok(())
	}

	#[benchmark]
	fn motion_close_expired() -> Result<(), BenchmarkError> {
		// Create an ended motion that is not approved (less than required approvals)
		let (motion_hash, _call) = create_ended_motion_with_approvals::<T>(0);

		let account = T::AccountId::decode(&mut &[1u8; 32][..]).unwrap_or_else(|_| {
			T::AccountId::decode(&mut &[0u8; 32][..]).expect("32 bytes should decode")
		});
		let origin = RawOrigin::Signed(account);

		#[extrinsic_call]
		motion_close(origin, motion_hash);

		// Verify the motion was removed
		assert!(Motions::<T>::get(motion_hash).is_none());

		Ok(())
	}

	#[benchmark]
	fn motion_close_approved() -> Result<(), BenchmarkError> {
		// Create an ended motion that is approved (has all required approvals)
		// Assuming unanimous approval is required (all authorities)
		let num_approvals = T::MaxAuthorityBodies::get();
		let (motion_hash, _call) = create_ended_motion_with_approvals::<T>(num_approvals);

		let account = T::AccountId::decode(&mut &[1u8; 32][..]).unwrap_or_else(|_| {
			T::AccountId::decode(&mut &[0u8; 32][..]).expect("32 bytes should decode")
		});
		let origin = RawOrigin::Signed(account);

		#[extrinsic_call]
		motion_close(origin, motion_hash);

		// Verify the motion was removed after execution
		assert!(Motions::<T>::get(motion_hash).is_none());

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

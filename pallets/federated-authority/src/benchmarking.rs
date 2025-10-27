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

use super::*;
use crate::Pallet;

use frame_benchmarking::{account, v2::*};
use frame_support::traits::{EnsureOrigin, Get};
use frame_system::RawOrigin;
use sp_runtime::DispatchError;

#[benchmarks]
mod benchmarks {
	use super::*;

	// Helper function to create a motion with a specific number of approvals
	fn create_motion_with_approvals<T: Config>(num_approvals: u32) -> (T::Hash, T::MotionCall) {
		let ends_block = frame_system::Pallet::<T>::block_number() + T::MotionDuration::get();
		Pallet::<T>::create_motion_approvals(num_approvals, ends_block)
	}

	// Helper function to create an ended motion with a specific number of approvals
	fn create_ended_motion_with_approvals<T: Config>(
		num_approvals: u32,
	) -> (T::Hash, T::MotionCall) {
		// Set ends_block to current block to make it already ended
		let ends_block = frame_system::Pallet::<T>::block_number();
		Pallet::<T>::create_motion_approvals(num_approvals, ends_block)
	}

	// Helper function to create a motion with a specific `AuthId` approver
	fn create_motion_with_approval<T: Config>(auth_id: AuthId) -> (T::Hash, T::MotionCall) {
		let ends_block = frame_system::Pallet::<T>::block_number() + T::MotionDuration::get();
		Pallet::<T>::create_motion_approval(auth_id, ends_block)
	}

	#[benchmark]
	fn motion_approve(
		a: Linear<1, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Create a motion with `a` existing approvals (leaving room for one more)
		let (motion_hash, call) = create_motion_with_approvals::<T>(a - 1);

		// Get a valid origin for the next authority
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		#[extrinsic_call]
		_(origin, Box::new(call));

		// Verify the motion has one more approval
		let motion = Motions::<T>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len() as u32, a);

		Ok(())
	}

	#[benchmark]
	fn motion_approve_new() -> Result<(), BenchmarkError> {
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		// Get a valid origin
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		#[extrinsic_call]
		motion_approve(origin, Box::new(call));

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
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_approve(origin, Box::new(call));
		}

		// The call should fail with `MotionHasEnded` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionHasEnded")))
		);

		Ok(())
	}

	#[benchmark]
	fn motion_approve_already_approved(
		a: Linear<1, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Get the origin that will approve and its `auth_id`
		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;
		let auth_id = T::MotionApprovalOrigin::ensure_origin(origin.clone())
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		// Create a motion with `a` existing approvals, including approval from `auth_id``
		create_motion_with_approval::<T>(auth_id);
		let (motion_hash, call) = create_motion_with_approvals::<T>(a - 1);

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_approve(origin, Box::new(call));
		}

		// The call should fail with `MotionAlreadyApproved` error
		// Verify the motion still has the same number of approvals
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionAlreadyApproved")))
		);

		let motion = Motions::<T>::get(motion_hash).unwrap();
		assert_eq!(motion.approvals.len() as u32, a);

		Ok(())
	}

	#[benchmark]
	fn motion_approve_exceeds_bounds() -> Result<(), BenchmarkError> {
		// Create a motion with maximum approvals `T::MaxAuthorityBodies`
		let (_motion_hash, call) = create_motion_with_approvals::<T>(T::MaxAuthorityBodies::get());

		let origin = T::MotionApprovalOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_approve(origin, Box::new(call));
		}

		// The call should fail with `MotionApprovalExceedsBounds` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionApprovalExceedsBounds")))
		);

		Ok(())
	}

	#[benchmark]
	fn motion_revoke(a: Linear<1, { T::MaxAuthorityBodies::get() }>) -> Result<(), BenchmarkError> {
		// Get a valid origin
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;
		let auth_id = T::MotionApprovalOrigin::ensure_origin(origin.clone())
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		// Create a motion with `a` existing approvals, including approval from `auth_id``
		create_motion_with_approval::<T>(auth_id);
		let (motion_hash, _call) = create_motion_with_approvals::<T>(a - 1);

		#[extrinsic_call]
		_(origin, motion_hash);

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
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_revoke(origin, motion_hash);
		}

		// The call should fail with `MotionHasEnded`` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionHasEnded")))
		);
		Ok(())
	}

	#[benchmark]
	fn motion_revoke_not_found() -> Result<(), BenchmarkError> {
		// Try to revoke from a non-existent motion
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		// Get a valid origin
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_revoke(origin, motion_hash);
		}

		// The call should fail with `MotionNotFound` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionNotFound")))
		);
		Ok(())
	}

	#[benchmark]
	fn motion_revoke_approval_missing(
		a: Linear<1, { T::MaxAuthorityBodies::get() }>,
	) -> Result<(), BenchmarkError> {
		// Create a motion with `a` approvals, but NOT from auth_id 0 (our origin)
		let (motion_hash, _call) = create_motion_with_approvals::<T>(a);

		// Get a valid origin (should be auth_id 0)
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_revoke(origin, motion_hash);
		}

		// The call should fail with `MotionApprovalMissing` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionApprovalMissing")))
		);
		Ok(())
	}

	#[benchmark]
	fn motion_revoke_remove() -> Result<(), BenchmarkError> {
		// Get a valid origin
		let origin = T::MotionRevokeOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;
		let auth_id = T::MotionApprovalOrigin::ensure_origin(origin.clone())
			.map_err(|_| BenchmarkError::Stop("BadOrigin"))?;

		// Create a motion with `auth_id` approval
		let (motion_hash, _call) = create_motion_with_approval::<T>(auth_id);

		#[extrinsic_call]
		motion_revoke(origin, motion_hash);

		// Verify the motion was removed
		assert!(Motions::<T>::get(motion_hash).is_none());

		Ok(())
	}

	#[benchmark]
	fn motion_close_still_ongoing() -> Result<(), BenchmarkError> {
		// Create a motion
		let (motion_hash, _call) = create_motion_with_approvals::<T>(1);

		let account = account("anyone", 0, 0);
		let origin = RawOrigin::Signed(account);

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_close(origin.into(), motion_hash);
		}

		// The call should fail with `MotionNotEnded` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionNotEnded")))
		);

		Ok(())
	}

	#[benchmark]
	fn motion_close_expired() -> Result<(), BenchmarkError> {
		// Create an ended motion
		let (motion_hash, _call) = create_ended_motion_with_approvals::<T>(1);

		let account = account("anyone", 0, 0);
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

		let account = account("anyone", 0, 0);
		let origin = RawOrigin::Signed(account);

		#[extrinsic_call]
		motion_close(origin, motion_hash);

		// Verify the motion was removed after execution
		assert!(Motions::<T>::get(motion_hash).is_none());

		Ok(())
	}

	#[benchmark]
	fn motion_close_not_found() -> Result<(), BenchmarkError> {
		let account = account("anyone", 0, 0);
		let origin = RawOrigin::Signed(account);

		// Create a call that never has been approved
		let call: T::MotionCall = frame_system::Call::<T>::remark { remark: vec![0] }.into();
		let motion_hash = T::Hashing::hash_of(&call);

		let result;

		#[block]
		{
			result = Pallet::<T>::motion_close(origin.into(), motion_hash);
		}

		// The call should fail with `MotionNotFound` error
		assert!(
			matches!(result, Err(e) if matches!(e.error, DispatchError::Module(ref m) if m.message == Some("MotionNotFound")))
		);

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

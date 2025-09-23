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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod types;

pub use pallet::*;
pub use types::*;

use frame_support::{BoundedBTreeSet, dispatch::PostDispatchInfo};
use sp_runtime::{
	Saturating,
	traits::{Dispatchable, Hash},
};
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	/// Struct holding Motion information
	#[derive(CloneNoBound, PartialEqNoBound, Decode, Encode, RuntimeDebugNoBound, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct MotionInfo<T: Config> {
		pub approvals: BoundedBTreeSet<AuthId, T::MaxAuthorityBodies>,
		pub ends_block: BlockNumberFor<T>,
		pub call: T::MotionCall,
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The runtime call dispatch type.
		type MotionCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ From<frame_system::Call<Self>>
			+ GetDispatchInfo;
		/// The number of expected authority bodies in the Federated Authority
		#[pallet::constant]
		type MaxAuthorityBodies: Get<u32>;
		/// Motions duration
		#[pallet::constant]
		type MotionDuration: Get<BlockNumberFor<Self>>;
		/// The necessary proportion of approvals out of T::MaxAuthorityBodies for the motion to be enacted
		type MotionApprovalProportion: FederatedAuthorityProportion;
		/// The priviledged origin to register an approved motion
		type MotionApprovalOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = AuthId>;
		/// The priviledged origin to revoke a previously registered approved motion before it gets enacted
		type MotionRevokeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = AuthId>;
	}

	#[pallet::storage]
	pub type Motions<T: Config> = StorageMap<_, Identity, T::Hash, MotionInfo<T>, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// The motion has already been approved by this authority.
		MotionAlreadyApproved,
		/// The authority trying to kill a motion was not found in the list of approvers.
		MotionApprovalMissing,
		/// The motion approval excees T::MaxAuthorityBodies
		MotionApprovalExceedsBounds,
		/// Motion not found
		MotionNotFound,
		/// Motion not finished
		MotionNotEnded,
		/// Motion has ended and therefore it doesn't accept more changes
		MotionHasEnded,
		/// Motion is approved but need to wait until the approval period ends
		MotionTooEarlyToClose,
		/// Motion already exists
		MotionAlreadyExists,
		/// Motion expired without enough approvals
		MotionExpired,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A motion was approved by one authority body
		MotionApproved { motion_hash: T::Hash, auth_id: AuthId },
		/// A motion was executed after approval. `motion_result` contains the call result
		MotionDispatched { motion_hash: T::Hash, motion_result: DispatchResult },
		/// A motion expired after not being
		MotionExpired { motion_hash: T::Hash },
		/// An previously approved motion gets revoked
		MotionRevoked { motion_hash: T::Hash, auth_id: AuthId },
		/// A motion has been removed
		MotionRemoved { motion_hash: T::Hash },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight({0})] // TODO: fix weight
		pub fn motion_approve(
			origin: OriginFor<T>,
			call: Box<<T as Config>::MotionCall>,
		) -> DispatchResultWithPostInfo {
			let auth_id = T::MotionApprovalOrigin::ensure_origin(origin)?;
			let motion_hash = T::Hashing::hash_of(&call);

			Motions::<T>::try_mutate(motion_hash, |maybe_motion| {
				// Motion already exists, just try to insert approval
				if let Some(motion) = maybe_motion {
					// Only proceed if the motion has not ended yet
					ensure!(!Self::has_ended(&motion), Error::<T>::MotionHasEnded);

					match motion.approvals.try_insert(auth_id) {
						Ok(true) => Ok(()),
						Ok(false) => Err(Error::<T>::MotionAlreadyApproved),
						Err(_) => Err(Error::<T>::MotionApprovalExceedsBounds),
					}
				} else {
					// Motion doesn't exist yet - initialize it
					let mut approvals = BoundedBTreeSet::new();
					approvals
						.try_insert(auth_id)
						.map_err(|_| Error::<T>::MotionApprovalExceedsBounds)?;

					let ends_block = Self::block_number().saturating_add(T::MotionDuration::get());

					*maybe_motion = Some(MotionInfo::<T> { approvals, ends_block, call: *call });

					Ok(())
				}
			})?;

			Self::deposit_event(Event::MotionApproved { motion_hash, auth_id });

			Ok(Pays::No.into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight({0})] // TODO: fix weight
		pub fn motion_revoke(
			origin: OriginFor<T>,
			motion_hash: T::Hash,
		) -> DispatchResultWithPostInfo {
			let auth_id = T::MotionRevokeOrigin::ensure_origin(origin)?;

			let total_approvals = Motions::<T>::try_mutate(motion_hash, |maybe_motion| {
				let motion = maybe_motion.as_mut().ok_or(Error::<T>::MotionNotFound)?;

				// Only proceed if the motion has not ended yet
				ensure!(!Self::has_ended(&motion), Error::<T>::MotionHasEnded);

				motion
					.approvals
					.remove(&auth_id)
					.then(|| motion.approvals.len() as u32)
					.ok_or(Error::<T>::MotionApprovalMissing)
			})?;

			// If approvals get empty, we proceed to remove the motion
			if total_approvals == 0 {
				Self::motion_remove(motion_hash);
			}

			Self::deposit_event(Event::MotionRevoked { motion_hash, auth_id });

			Ok(Pays::No.into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight({0})] // TODO: fix weight
		pub fn motion_close(
			origin: OriginFor<T>,
			motion_hash: T::Hash,
		) -> DispatchResultWithPostInfo {
			// Anyone can try to close a motion
			ensure_signed(origin)?;

			let motion = Motions::<T>::get(motion_hash).ok_or(Error::<T>::MotionNotFound)?;
			let total_approvals = motion.approvals.len() as u32;

			if Self::is_motion_approved(total_approvals) {
				// Only allow closure if the motion has ended
				ensure!(Self::has_ended(&motion), Error::<T>::MotionTooEarlyToClose);

				// Dispatch motion
				Self::motion_dispatch(motion_hash)?;
				// Remove after dispatch
				Self::motion_remove(motion_hash);
			} else {
				// Only allow closure if the motion has ended
				ensure!(Self::has_ended(&motion), Error::<T>::MotionNotEnded);

				// Motion expired without enough approvals
				Self::deposit_event(Event::MotionExpired { motion_hash });
				Self::motion_remove(motion_hash);
			}

			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {
		fn motion_dispatch(motion_hash: T::Hash) -> DispatchResult {
			let motion = Motions::<T>::get(motion_hash).ok_or(Error::<T>::MotionNotFound)?;
			let res = motion.call.dispatch(frame_system::RawOrigin::Root.into());
			let motion_result = res.map(|_| ()).map_err(|e| e.error);
			Self::deposit_event(Event::MotionDispatched { motion_hash, motion_result });
			motion_result
		}

		fn motion_remove(motion_hash: T::Hash) {
			Motions::<T>::remove(motion_hash);
			Self::deposit_event(Event::MotionRemoved { motion_hash });
		}

		fn is_motion_approved(total_approvals: u32) -> bool {
			T::MotionApprovalProportion::reached_proportion(
				total_approvals,
				T::MaxAuthorityBodies::get(),
			)
		}

		fn block_number() -> BlockNumberFor<T> {
			<frame_system::Pallet<T>>::block_number()
		}

		/// Returns `true` if the motion has finished (expired).
		fn has_ended(motion: &MotionInfo<T>) -> bool {
			Self::block_number() >= motion.ends_block
		}
	}
}

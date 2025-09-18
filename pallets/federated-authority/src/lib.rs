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

// #[cfg(test)]
// mod mock;
// #[cfg(test)]
// mod tests;

pub use pallet::*;

use frame_support::{BoundedBTreeSet, dispatch::PostDispatchInfo};
use sp_runtime::{
	Saturating,
	traits::{Dispatchable, Hash},
};
use sp_std::prelude::*;

pub type AuthId = u8;

pub trait FederatedAuthorityProportion {
	fn reached_proportion(n: u32, d: u32) -> bool;
}

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
		MotionNotFinished,
		/// Motion already exists
		MotionAlreadyExists,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A motion was executed. motion_result contains the call result
		MotionDispatched { motion_hash: T::Hash, motion_result: DispatchResult },
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

			Motions::<T>::try_mutate_exists(motion_hash, |maybe_motion| -> DispatchResult {
				let motion = maybe_motion.as_mut().ok_or(Error::<T>::MotionNotFound)?;
				let total_approvals = motion.approvals.len() as u32;
				// Check if motion is approved
				if Self::is_motion_approved(total_approvals) {
					// Dispatch motion
					Self::motion_dispatch(motion_hash)?;
					// Remove after dispatch
					*maybe_motion = None;
				// If it has not been approved yet it can only be closed if the motion has finished
				} else {
					// Check if motion has expired
					let current_block = <frame_system::Pallet<T>>::block_number();
					if current_block >= motion.ends_block {
						// Motion expired without enough approvals, remove it
						*maybe_motion = None;
					} else {
						// Motion still ongoing
						return Err(Error::<T>::MotionNotFinished.into());
					}
				}

				Ok(())
			})?;

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
	}
}

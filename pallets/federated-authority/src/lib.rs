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

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod types;
pub mod weights;

pub use pallet::*;
pub use types::*;

use frame_support::{
	BoundedBTreeSet,
	dispatch::{Pays, PostDispatchInfo},
};
use sp_runtime::{
	DispatchError, DispatchErrorWithPostInfo, Saturating,
	traits::{Dispatchable, Hash},
};
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;
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
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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
		#[pallet::weight((
            T::WeightInfo::motion_approve(T::MaxAuthorityBodies::get()),
            DispatchClass::Operational
        ))]
		#[allow(clippy::useless_conversion)]
		pub fn motion_approve(
			origin: OriginFor<T>,
			call: Box<<T as Config>::MotionCall>,
		) -> DispatchResultWithPostInfo {
			let auth_id = T::MotionApprovalOrigin::ensure_origin(origin)?;
			let motion_hash = T::Hashing::hash_of(&call);

			let (is_new, total_approvals) = Motions::<T>::try_mutate(motion_hash, |maybe_motion| {
				// Motion already exists, just try to insert approval
				if let Some(motion) = maybe_motion {
					let total_approvals = motion.approvals.len() as u32;

					// Only proceed if the motion has not ended yet
					if Self::has_ended(motion) {
						return Err((Error::<T>::MotionHasEnded, total_approvals));
					}

					match motion.approvals.try_insert(auth_id) {
						Ok(true) => Ok((false, total_approvals)),
						Ok(false) => Err((Error::<T>::MotionAlreadyApproved, total_approvals)),
						Err(_) => Err((Error::<T>::MotionApprovalExceedsBounds, total_approvals)),
					}
				} else {
					// Motion doesn't exist yet - initialize it
					let mut approvals = BoundedBTreeSet::new();
					approvals
						.try_insert(auth_id)
						.map_err(|_| (Error::<T>::MotionApprovalExceedsBounds, 0))?;

					let ends_block = Self::block_number().saturating_add(T::MotionDuration::get());

					*maybe_motion = Some(MotionInfo::<T> { approvals, ends_block, call: *call });

					Ok((true, 1))
				}
			})
			.map_err(|(err, total_approvals)| {
				// Return actual weight based on the specific error case
				let actual_weight = match err {
					Error::<T>::MotionHasEnded => Some(T::WeightInfo::motion_approve_ended()),
					Error::<T>::MotionAlreadyApproved => {
						Some(T::WeightInfo::motion_approve_already_approved(total_approvals))
					},
					Error::<T>::MotionApprovalExceedsBounds => {
						Some(T::WeightInfo::motion_approve_exceeds_bounds())
					},
					_ => return err.into(), // This should be unreachable
				};

				let post_info = PostDispatchInfo { actual_weight, pays_fee: Pays::No };

				DispatchErrorWithPostInfo { post_info, error: err.into() }
			})?;

			Self::deposit_event(Event::MotionApproved { motion_hash, auth_id });

			// Return actual weight based on whether motion was new or existing
			let actual_weight = if is_new {
				T::WeightInfo::motion_approve_new()
			} else {
				T::WeightInfo::motion_approve(total_approvals)
			};

			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::No })
		}

		#[pallet::call_index(1)]
		#[pallet::weight((
            T::WeightInfo::motion_revoke(T::MaxAuthorityBodies::get()).max(T::WeightInfo::motion_revoke_remove()),
            DispatchClass::Operational
        ))]
		#[allow(clippy::useless_conversion)]
		pub fn motion_revoke(
			origin: OriginFor<T>,
			motion_hash: T::Hash,
		) -> DispatchResultWithPostInfo {
			let auth_id = T::MotionRevokeOrigin::ensure_origin(origin)?;

			let (final_approvals, initial_approvals) =
				Motions::<T>::try_mutate(motion_hash, |maybe_motion| {
					let motion = maybe_motion.as_mut().ok_or((Error::<T>::MotionNotFound, 0u32))?;
					let initial_count = motion.approvals.len() as u32;

					// Only proceed if the motion has not ended yet
					if Self::has_ended(motion) {
						return Err((Error::<T>::MotionHasEnded, initial_count));
					}

					motion
						.approvals
						.remove(&auth_id)
						.then(|| (motion.approvals.len() as u32, initial_count))
						.ok_or((Error::<T>::MotionApprovalMissing, initial_count))
				})
				.map_err(|(err, approvals)| {
					// Return actual weight based on the specific error case
					let actual_weight = match err {
						Error::<T>::MotionNotFound => {
							Some(T::WeightInfo::motion_revoke_not_found())
						},
						Error::<T>::MotionHasEnded => Some(T::WeightInfo::motion_revoke_ended()),
						Error::<T>::MotionApprovalMissing => {
							Some(T::WeightInfo::motion_revoke_approval_missing(approvals))
						},
						_ => return err.into(), // This should be unreachable
					};

					let post_info = PostDispatchInfo { actual_weight, pays_fee: Pays::No };

					DispatchErrorWithPostInfo { post_info, error: err.into() }
				})?;

			Self::deposit_event(Event::MotionRevoked { motion_hash, auth_id });

			// Return actual weight based on whether the motion gets removed or not
			let actual_weight = if final_approvals == 0 {
				// If approvals get empty, we proceed to remove the motion
				Self::motion_remove(motion_hash);
				T::WeightInfo::motion_revoke_remove()
			} else {
				T::WeightInfo::motion_revoke(initial_approvals)
			};
			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::No })
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::motion_close_approved().max(T::WeightInfo::motion_close_expired()))]
		#[allow(clippy::useless_conversion)]
		pub fn motion_close(
			origin: OriginFor<T>,
			motion_hash: T::Hash,
		) -> DispatchResultWithPostInfo {
			// Anyone can try to close a motion
			ensure_signed(origin)?;

			let motion = Motions::<T>::get(motion_hash).ok_or_else(|| {
				let post_info = PostDispatchInfo {
					actual_weight: Some(T::WeightInfo::motion_close_not_found()),
					pays_fee: Pays::No,
				};
				DispatchErrorWithPostInfo { post_info, error: Error::<T>::MotionNotFound.into() }
			})?;

			let total_approvals = motion.approvals.len() as u32;
			let has_ended = Self::has_ended(&motion);

			if Self::is_motion_approved(total_approvals) {
				// Dispatch motion
				Self::motion_dispatch(motion_hash)?;
				// Get dispatch weight
				let dispatch_weight = Self::get_dispatch_weight(motion_hash)?;
				// Remove after dispatch
				Self::motion_remove(motion_hash);

				// Return actual weight for approved motion
				Ok(PostDispatchInfo {
					actual_weight: Some(
						T::WeightInfo::motion_close_approved().saturating_add(dispatch_weight),
					),
					pays_fee: Pays::No,
				})
			} else {
				// Only allow closure if the motion has ended
				if !has_ended {
					let post_info = PostDispatchInfo {
						actual_weight: Some(T::WeightInfo::motion_close_still_ongoing()),
						pays_fee: Pays::No,
					};

					return Err(DispatchErrorWithPostInfo {
						post_info,
						error: Error::<T>::MotionNotEnded.into(),
					});
				}

				// Motion expired without enough approvals
				Self::deposit_event(Event::MotionExpired { motion_hash });
				Self::motion_remove(motion_hash);

				// Return actual weight for expired motion
				Ok(PostDispatchInfo {
					actual_weight: Some(T::WeightInfo::motion_close_expired()),
					pays_fee: Pays::No,
				})
			}
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

		fn get_dispatch_weight(motion_hash: T::Hash) -> Result<Weight, DispatchError> {
			let motion = Motions::<T>::get(motion_hash).ok_or(Error::<T>::MotionNotFound)?;
			Ok(motion.call.get_dispatch_info().call_weight)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn motion_call() -> (T::Hash, T::MotionCall) {
			let call: T::MotionCall =
				frame_system::Call::<T>::remark { remark: vec![1, 2, 3] }.into();
			let motion_hash = T::Hashing::hash_of(&call);

			(motion_hash, call)
		}

		#[cfg(feature = "runtime-benchmarks")]
		pub fn create_motion_approvals(
			num_approvals: u32,
			ends_block: BlockNumberFor<T>,
		) -> (T::Hash, T::MotionCall) {
			let (motion_hash, call) = Self::motion_call();

			// Collect approvals to add
			let mut new_approvals = BoundedBTreeSet::new();
			for i in 0..num_approvals {
				new_approvals.try_insert(i).unwrap();
			}

			Self::add_approvals_to_motion(new_approvals, motion_hash, call.clone(), ends_block);

			(motion_hash, call)
		}

		#[cfg(feature = "runtime-benchmarks")]
		pub fn create_motion_approval(
			auth_id: AuthId,
			ends_block: BlockNumberFor<T>,
		) -> (T::Hash, T::MotionCall) {
			let (motion_hash, call) = Self::motion_call();

			// Collect approvals to add
			let mut new_approvals = BoundedBTreeSet::new();
			new_approvals.try_insert(auth_id).unwrap();

			Self::add_approvals_to_motion(new_approvals, motion_hash, call.clone(), ends_block);

			(motion_hash, call)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn add_approvals_to_motion(
			approvals: BoundedBTreeSet<AuthId, T::MaxAuthorityBodies>,
			motion_hash: T::Hash,
			call: T::MotionCall,
			ends_block: BlockNumberFor<T>,
		) {
			Motions::<T>::mutate(motion_hash, |maybe_motion| {
				match maybe_motion {
					// If it already exists, just extend the approvals
					Some(motion) => {
						for a in approvals {
							let _ = motion.approvals.try_insert(a);
						}
					},
					// If not, create a new MotionInfo
					None => {
						*maybe_motion =
							Some(MotionInfo::<T> { approvals, ends_block, call: call.clone() });
					},
				}
			});
		}
	}
}

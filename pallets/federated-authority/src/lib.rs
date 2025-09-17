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

use frame_support::BoundedBTreeSet;
use sp_runtime::traits::Hash;
use sp_std::prelude::*;

pub type AuthId = u8;

pub trait FederatedAuthorityProportion {
	fn reached_proportion(n: u32, d: u32) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::GetDispatchInfo, pallet_prelude::*, traits::UnfilteredDispatchable,
	};
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// A call to be executed by the pallet
		#[pallet::no_default_bounds]
		type RuntimeCall: Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;
		/// The number of expected authority bodies in the Federated Authority
		#[pallet::constant]
		type MaxAuthorityBodies: Get<u32>;
		type MotionApprovalProportion: FederatedAuthorityProportion;
		/// The priviledged origin to register an approved motion
		type MotionApprovalOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = AuthId>;
		type MotionKillProportion: FederatedAuthorityProportion;
		/// The priviledged origin to kill a previously registered approved motion before it gets enacted
		type MotionKillOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = AuthId>;
	}

	#[pallet::storage]
	#[pallet::getter(fn approvals)]
	pub type MotionApprovals<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::Hash,
		BoundedBTreeSet<AuthId, T::MaxAuthorityBodies>,
		ValueQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// The motion has already been approved by this authority.
		MotionAlreadyApprovedBy { auth_id: AuthId },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight({0})] // TODO: fix weight
		pub fn motion_approve(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			let auth_id = T::MotionApprovalOrigin::ensure_origin(origin)?;

			let call_hash = T::Hashing::hash_of(&call);

			MotionApprovals::<T>::try_mutate(call_hash, |approvers| {
				approvers
					.try_insert(auth_id)
					.map_err(|_| Error::<T>::MotionAlreadyApprovedBy { auth_id })
			})?;

			let total_approvals = MotionApprovals::<T>::get(call_hash).len() as u32;

			if T::MotionApprovalProportion::reached_proportion(
				total_approvals,
				T::MaxAuthorityBodies::get(),
			) {
				// TODO: enact call
			}

			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {}
}

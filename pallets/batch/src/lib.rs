// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// Copyright (C) Parity Technologies (UK) Ltd. (portions adapted from `pallet-utility`)
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

//! # Batch Pallet
//!
//! A vendored, batch-only subset of `pallet-utility`. Provides `batch` and `batch_all`
//! and nothing else: the `dispatch_as`, `as_derivative`, `force_batch`, `with_weight`,
//! and `if_else` calls are intentionally absent.
//!
//! The motivation is governance safety. When wired with `RawOrigin::Root` (as the
//! federated-authority pallet does), upstream `pallet-utility::dispatch_as` lets the
//! root caller forge an arbitrary `PalletsOrigin` — including `None` and
//! `Signed(_)` — and dispatch any call as that origin (bypassing `BaseCallFilter`).
//! Stripping `dispatch_as` (and the `PalletsOrigin` associated type that exists
//! solely to support it) removes the origin-forging primitive entirely.
//!
//! ## Calls
//!
//! - [`Call::batch`] — best-effort batching; on first failure, emits `BatchInterrupted`
//!   and stops, but does not revert prior calls in the batch.
//! - [`Call::batch_all`] — atomic batching; any inner failure reverts the whole batch.
//!
//! Both calls require `Root` origin and reject everything else with `BadOrigin`.
//! Inner calls are dispatched with `dispatch_bypass_filter`, matching how governance
//! already invokes single calls via `federated-authority`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod benchmarking;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo, extract_actual_weight},
	traits::UnfilteredDispatchable,
};
use sp_runtime::traits::Dispatchable;
pub use weights::WeightInfo;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::DispatchClass, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait.
	///
	/// Note the absence of any `PalletsOrigin` associated type: the calls below never
	/// dispatch with an origin other than the caller's, so there is no need (and no
	/// way) to express "dispatch as some other origin."
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>
			+ UnfilteredDispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// Batch of dispatches did not complete fully. Index of first failing dispatch given, as
		/// well as the error.
		BatchInterrupted { index: u32, error: DispatchError },
		/// Batch of dispatches completed fully with no error.
		BatchCompleted,
		/// A single item within a Batch of dispatches has completed with no error.
		ItemCompleted,
	}

	// Align the call size to 1KB. As we are currently compiling the runtime for native/wasm
	// the `size_of` of the `Call` can be different. To ensure that this don't lead to
	// mismatches between native/wasm or to different metadata for the same runtime, we
	// align the call size. The value is chosen big enough to hopefully never reach it.
	const CALL_ALIGN: u32 = 1024;

	#[pallet::extra_constants]
	impl<T: Config> Pallet<T> {
		/// The limit on the number of batched calls.
		fn batched_calls_limit() -> u32 {
			let allocator_limit = sp_core::MAX_POSSIBLE_ALLOCATION;
			let call_size = (core::mem::size_of::<<T as Config>::RuntimeCall>() as u32)
				.div_ceil(CALL_ALIGN)
				* CALL_ALIGN;
			// The margin to take into account vec doubling capacity.
			let margin_factor = 3;

			allocator_limit / margin_factor / call_size
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// If you hit this error, you need to try to `Box` big dispatchable parameters.
			assert!(
				core::mem::size_of::<<T as Config>::RuntimeCall>() as u32 <= CALL_ALIGN,
				"Call enum size should be smaller than {} bytes.",
				CALL_ALIGN,
			);
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Too many calls batched.
		TooManyCalls,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Send a batch of dispatch calls.
		///
		/// Must be called from `Root`; any other origin is rejected with `BadOrigin`.
		/// Inner calls are dispatched with `dispatch_bypass_filter`, bypassing
		/// `frame_system::Config::BaseCallFilter`.
		///
		/// - `calls`: The calls to be dispatched. The number of calls must not exceed the
		///   constant: `batched_calls_limit` (available in constant metadata).
		///
		/// ## Complexity
		/// - O(C) where C is the number of calls to be batched.
		///
		/// On success this returns `Ok`. To determine whether each inner call succeeded, observe
		/// the deposited events: `BatchInterrupted` (with the failing index and error) on the
		/// first failure, or `BatchCompleted` if all calls succeeded.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let (dispatch_weight, dispatch_class) = Pallet::<T>::weight_and_dispatch_class(calls);
			let dispatch_weight = dispatch_weight.saturating_add(T::WeightInfo::batch(calls.len() as u32));
			(dispatch_weight, dispatch_class)
		})]
		pub fn batch(
			origin: OriginFor<T>,
			calls: Vec<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			let calls_len = calls.len();
			ensure!(calls_len <= Self::batched_calls_limit() as usize, Error::<T>::TooManyCalls);

			// Track the actual weight of each of the batch calls.
			let mut weight = Weight::zero();
			for (index, call) in calls.into_iter().enumerate() {
				let info = call.get_dispatch_info();
				let result = call.dispatch_bypass_filter(origin.clone());
				// Add the weight of this call.
				weight = weight.saturating_add(extract_actual_weight(&result, &info));
				if let Err(e) = result {
					Self::deposit_event(Event::BatchInterrupted {
						index: index as u32,
						error: e.error,
					});
					// Take the weight of this function itself into account.
					let base_weight = T::WeightInfo::batch(index.saturating_add(1) as u32);
					// Return the actual used weight + base_weight of this call.
					return Ok(Some(base_weight.saturating_add(weight)).into());
				}
				Self::deposit_event(Event::ItemCompleted);
			}
			Self::deposit_event(Event::BatchCompleted);
			let base_weight = T::WeightInfo::batch(calls_len as u32);
			Ok(Some(base_weight.saturating_add(weight)).into())
		}

		/// Send a batch of dispatch calls and atomically execute them.
		/// The whole transaction will rollback and fail if any of the calls failed.
		///
		/// Must be called from `Root`; any other origin is rejected with `BadOrigin`.
		/// Inner calls are dispatched with `dispatch_bypass_filter`, bypassing
		/// `frame_system::Config::BaseCallFilter`.
		///
		/// - `calls`: The calls to be dispatched. The number of calls must not exceed the
		///   constant: `batched_calls_limit` (available in constant metadata).
		///
		/// ## Complexity
		/// - O(C) where C is the number of calls to be batched.
		#[pallet::call_index(1)]
		#[pallet::weight({
			let (dispatch_weight, dispatch_class) = Pallet::<T>::weight_and_dispatch_class(calls);
			let dispatch_weight = dispatch_weight.saturating_add(T::WeightInfo::batch_all(calls.len() as u32));
			(dispatch_weight, dispatch_class)
		})]
		pub fn batch_all(
			origin: OriginFor<T>,
			calls: Vec<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			let calls_len = calls.len();
			ensure!(calls_len <= Self::batched_calls_limit() as usize, Error::<T>::TooManyCalls);

			// Track the actual weight of each of the batch calls.
			let mut weight = Weight::zero();
			for (index, call) in calls.into_iter().enumerate() {
				let info = call.get_dispatch_info();
				let result = call.dispatch_bypass_filter(origin.clone());
				// Add the weight of this call.
				weight = weight.saturating_add(extract_actual_weight(&result, &info));
				result.map_err(|mut err| {
					// Take the weight of this function itself into account.
					let base_weight = T::WeightInfo::batch_all(index.saturating_add(1) as u32);
					// Return the actual used weight + base_weight of this call.
					err.post_info = Some(base_weight.saturating_add(weight)).into();
					err
				})?;
				Self::deposit_event(Event::ItemCompleted);
			}
			Self::deposit_event(Event::BatchCompleted);
			let base_weight = T::WeightInfo::batch_all(calls_len as u32);
			Ok(Some(base_weight.saturating_add(weight)).into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the accumulated `weight` and the dispatch class for the given `calls`.
		fn weight_and_dispatch_class(
			calls: &[<T as Config>::RuntimeCall],
		) -> (Weight, DispatchClass) {
			let dispatch_infos = calls.iter().map(|call| call.get_dispatch_info());
			let (dispatch_weight, dispatch_class) = dispatch_infos.fold(
				(Weight::zero(), DispatchClass::Operational),
				|(total_weight, dispatch_class): (Weight, DispatchClass), di| {
					(
						total_weight.saturating_add(di.call_weight),
						// If not all are `Operational`, we want to use `DispatchClass::Normal`.
						if di.class == DispatchClass::Normal { di.class } else { dispatch_class },
					)
				},
			);

			(dispatch_weight, dispatch_class)
		}
	}
}

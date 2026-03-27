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

//! `TransactionExtension` that throttles signed transactions by tracking
//! per-account byte usage within a rolling block window.
//!
//! - `validate()` reads the throttle state and rejects if over the limit.
//! - `prepare()` writes the updated usage to storage (persists during block execution).

use crate::pallet::Config;
use core::marker::PhantomData;
use frame_support::RuntimeDebugNoBound;
use frame_support::{dispatch::DispatchInfo, pallet_prelude::*};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	Saturating,
	traits::{
		AsSystemOriginSigner, DispatchInfoOf, Dispatchable, Implication, TransactionExtension,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
};

#[derive(
	Encode, Decode, Clone, Eq, PartialEq, TypeInfo, RuntimeDebugNoBound, DecodeWithMemTracking,
)]
#[scale_info(skip_type_params(T))]
pub struct CheckThrottle<T: Config>(PhantomData<T>);

impl<T: Config> Default for CheckThrottle<T> {
	fn default() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config> CheckThrottle<T> {
	pub fn new() -> Self {
		Self::default()
	}
}

impl<T: Config + Send + Sync> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
	for CheckThrottle<T>
where
	<T as frame_system::Config>::RuntimeCall: Dispatchable<Info = DispatchInfo>,
	<<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin:
		AsSystemOriginSigner<T::AccountId> + Clone,
{
	const IDENTIFIER: &'static str = "CheckThrottle";
	type Implicit = ();
	type Val = Option<T::AccountId>;
	type Pre = ();

	fn validate(
		&self,
		origin: <<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Implication,
		_source: TransactionSource,
	) -> sp_runtime::traits::ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
		let Some(who) = origin.as_system_origin_signer() else {
			// Not a signed transaction — skip throttle check
			return Ok((ValidTransaction::default(), None, origin));
		};
		let who = who.clone();

		let current_block = frame_system::Pallet::<T>::block_number();
		let usage = crate::AccountUsage::<T>::get(&who);

		// Determine effective usage: reset if window has expired
		let window_size =
			frame_system::pallet_prelude::BlockNumberFor::<T>::from(T::WindowSize::get());
		let (effective_bytes, effective_txs) =
			if current_block.saturating_sub(usage.window_start) >= window_size {
				(0u64, 0u64)
			} else {
				(usage.bytes_used, usage.txs_used)
			};

		let new_bytes = effective_bytes.saturating_add(len as u64);
		if new_bytes > T::MaxBytes::get() {
			return Err(TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources));
		}

		let new_txs = effective_txs.saturating_add(1);
		if new_txs > T::MaxTxs::get() {
			return Err(TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources));
		}

		Ok((ValidTransaction::default(), Some(who), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &<<T as frame_system::Config>::RuntimeCall as Dispatchable>::RuntimeOrigin,
		_call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let Some(who) = val else {
			// Not a signed transaction — nothing to update
			return Ok(());
		};

		let current_block = frame_system::Pallet::<T>::block_number();
		let window_size =
			frame_system::pallet_prelude::BlockNumberFor::<T>::from(T::WindowSize::get());

		crate::AccountUsage::<T>::mutate(&who, |usage| {
			if current_block.saturating_sub(usage.window_start) >= window_size {
				usage.bytes_used = 0;
				usage.txs_used = 0;
				usage.window_start = current_block;
			}
			usage.bytes_used = usage.bytes_used.saturating_add(len as u64);
			usage.txs_used = usage.txs_used.saturating_add(1);
		});

		Ok(())
	}

	fn weight(&self, _call: &<T as frame_system::Config>::RuntimeCall) -> sp_runtime::Weight {
		// One storage read in validate + one storage read/write (mutate) in prepare.
		T::DbWeight::get().reads_writes(2, 1)
	}
}

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

//! # System Parameters Pallet
//!
//! This pallet stores and manages system-wide parameters such as:
//! - Terms and Conditions (hash and URL)
//! - D-Parameter (controlling authority selection)

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

mod runtime_api;
pub use runtime_api::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use crate::WeightInfo;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use scale_info::prelude::{string::String, vec::Vec};
	use sidechain_domain::DParameter;

	/// Maximum length for URL storage (256 bytes should be sufficient for most URLs)
	pub const MAX_URL_SIZE: u32 = 256;

	/// Terms and Conditions structure storing a hash and URL
	#[derive(
		Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct TermsAndConditions<Hash> {
		/// SHA-256 hash of the terms and conditions document
		pub hash: Hash,
		/// URL where the terms and conditions can be found (UTF-8 encoded)
		pub url: BoundedVec<u8, ConstU32<MAX_URL_SIZE>>,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Origin that can update system parameters.
		type SystemOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// Storage items

	/// Terms and Conditions storage
	#[pallet::storage]
	#[pallet::getter(fn terms_and_conditions)]
	pub type TermsAndConditionsStorage<T: Config> =
		StorageValue<_, TermsAndConditions<T::Hash>, OptionQuery>;

	/// D-Parameter storage as (num_permissioned_candidates, num_registered_candidates)
	/// Uses ValueQuery with default values of (0, 0)
	#[pallet::storage]
	pub type DParameterStorage<T: Config> = StorageValue<_, (u16, u16), ValueQuery>;

	// Genesis configuration

	/// Genesis configuration for Terms and Conditions
	#[derive(Debug, Clone, Default, frame_support::Serialize, frame_support::Deserialize)]
	pub struct TermsAndConditionsGenesisConfig<Hash> {
		/// SHA-256 hash of the terms and conditions document
		pub hash: Option<Hash>,
		/// URL where the terms and conditions can be found
		pub url: Option<String>,
	}

	/// Genesis configuration for D-Parameter
	#[derive(Debug, Clone, Default, frame_support::Serialize, frame_support::Deserialize)]
	pub struct DParameterGenesisConfig {
		/// Number of permissioned candidates
		pub num_permissioned_candidates: Option<u16>,
		/// Number of registered candidates
		pub num_registered_candidates: Option<u16>,
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Terms and conditions configuration
		pub terms_and_conditions: TermsAndConditionsGenesisConfig<T::Hash>,
		/// D-Parameter configuration
		pub d_parameter: DParameterGenesisConfig,
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Initialize Terms and Conditions if both hash and url are provided
			if let (Some(hash), Some(url)) =
				(&self.terms_and_conditions.hash, &self.terms_and_conditions.url)
			{
				let url_bounded: BoundedVec<u8, ConstU32<MAX_URL_SIZE>> = url
					.as_bytes()
					.to_vec()
					.try_into()
					.expect("Terms and conditions URL exceeds maximum length");

				TermsAndConditionsStorage::<T>::put(TermsAndConditions {
					hash: *hash,
					url: url_bounded,
				});
			}

			// Initialize D-Parameter if provided
			if let (Some(num_permissioned), Some(num_registered)) = (
				self.d_parameter.num_permissioned_candidates,
				self.d_parameter.num_registered_candidates,
			) {
				DParameterStorage::<T>::put((num_permissioned, num_registered));
			}
		}
	}

	// Events

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Terms and Conditions have been updated
		TermsAndConditionsUpdated {
			/// The new hash of the terms and conditions
			hash: T::Hash,
			/// The new URL
			url: BoundedVec<u8, ConstU32<MAX_URL_SIZE>>,
		},
		/// D-Parameter has been updated
		DParameterUpdated {
			/// Number of permissioned candidates
			num_permissioned_candidates: u16,
			/// Number of registered candidates
			num_registered_candidates: u16,
		},
	}

	// Errors

	#[pallet::error]
	pub enum Error<T> {
		/// The provided URL exceeds the maximum allowed length
		UrlTooLong,
	}

	// Dispatchable functions (extrinsics)

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Update the Terms and Conditions.
		///
		/// Can only be called by the configured SystemOrigin.
		///
		/// # Arguments
		/// * `hash` - SHA-256 hash of the terms and conditions document
		/// * `url` - URL where the terms and conditions can be found
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_terms_and_conditions())]
		pub fn update_terms_and_conditions(
			origin: OriginFor<T>,
			hash: T::Hash,
			url: Vec<u8>,
		) -> DispatchResult {
			T::SystemOrigin::ensure_origin(origin)?;

			let url_bounded: BoundedVec<u8, ConstU32<MAX_URL_SIZE>> =
				url.try_into().map_err(|_| Error::<T>::UrlTooLong)?;

			TermsAndConditionsStorage::<T>::put(TermsAndConditions {
				hash,
				url: url_bounded.clone(),
			});

			Self::deposit_event(Event::TermsAndConditionsUpdated { hash, url: url_bounded });

			Ok(())
		}

		/// Update the D-Parameter.
		///
		/// Can only be called by the configured SystemOrigin.
		///
		/// # Arguments
		/// * `num_permissioned_candidates` - Expected number of permissioned candidates
		/// * `num_registered_candidates` - Expected number of registered candidates
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::update_d_parameter())]
		pub fn update_d_parameter(
			origin: OriginFor<T>,
			num_permissioned_candidates: u16,
			num_registered_candidates: u16,
		) -> DispatchResult {
			T::SystemOrigin::ensure_origin(origin)?;

			DParameterStorage::<T>::put((num_permissioned_candidates, num_registered_candidates));

			Self::deposit_event(Event::DParameterUpdated {
				num_permissioned_candidates,
				num_registered_candidates,
			});

			Ok(())
		}
	}

	// Helper functions
	impl<T: Config> Pallet<T> {
		/// Get the current Terms and Conditions
		pub fn get_terms_and_conditions() -> Option<TermsAndConditions<T::Hash>> {
			TermsAndConditionsStorage::<T>::get()
		}

		/// Get the current D-Parameter
		pub fn get_d_parameter() -> DParameter {
			let (num_permissioned, num_registered) = DParameterStorage::<T>::get();
			DParameter::new(num_permissioned, num_registered)
		}
	}
}

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

//! # System Parameters Pallet
//!
//! This pallet stores and manages system-wide parameters such as:
//! - Terms and Conditions (hash and URL)
//! - D-Parameter (controlling authority selection)
//! - `cnight_weight` (with the `decentralization` feature): the lovelace of virtual stake
//!   per STAR of staked cNIGHT, NIGHT Staking on Cardano MIP rule 25

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

#[cfg(feature = "decentralization")]
pub mod migrations;

#[frame_support::pallet]
// clippy 1.100 lints FRAME-generated code; these fire on macro output, not on this module.
#[allow(clippy::clone_on_copy, clippy::redundant_field_names, clippy::duplicated_attributes)]
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

		/// Maximum committee size the D-Parameter may request in total.
		#[pallet::constant]
		type MaxCommitteeSize: Get<u32>;

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

	/// `cnight_weight` at activation: 1, the MIP's value, itself a placeholder (Open
	/// Question 6). Genesis and [`crate::migrations::InitializeCnightWeight`] set it.
	#[cfg(feature = "decentralization")]
	pub const INITIAL_CNIGHT_WEIGHT: (u64, u64) = (1, 1);

	/// `cnight_weight` as (numerator, denominator): the lovelace of virtual stake per STAR of
	/// staked cNIGHT (NIGHT Staking on Cardano MIP, rule 25). Always set, by genesis or by
	/// [`crate::migrations::InitializeCnightWeight`], and its denominator is never 0.
	///
	/// A change counts from the next snapshot: each snapshot records the value it takes,
	/// and this storage holds only the current one.
	#[cfg(feature = "decentralization")]
	#[pallet::storage]
	pub type CnightWeightStorage<T: Config> = StorageValue<_, (u64, u64), ValueQuery>;

	// Genesis configuration

	/// Genesis configuration for Terms and Conditions
	#[derive(Debug, Clone, frame_support::Serialize, frame_support::Deserialize)]
	pub struct TermsAndConditionsGenesisConfig<Hash> {
		/// SHA-256 hash of the terms and conditions document
		pub hash: Option<Hash>,
		/// URL where the terms and conditions can be found
		pub url: Option<String>,
	}

	/// Default terms and conditions URL used across all networks
	pub const DEFAULT_TERMS_AND_CONDITIONS_URL: &str = "https://www.midnight.gd/global-terms-txt";

	/// Default terms and conditions hash bytes (SHA-256 of the terms document)
	pub const DEFAULT_TERMS_AND_CONDITIONS_HASH_BYTES: [u8; 32] = [
		0xca, 0x85, 0xed, 0x77, 0xbc, 0xe6, 0x82, 0x88, 0xe5, 0x53, 0x00, 0xf0, 0x06, 0xcc, 0xd5,
		0xcc, 0xe5, 0xd4, 0x94, 0x0d, 0xc3, 0x9f, 0xc4, 0x11, 0x73, 0xa9, 0xc2, 0xec, 0xd1, 0xeb,
		0x61, 0x6e,
	];

	impl<Hash: Default + AsMut<[u8]>> Default for TermsAndConditionsGenesisConfig<Hash> {
		fn default() -> Self {
			use crate::alloc::string::ToString;

			let mut hash = Hash::default();
			let hash_bytes = hash.as_mut();
			let len = hash_bytes.len().min(DEFAULT_TERMS_AND_CONDITIONS_HASH_BYTES.len());
			hash_bytes[..len].copy_from_slice(&DEFAULT_TERMS_AND_CONDITIONS_HASH_BYTES[..len]);
			Self { hash: Some(hash), url: Some(DEFAULT_TERMS_AND_CONDITIONS_URL.to_string()) }
		}
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
			// Initialize Terms and Conditions; both hash and url must be present.
			let hash = self
				.terms_and_conditions
				.hash
				.as_ref()
				.expect("Genesis terms and conditions hash must be set");
			let url = self
				.terms_and_conditions
				.url
				.as_ref()
				.expect("Genesis terms and conditions URL must be set");

			let url_bounded: BoundedVec<u8, ConstU32<MAX_URL_SIZE>> = url
				.as_bytes()
				.to_vec()
				.try_into()
				.expect("Terms and conditions URL exceeds maximum length");

			TermsAndConditionsStorage::<T>::put(TermsAndConditions {
				hash: *hash,
				url: url_bounded,
			});

			// Initialize D-Parameter if provided
			if let (Some(num_permissioned), Some(num_registered)) = (
				self.d_parameter.num_permissioned_candidates,
				self.d_parameter.num_registered_candidates,
			) {
				Pallet::<T>::ensure_valid_d_parameter(num_permissioned, num_registered)
					.expect("Genesis D-Parameter is invalid");
				DParameterStorage::<T>::put((num_permissioned, num_registered));
			}

			#[cfg(feature = "decentralization")]
			CnightWeightStorage::<T>::put(INITIAL_CNIGHT_WEIGHT);
		}
	}

	// Events

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
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
		/// `cnight_weight` has been updated
		#[cfg(feature = "decentralization")]
		CnightWeightUpdated {
			/// Lovelace of virtual stake per `denominator` STAR of staked cNIGHT
			numerator: u64,
			/// The STAR `numerator` is per; never 0
			denominator: u64,
		},
	}

	// Errors

	#[pallet::error]
	pub enum Error<T> {
		/// The provided URL exceeds the maximum allowed length
		UrlTooLong,
		/// The D-Parameter requests no committee seats
		DParameterEmpty,
		/// The D-Parameter's total seat count exceeds `MaxCommitteeSize`
		DParameterTooLarge,
		/// `cnight_weight`'s denominator is 0
		#[cfg(feature = "decentralization")]
		CnightWeightZeroDenominator,
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
		///
		/// The total seat count must be non-zero and at most `MaxCommitteeSize`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::update_d_parameter())]
		pub fn update_d_parameter(
			origin: OriginFor<T>,
			num_permissioned_candidates: u16,
			num_registered_candidates: u16,
		) -> DispatchResult {
			T::SystemOrigin::ensure_origin(origin)?;
			Self::ensure_valid_d_parameter(num_permissioned_candidates, num_registered_candidates)?;

			DParameterStorage::<T>::put((num_permissioned_candidates, num_registered_candidates));

			Self::deposit_event(Event::DParameterUpdated {
				num_permissioned_candidates,
				num_registered_candidates,
			});

			Ok(())
		}

		/// Update `cnight_weight`, the lovelace of virtual stake per STAR of staked cNIGHT,
		/// as `numerator / denominator` (NIGHT Staking on Cardano MIP, rule 25).
		///
		/// Can only be called by the configured SystemOrigin, as the D-Parameter is set.
		/// The denominator must be above 0. A change counts from the next snapshot.
		// The same one small write as `update_d_parameter`. A benchmark of its own would
		// exist only with the `decentralization` feature, and the generated runtime weights
		// are built without it.
		#[cfg(feature = "decentralization")]
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::update_d_parameter())]
		pub fn update_cnight_weight(
			origin: OriginFor<T>,
			numerator: u64,
			denominator: u64,
		) -> DispatchResult {
			T::SystemOrigin::ensure_origin(origin)?;
			ensure!(denominator > 0, Error::<T>::CnightWeightZeroDenominator);

			CnightWeightStorage::<T>::put((numerator, denominator));

			Self::deposit_event(Event::CnightWeightUpdated { numerator, denominator });

			Ok(())
		}
	}

	// Helper functions
	impl<T: Config> Pallet<T> {
		/// Get the current Terms and Conditions
		pub fn get_terms_and_conditions() -> Option<TermsAndConditions<T::Hash>> {
			TermsAndConditionsStorage::<T>::get()
		}

		/// Get the current `cnight_weight` as (numerator, denominator)
		#[cfg(feature = "decentralization")]
		pub fn get_cnight_weight() -> (u64, u64) {
			CnightWeightStorage::<T>::get()
		}

		/// Get the current D-Parameter
		pub fn get_d_parameter() -> DParameter {
			let (num_permissioned, num_registered) = DParameterStorage::<T>::get();
			DParameter::new(num_permissioned, num_registered)
		}

		/// Checks that the D-Parameter requests a committee of 1 to `MaxCommitteeSize` seats.
		pub fn ensure_valid_d_parameter(
			num_permissioned_candidates: u16,
			num_registered_candidates: u16,
		) -> Result<(), Error<T>> {
			let total =
				u32::from(num_permissioned_candidates) + u32::from(num_registered_candidates);
			ensure!(total > 0, Error::<T>::DParameterEmpty);
			ensure!(total <= T::MaxCommitteeSize::get(), Error::<T>::DParameterTooLarge);
			Ok(())
		}
	}
}

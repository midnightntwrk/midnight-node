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

use sp_api::decl_runtime_apis;
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Values for tracking position of a sync on Cardano
/// Block hash here is mostly informational for debugging purposes
/// TODO: Default probably shouldn't be derived
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Copy,
	Clone,
	Eq,
	PartialEq,
	Debug,
	Default,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct CardanoPosition {
	/// Hash of the last processed block
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub block_hash: [u8; 32],
	/// Block number of the last processed block
	pub block_number: u32,
	/// The index of the next transaction to process in the block
	pub tx_index_in_block: u32,
}

impl core::fmt::Display for CardanoPosition {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"{{ block_number: {}, block_hash: {}, block_index: {} }}",
			self.block_number,
			hex::encode(self.block_hash),
			self.tx_index_in_block
		)
	}
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct TokenObservationConfig {
	pub mapping_validator_address: String,
	pub redemption_validator_address: String,
	pub policy_id: String,
	pub asset_name: String,
}

impl CardanoPosition {
	/// Increment CardanoPosition to the next tx index.
	/// Useful for pointing to the next-block position
	pub fn increment(mut self) -> Self {
		self.tx_index_in_block += 1;
		self
	}
}

impl PartialOrd for CardanoPosition {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		match self.block_number.partial_cmp(&other.block_number) {
			Some(core::cmp::Ordering::Equal) => {},
			ord => return ord,
		}
		self.tx_index_in_block.partial_cmp(&other.tx_index_in_block)
	}
}

decl_runtime_apis! {
	pub trait NativeTokenObservationApi {
		/// Get the contract address on Cardano which executes Glacier Drop redemptions
		fn get_redemption_validator_address() -> Vec<u8>;
		/// Get the contract address on Cardano which emits registration mappings in utxo datums
		fn get_mapping_validator_address() -> Vec<u8>;
		/// Get the Cardano native token identifier for the chosen asset
		fn get_native_token_identifier() -> (Vec<u8>, Vec<u8>);

		fn get_next_cardano_position() -> CardanoPosition;

		fn get_cardano_block_window_size() -> u32;

		fn get_utxo_capacity_per_block() -> u32;
	}
}

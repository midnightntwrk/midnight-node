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

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use sp_api::decl_runtime_apis;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sidechain_domain::McTxHash;

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
pub struct CNightAddresses {
	/// Address of the cNight mapping validator
	pub mapping_validator_address: String,
	/// Address of the glacier drop redemption validator
	pub redemption_validator_address: String,
	/// Policy ID of the currency token (i.e. cNIGHT)
	pub policy_id: String,
	/// Asset name of the currency token (i.e. cNIGHT)
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

pub const INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = *b"ntobsrve";

#[derive(Encode, Debug, PartialEq)]
#[cfg_attr(feature = "std", derive(Decode, DecodeWithMemTracking, thiserror::Error))]
pub enum InherentError {
	#[cfg_attr(feature = "std", error("Unexpected error"))]
	UnexpectedTokenObserveInherent(Option<Vec<Vec<u8>>>, Option<Vec<Vec<u8>>>),
	#[cfg_attr(feature = "std", error("Inherent data missing"))]
	Missing,
	#[cfg_attr(feature = "std", error("Other unexpected inherent error"))]
	Other,
}

impl sp_inherents::IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}

#[derive(Decode, DecodeWithMemTracking, Debug, Encode, Clone)]
pub struct MidnightObservationTokenMovement {
	pub utxos: Vec<ObservedUtxo>,
	pub next_cardano_position: CardanoPosition,
}

#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo)]
pub struct ObservedUtxo {
	pub header: ObservedUtxoHeader,
	pub data: ObservedUtxoData,
}

impl PartialOrd for ObservedUtxo {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Eq for ObservedUtxo {}

impl Ord for ObservedUtxo {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.header.tx_position.partial_cmp(&other.header.tx_position).unwrap()
	}
}

/// A struct to contain all UTXOs in a given range
#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ObservedUtxos {
	// Start position (inclusive)
	pub start: CardanoPosition,
	// End position (inclusive)
	pub end: CardanoPosition,
	pub utxos: Vec<ObservedUtxo>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum ObservedUtxoData {
	RedemptionCreate(RedemptionCreateData),
	RedemptionSpend(RedemptionSpendData),
	Registration(RegistrationData),
	Deregistration(DeregistrationData),
	AssetCreate(CreateData),
	AssetSpend(SpendData),
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RedemptionCreateData {
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub owner: Vec<u8>,
	pub value: u128,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RedemptionSpendData {
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub owner: Vec<u8>,
	pub value: u128,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub spending_tx_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RegistrationData {
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub cardano_address: Vec<u8>,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub dust_address: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct DeregistrationData {
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub cardano_address: Vec<u8>,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub dust_address: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct CreateData {
	pub value: u128,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub owner: Vec<u8>,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct SpendData {
	pub value: u128,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub owner: Vec<u8>,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
	#[cfg_attr(feature = "std", serde(with = "hex"))]
	pub spending_tx_hash: [u8; 32],
}

/// Header for an observed UTXO
/// This header can be used for both create and spend events for UTXOs.
/// The ordering assumes that each header is unique per TX i.e. that only one relevant UTXO is included in each transaction
#[derive(Debug, Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, PartialEq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ObservedUtxoHeader {
	pub tx_position: CardanoPosition,
	pub tx_hash: McTxHash,
	pub utxo_tx_hash: McTxHash,
	pub utxo_index: UtxoIndexInTx,
}

impl core::fmt::Display for ObservedUtxoHeader {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"{{ tx_position: {}, tx_hash: {}, utxo: {}#{} }}",
			self.tx_position,
			hex::encode(self.tx_hash.0),
			hex::encode(self.utxo_tx_hash.0),
			self.utxo_index.0
		)
	}
}

#[derive(
	Debug, Copy, Clone, PartialEq, PartialOrd, Encode, Decode, DecodeWithMemTracking, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct UtxoIndexInTx(pub u16);

impl PartialOrd for ObservedUtxoHeader {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		match self.tx_position.partial_cmp(&other.tx_position) {
			Some(core::cmp::Ordering::Equal) => {},
			ord => return ord,
		}
		match self.tx_hash.0.partial_cmp(&other.tx_hash.0) {
			Some(core::cmp::Ordering::Equal) => {},
			ord => return ord,
		}
		match self.utxo_tx_hash.0.partial_cmp(&other.utxo_tx_hash.0) {
			Some(core::cmp::Ordering::Equal) => {},
			ord => return ord,
		}
		self.utxo_index.partial_cmp(&other.utxo_index)
	}
}

decl_runtime_apis! {
	pub trait CNightObservationApi {
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

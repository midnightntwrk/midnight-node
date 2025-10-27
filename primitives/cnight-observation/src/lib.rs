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
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sidechain_domain::McTxHash;

#[cfg(feature = "std")]
use sqlx::types::chrono::{DateTime, Utc};

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
	Serialize,
	Deserialize,
)]
pub struct TimestampUnixMillis(pub i64);

#[cfg(feature = "std")]
impl From<DateTime<Utc>> for TimestampUnixMillis {
	fn from(value: DateTime<Utc>) -> Self {
		Self(value.timestamp_millis())
	}
}

/// Values for tracking position of a sync on Cardano
/// Block hash here is mostly informational for debugging purposes
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
	Serialize,
	Deserialize,
)]
pub struct CardanoPosition {
	/// Hash of the last processed block
	#[serde(with = "hex")]
	pub block_hash: [u8; 32],
	/// Block number of the last processed block
	pub block_number: u32,
	/// Block timestamp (seconds since unix epoch) of the last processed block
	pub block_timestamp: TimestampUnixMillis,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(serde_valid::Validate))]
pub struct CNightAddresses {
	/// Address of the cNight mapping validator. Shelley address, Bech32
	#[cfg_attr(feature = "std", validate(pattern = r"^(addr|addr_test)1[0-9a-z]{1,108}$"))]
	pub mapping_validator_address: String,
	/// Address of the glacier drop redemption validator. Shelley address, Bech32
	#[cfg_attr(feature = "std", validate(pattern = r"^(addr|addr_test)1[0-9a-z]{1,108}$"))]
	pub redemption_validator_address: String,
	/// Policy ID of the currency token (i.e. cNIGHT)
	#[serde(with = "hex")]
	pub cnight_policy_id: [u8; 28],
	/// Asset name of the currency token. Max length: 32 bytes
	/// [Cardano Source](https://github.com/IntersectMBO/cardano-ledger/blob/683bef2e40cbd10339452c9f2009867c855baf1a/shelley-ma/shelley-ma-test/cddl-files/shelley-ma.cddl#L252)
	#[cfg_attr(feature = "std", validate(max_length = 32))]
	// Ascii only
	#[cfg_attr(feature = "std", validate(pattern = r"^[\x00-\x7F]*$"))]
	pub cnight_asset_name: String,
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

#[derive(
	Debug, Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, Serialize, Deserialize,
)]
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
		self.header.partial_cmp(&other.header).unwrap()
	}
}

/// A struct to contain all UTXOs in a given range
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObservedUtxos {
	// Start position (inclusive)
	pub start: CardanoPosition,
	// End position (inclusive)
	pub end: CardanoPosition,
	pub utxos: Vec<ObservedUtxo>,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub enum ObservedUtxoData {
	RedemptionCreate(RedemptionCreateData),
	RedemptionSpend(RedemptionSpendData),
	Registration(RegistrationData),
	Deregistration(DeregistrationData),
	AssetCreate(CreateData),
	AssetSpend(SpendData),
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct RedemptionCreateData {
	#[serde(with = "hex")]
	pub owner: Vec<u8>,
	pub value: u128,
	#[serde(with = "hex")]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct RedemptionSpendData {
	#[serde(with = "hex")]
	pub owner: Vec<u8>,
	pub value: u128,
	#[serde(with = "hex")]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
	#[serde(with = "hex")]
	pub spending_tx_hash: [u8; 32],
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct RegistrationData {
	#[serde(with = "hex")]
	pub cardano_address: Vec<u8>,
	#[serde(with = "hex")]
	pub dust_address: Vec<u8>,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct DeregistrationData {
	#[serde(with = "hex")]
	pub cardano_address: Vec<u8>,
	#[serde(with = "hex")]
	pub dust_address: Vec<u8>,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct CreateData {
	pub value: u128,
	#[serde(with = "hex")]
	pub owner: Vec<u8>,
	#[serde(with = "hex")]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct SpendData {
	pub value: u128,
	#[serde(with = "hex")]
	pub owner: Vec<u8>,
	#[serde(with = "hex")]
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
	#[serde(with = "hex")]
	pub spending_tx_hash: [u8; 32],
}

/// Header for an observed UTXO
/// This header can be used for both create and spend events for UTXOs.
/// The ordering assumes that each header is unique per TX i.e. that only one relevant UTXO is included in each transaction
#[derive(
	Debug, Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, PartialEq, Serialize, Deserialize,
)]
pub struct ObservedUtxoHeader {
	/// The position of the observed TX on-chain.
	pub tx_position: CardanoPosition,
	/// The hash of the observed TX.
	pub tx_hash: McTxHash,
	/// The hash of the TX which created the UTXO.
	pub utxo_tx_hash: McTxHash,
	/// The index of the UTXO within the TX which created it.
	pub utxo_index: UtxoIndexInTx,
}
impl ObservedUtxoHeader {
	fn is_spend(&self) -> bool {
		self.tx_hash == self.utxo_tx_hash
	}
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
	Debug,
	Copy,
	Clone,
	PartialEq,
	PartialOrd,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub struct UtxoIndexInTx(pub u16);

impl PartialOrd for ObservedUtxoHeader {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		match self.tx_position.partial_cmp(&other.tx_position) {
			Some(core::cmp::Ordering::Equal) => {},
			ord => return ord,
		}
		if self.is_spend() && !other.is_spend() {
			return Some(core::cmp::Ordering::Less);
		}
		if !self.is_spend() && other.is_spend() {
			return Some(core::cmp::Ordering::Greater);
		}
		// We need an ordering which is consistent between validators,
		// not necessarily the real ordering on-chain.
		// Ordering by hash then index is good enough.
		match self.utxo_tx_hash.0.partial_cmp(&other.utxo_tx_hash.0) {
			Some(core::cmp::Ordering::Equal) => {},
			ord => return ord,
		}
		self.utxo_index.0.partial_cmp(&other.utxo_index.0)
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

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

use midnight_primitives_native_token_observation::CardanoPosition;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sidechain_domain::McTxHash;
use sp_std::vec::Vec;

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
	pub owner: Vec<u8>,
	pub value: u128,
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RedemptionSpendData {
	pub owner: Vec<u8>,
	pub value: u128,
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
	pub spending_tx_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RegistrationData {
	pub cardano_address: Vec<u8>,
	pub dust_address: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct DeregistrationData {
	pub cardano_address: Vec<u8>,
	pub dust_address: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct CreateData {
	pub value: u128,
	pub owner: Vec<u8>,
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct SpendData {
	pub value: u128,
	pub owner: Vec<u8>,
	pub utxo_tx_hash: [u8; 32],
	pub utxo_tx_index: u16,
	pub spending_tx_hash: [u8; 32],
}

/// Header for an observed UTXO
/// This header can be used for both create and spend events for UTXOs.
/// The ordering assumes that each header is unique per TX i.e. that only one relevant UTXO is included in each transaction
#[derive(
	Debug,
	Clone,
	parity_scale_codec::Encode,
	parity_scale_codec::Decode,
	DecodeWithMemTracking,
	scale_info::TypeInfo,
	PartialEq,
)]
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
	Debug,
	Copy,
	Clone,
	PartialEq,
	PartialOrd,
	parity_scale_codec::Encode,
	parity_scale_codec::Decode,
	DecodeWithMemTracking,
	scale_info::TypeInfo,
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

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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;

use frame_support::{storage::bounded_vec::BoundedVec, traits::ConstU32};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sidechain_domain::{McBlockHash, McTxHash};

#[cfg(feature = "std")]
use sqlx::types::chrono::{DateTime, Utc};

/// Addresses are in Bech32 repr. The max length is:
/// max(len('addr'), len('addr_test')) + 1 byte separator + len(bech32_encode(<shelly_address_max = 57 bytes>))
/// = 9 + 1 + 98 = 108
pub const CARDANO_BECH32_ADDRESS_MAX_LENGTH: u32 = 108;
pub const CARDANO_REWARD_ADDRESS_LENGTH: usize = 29;

/// Cardano native-asset policy ID length in bytes (fixed-width per Cardano protocol).
pub const CNIGHT_POLICY_ID_LENGTH: u32 = 28;

/// Cardano native-asset name maximum length in bytes.
pub const CARDANO_ASSET_NAME_MAX_LENGTH: u32 = 32;

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
	PartialOrd,
	Ord,
)]
pub struct CardanoRewardAddressBytes(
	#[serde(with = "hex")] pub [u8; CARDANO_REWARD_ADDRESS_LENGTH],
);

/// The two CIP-19 address-type nibbles that denote a reward (stake) account:
/// 14 = reward account keyed by a stake key hash, 15 = reward account keyed by a
/// script hash. Any other upper nibble is a different address category (Shelley
/// base/pointer/enterprise or Byron) and is not a reward address.
const CARDANO_REWARD_ADDRESS_TYPE_KEY_HASH: u8 = 14;
const CARDANO_REWARD_ADDRESS_TYPE_SCRIPT_HASH: u8 = 15;

/// Why a byte string was rejected as a CIP-19 Cardano reward address.
///
/// Each variant maps to exactly one of the checks [`CardanoRewardAddressBytes::try_new`]
/// runs, so a rejection points at the specific header property that failed rather
/// than collapsing every failure into a single opaque error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardanoRewardAddressError {
	/// The input was not the fixed CIP-19 reward-address length of 29 bytes
	/// (1 header byte + 28 credential bytes).
	WrongLength { found: usize },
	/// The header's upper nibble is not a reward-address type (14 or 15), so the
	/// bytes describe a different Cardano address category.
	WrongAddressType { found: u8 },
	/// The header's lower nibble does not match the network the chain expects
	/// (testnet = 0, mainnet = 1).
	WrongNetwork { expected: u8, found: u8 },
}

impl core::fmt::Display for CardanoRewardAddressError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			CardanoRewardAddressError::WrongLength { found } => write!(
				f,
				"invalid Cardano reward address length: expected {CARDANO_REWARD_ADDRESS_LENGTH} bytes, found {found}"
			),
			CardanoRewardAddressError::WrongAddressType { found } => write!(
				f,
				"invalid Cardano reward address type: header nibble {found} is not a reward account (expected 14 or 15)"
			),
			CardanoRewardAddressError::WrongNetwork { expected, found } => write!(
				f,
				"invalid Cardano reward address network: expected network id {expected}, found {found}"
			),
		}
	}
}

impl CardanoRewardAddressBytes {
	/// Validated CIP-19 constructor: the single trust-boundary entry point that
	/// asserts a byte string is a well-formed reward address for `expected_network`.
	///
	/// Checks run length-first so that reading the header byte is only ever done on
	/// a 29-byte input, then the header's two nibbles: the upper nibble must be a
	/// reward-account type (14 or 15) and the lower nibble must equal
	/// `expected_network`. Uses plain slice/bit arithmetic so it compiles under
	/// `no_std`.
	pub fn try_new(
		bytes: Vec<u8>,
		expected_network: u8,
	) -> Result<Self, CardanoRewardAddressError> {
		if bytes.len() != CARDANO_REWARD_ADDRESS_LENGTH {
			return Err(CardanoRewardAddressError::WrongLength { found: bytes.len() });
		}

		let header = bytes[0];
		let address_type = header >> 4;
		if address_type != CARDANO_REWARD_ADDRESS_TYPE_KEY_HASH
			&& address_type != CARDANO_REWARD_ADDRESS_TYPE_SCRIPT_HASH
		{
			return Err(CardanoRewardAddressError::WrongAddressType { found: address_type });
		}

		let network = header & 0x0F;
		if network != expected_network {
			return Err(CardanoRewardAddressError::WrongNetwork {
				expected: expected_network,
				found: network,
			});
		}

		// Length is confirmed to be exactly 29, so this conversion cannot fail.
		Ok(Self(bytes.try_into().expect("length checked to be 29 above")))
	}
}

impl TryFrom<Vec<u8>> for CardanoRewardAddressBytes {
	type Error = <[u8; CARDANO_REWARD_ADDRESS_LENGTH] as TryFrom<Vec<u8>>>::Error;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Ok(Self(value.try_into()?))
	}
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Clone,
	Eq,
	PartialEq,
	Debug,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
pub struct DustPublicKeyBytes(pub BoundedVec<u8, ConstU32<33>>);

impl Default for DustPublicKeyBytes {
	fn default() -> Self {
		Self(BoundedVec::new())
	}
}

impl TryFrom<Vec<u8>> for DustPublicKeyBytes {
	type Error = <BoundedVec<u8, ConstU32<33>> as TryFrom<Vec<u8>>>::Error;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Ok(Self(value.try_into()?))
	}
}

impl TryFrom<&[u8]> for DustPublicKeyBytes {
	type Error = <DustPublicKeyBytes as TryFrom<Vec<u8>>>::Error;

	fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
		value.to_vec().try_into()
	}
}

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
	pub block_hash: McBlockHash,
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
			self.block_number, self.block_hash, self.tx_index_in_block
		)
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(serde_valid::Validate))]
pub struct CNightAddresses {
	/// Address of the cNight mapping validator. Shelley address, Bech32
	#[cfg_attr(feature = "std", validate(pattern = r"^(addr|addr_test)1[0-9a-z]{1,108}$"))]
	pub mapping_validator_address: String,

	/// Asset name of the auth token. Max length: 32 bytes
	/// [Cardano Source](https://github.com/IntersectMBO/cardano-ledger/blob/683bef2e40cbd10339452c9f2009867c855baf1a/shelley-ma/shelley-ma-test/cddl-files/shelley-ma.cddl#L252)
	#[cfg_attr(feature = "std", validate(max_length = 32))]
	#[cfg_attr(feature = "std", validate(pattern = r"^[\x00-\x7F]*$"))] // Ascii only
	pub auth_token_asset_name: String,

	/// Policy ID of the currency token (i.e. cNIGHT)
	#[serde(with = "hex")]
	pub cnight_policy_id: [u8; 28],

	/// Asset name of the currency token. Max length: 32 bytes
	/// [Cardano Source](https://github.com/IntersectMBO/cardano-ledger/blob/683bef2e40cbd10339452c9f2009867c855baf1a/shelley-ma/shelley-ma-test/cddl-files/shelley-ma.cddl#L252)
	#[cfg_attr(feature = "std", validate(max_length = 32))]
	#[cfg_attr(feature = "std", validate(pattern = r"^[\x00-\x7F]*$"))] // Ascii only
	pub cnight_asset_name: String,
}

impl CardanoPosition {
	/// Increment CardanoPosition to the next tx index.
	/// Useful for pointing to the next-block position
	pub fn increment(mut self) -> Self {
		self.tx_index_in_block += 1;
		self
	}

	/// Lowest position within `block_number` (tx index 0). Only
	/// `(block_number, tx_index_in_block)` are significant when used as a
	/// range bound; `block_hash`/`block_timestamp` are placeholders.
	pub fn min_for_block(block_number: u32) -> Self {
		Self {
			block_hash: McBlockHash([0u8; 32]),
			block_number,
			block_timestamp: Default::default(),
			tx_index_in_block: 0,
		}
	}

	/// Highest position within `block_number`. `tx_index_in_block` is
	/// `i32::MAX` so it survives the `as i32` cast in the SQL bind path
	/// without underflowing to `-1`. Like [`Self::min_for_block`], the
	/// `block_hash`/`block_timestamp` are placeholders.
	pub fn max_for_block(block_number: u32) -> Self {
		Self {
			block_hash: McBlockHash([0u8; 32]),
			block_number,
			block_timestamp: Default::default(),
			tx_index_in_block: u32::try_from(i32::MAX).expect("i32::MAX is non-negative"),
		}
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
	#[cfg_attr(feature = "std", error("Inherent data decode failed"))]
	DecodeFailed,
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
	#[codec(index = 2)]
	Registration(RegistrationData),
	#[codec(index = 3)]
	Deregistration(DeregistrationData),
	#[codec(index = 4)]
	AssetCreate(CreateData),
	#[codec(index = 5)]
	AssetSpend(SpendData),
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct RegistrationData {
	pub cardano_reward_address: CardanoRewardAddressBytes,
	pub dust_public_key: DustPublicKeyBytes,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct DeregistrationData {
	pub cardano_reward_address: CardanoRewardAddressBytes,
	pub dust_public_key: DustPublicKeyBytes,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct CreateData {
	pub value: u128,
	pub owner: CardanoRewardAddressBytes,
	pub utxo_tx_hash: McTxHash,
	pub utxo_tx_index: u16,
}

#[derive(
	Debug, Clone, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize,
)]
pub struct SpendData {
	pub value: u128,
	pub owner: CardanoRewardAddressBytes,
	pub utxo_tx_hash: McTxHash,
	pub utxo_tx_index: u16,
	pub spending_tx_hash: McTxHash,
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
	fn is_create(&self) -> bool {
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
		if self.is_create() && !other.is_create() {
			return Some(core::cmp::Ordering::Less);
		}
		if !self.is_create() && other.is_create() {
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
	// v2 marks the consensus-affecting reduction of the cNight db-sync over-fetch
	// factor from 64x to 4x. Node binaries gate the multiplier on this version so
	// the change only takes effect at the runtime upgrade boundary; mixing old and
	// new binaries against the same runtime version stays consensus-equivalent.
	#[api_version(2)]
	pub trait CNightObservationApi {
		/// Get the contract address on Cardano which emits registration mappings in utxo datums
		fn get_mapping_validator_address() -> Vec<u8>;
		/// Get the Cardano Auth token asset name
		fn get_auth_token_asset_name() -> Vec<u8>;

		/// Get the Cardano CNight token identifier
		fn get_cnight_token_identifier() -> (Vec<u8>, Vec<u8>);

		fn get_next_cardano_position() -> CardanoPosition;

		fn get_cardano_block_window_size() -> u32;

		// Despite the historic name, this returns the per-block *transaction* capacity
		// (`pallet_cnight_observation::CardanoTxCapacityPerBlock`), not a UTXO count.
		// Callers must multiply by the per-tx UTXO over-fetch factor to get a row limit.
		fn get_utxo_capacity_per_block() -> u32;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const MAINNET: u8 = 1;
	const TESTNET: u8 = 0;

	/// Build a 29-byte reward address with the given header byte followed by a
	/// deterministic 28-byte credential.
	fn reward_address_with_header(header: u8) -> Vec<u8> {
		let mut bytes = Vec::with_capacity(CARDANO_REWARD_ADDRESS_LENGTH);
		bytes.push(header);
		bytes.extend((0..28u8).map(|i| i.wrapping_add(1)));
		bytes
	}

	#[test]
	fn try_new_rejects_wrong_length() {
		// 28 bytes (one short) and 30 bytes (one long) both fail on length before
		// the header is ever inspected.
		let short = vec![0u8; CARDANO_REWARD_ADDRESS_LENGTH - 1];
		assert_eq!(
			CardanoRewardAddressBytes::try_new(short, MAINNET),
			Err(CardanoRewardAddressError::WrongLength { found: 28 })
		);

		let long = vec![0u8; CARDANO_REWARD_ADDRESS_LENGTH + 1];
		assert_eq!(
			CardanoRewardAddressBytes::try_new(long, MAINNET),
			Err(CardanoRewardAddressError::WrongLength { found: 30 })
		);
	}

	#[test]
	fn try_new_rejects_wrong_address_type() {
		// Type nibble 0 (Shelley base) with a mainnet network nibble: correct
		// length and network, but not a reward-account type.
		let header = (0 << 4) | MAINNET;
		let bytes = reward_address_with_header(header);
		assert_eq!(
			CardanoRewardAddressBytes::try_new(bytes, MAINNET),
			Err(CardanoRewardAddressError::WrongAddressType { found: 0 })
		);
	}

	#[test]
	fn try_new_rejects_wrong_network() {
		// Valid reward type (14) but the network nibble is mainnet while the chain
		// expects testnet.
		let header = (CARDANO_REWARD_ADDRESS_TYPE_KEY_HASH << 4) | MAINNET;
		let bytes = reward_address_with_header(header);
		assert_eq!(
			CardanoRewardAddressBytes::try_new(bytes, TESTNET),
			Err(CardanoRewardAddressError::WrongNetwork { expected: TESTNET, found: MAINNET })
		);
	}

	#[test]
	fn try_new_rejects_malformed_header() {
		// A 29-byte blob whose header is neither a reward type nor the expected
		// network. The type check runs first, so the specific variant is
		// WrongAddressType (type nibble 8 = Byron), matching PL-2: "malformed" is
		// surfaced as whichever nibble check fails first.
		let header = (8 << 4) | 0x0F;
		let bytes = reward_address_with_header(header);
		assert_eq!(
			CardanoRewardAddressBytes::try_new(bytes, MAINNET),
			Err(CardanoRewardAddressError::WrongAddressType { found: 8 })
		);
	}

	#[test]
	fn try_new_accepts_valid_key_hash_reward_address() {
		// Type 14 (stake key hash) for mainnet round-trips to the exact bytes.
		let header = (CARDANO_REWARD_ADDRESS_TYPE_KEY_HASH << 4) | MAINNET;
		let bytes = reward_address_with_header(header);
		let addr = CardanoRewardAddressBytes::try_new(bytes.clone(), MAINNET)
			.expect("valid type-14 mainnet reward address");
		assert_eq!(addr.0.to_vec(), bytes);
	}

	#[test]
	fn try_new_accepts_valid_script_hash_reward_address() {
		// Type 15 (script hash) for testnet round-trips to the exact bytes.
		let header = (CARDANO_REWARD_ADDRESS_TYPE_SCRIPT_HASH << 4) | TESTNET;
		let bytes = reward_address_with_header(header);
		let addr = CardanoRewardAddressBytes::try_new(bytes.clone(), TESTNET)
			.expect("valid type-15 testnet reward address");
		assert_eq!(addr.0.to_vec(), bytes);
	}
}

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

use std::str::FromStr;

use midnight_node_ledger_helpers::*;
use serde::Deserialize;

use crate::tx_generator::source::FetchCacheConfig;

pub trait TokenDecode: Sized + Send + Sync + Clone {
	fn decode(token_id: [u8; 32]) -> Self;
}

impl TokenDecode for UnshieldedTokenType {
	fn decode(token_id: [u8; 32]) -> Self {
		UnshieldedTokenType(HashOutput(token_id))
	}
}

impl TokenDecode for ShieldedTokenType {
	fn decode(token_id: [u8; 32]) -> Self {
		ShieldedTokenType(HashOutput(token_id))
	}
}

pub fn token_decode<T: TokenDecode>(input: &str) -> Result<T, clap::error::Error> {
	let token_id: [u8; 32] = hex_str_decode(input)?;
	let token = T::decode(token_id);

	Ok(token)
}

pub fn wallet_seed_decode(input: &str) -> Result<WalletSeed, clap::error::Error> {
	input.parse().map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse seed: {}", e)),
		);
		err
	})
}

pub fn keypair_from_str(input: &str) -> Result<Keypair, clap::error::Error> {
	input.parse().map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse keypair: {}", e)),
		);
		err
	})
}

pub fn serde_json_decode<T: for<'a> Deserialize<'a>>(input: &str) -> Result<T, clap::error::Error> {
	serde_json::from_str(input).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse input json: {}", e)),
		);
		err
	})
}

pub fn hex_ledger_decode<T: Deserializable + Tagged>(input: &str) -> Result<T, clap::error::Error> {
	hex_ledger_tagged_decode::<T>(input)
}

pub fn coin_public_decode(input: &str) -> Result<CoinPublicKey, clap::error::Error> {
	hex_ledger_decode(input)
}

pub fn contract_address_decode(input: &str) -> Result<ContractAddress, clap::error::Error> {
	hex_ledger_decode(input)
}

pub fn hex_ledger_untagged_decode<T>(input: &str) -> Result<T, clap::error::Error>
where
	T: Deserializable,
{
	let bytes = hex::decode(input).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid hex input: {}", e)),
		);
		err
	})?;

	let mut cursor = &bytes[..];
	let res = <T as Deserializable>::deserialize(&mut cursor, 0).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to deserialize arg: {e}")),
		);
		err
	})?;

	if !cursor.is_empty() {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!(
				"trailing data after deserialization: {} extra byte(s)",
				cursor.len()
			)),
		);
		return Err(err);
	}

	Ok(res)
}

pub fn hex_ledger_tagged_decode<T>(input: &str) -> Result<T, clap::error::Error>
where
	T: Deserializable + Tagged,
{
	let bytes = hex::decode(input).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse: {}", e)),
		);
		err
	})?;

	let res: T = deserialize(&mut &bytes[..]).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to deserialize arg: {e}")),
		);
		err
	})?;

	Ok(res)
}

pub fn hex_bytes(input: &str) -> Result<Vec<u8>, clap::error::Error> {
	// Remove 0x prefix if present
	let hex_str = input.strip_prefix("0x").unwrap_or(input);
	hex::decode(hex_str).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid hex input: {}", e)),
		);
		err
	})
}

pub fn hex_str_decode<T>(input: &str) -> Result<T, clap::error::Error>
where
	T: TryFrom<Vec<u8>, Error = Vec<u8>>,
{
	let bytes = hex_bytes(input)?;
	let res: T = bytes.try_into().map_err(|e: Vec<u8>| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!(
				"incorrect length for token type string. Expected 32, got {}",
				e.len()
			)),
		);
		err
	})?;

	Ok(res)
}

pub fn fetch_cache_config(input: &str) -> Result<FetchCacheConfig, clap::Error> {
	FetchCacheConfig::from_str(input).map_err(|error| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid fetch cache config: {}", error)),
		);
		err
	})
}

pub fn wallet_address(input: &str) -> Result<WalletAddress, clap::Error> {
	WalletAddress::from_str(input).map_err(|error| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid wallet address: {}", error)),
		);
		err
	})
}

pub fn utxo_id_decode(input: &str) -> Result<UtxoId, clap::Error> {
	UtxoId::from_str(input).map_err(|error| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid utxo id: {}", error)),
		);
		err
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Helper: serialize a Tagged value to its tagged hex representation.
	fn to_tagged_hex<T: Serializable + Tagged>(val: &T) -> String {
		let bytes = serialize(val).expect("serialization should succeed");
		hex::encode(bytes)
	}

	#[test]
	fn coin_public_decode_accepts_tagged_input() {
		let key = CoinPublicKey(HashOutput([0u8; 32]));
		let tagged_hex = to_tagged_hex(&key);
		assert!(coin_public_decode(&tagged_hex).is_ok());
	}

	#[test]
	fn contract_address_decode_accepts_tagged_input() {
		let tagged_hex =
			include_str!("../../../../res/test-contract/contract_address_undeployed_tagged.mn")
				.trim();
		assert!(contract_address_decode(tagged_hex).is_ok());
	}

	#[test]
	fn coin_public_decode_rejects_untagged_input() {
		let res = coin_public_decode(&"0".repeat(64)); // 32 bytes raw, no tag
		assert!(res.is_err(), "untagged input should be rejected for CoinPublicKey");
	}

	#[test]
	fn contract_address_decode_rejects_untagged_input() {
		let res = contract_address_decode(&"0".repeat(64)); // 32 bytes raw, no tag
		assert!(res.is_err(), "untagged input should be rejected for ContractAddress");
	}

	#[test]
	fn coin_public_decode_rejects_wrong_tag() {
		// Serialize a ContractAddress and try to decode as CoinPublicKey — tags differ.
		let addr = ContractAddress(HashOutput([0u8; 32]));
		let wrong_tag_hex = to_tagged_hex(&addr);
		let res = coin_public_decode(&wrong_tag_hex);
		assert!(res.is_err(), "wrong tag should be rejected");
	}

	#[test]
	fn contract_address_decode_rejects_trailing_bytes() {
		let tagged_hex =
			include_str!("../../../../res/test-contract/contract_address_undeployed_tagged.mn")
				.trim();
		let with_trailing = format!("{}00", tagged_hex);
		let res = contract_address_decode(&with_trailing);
		assert!(res.is_err(), "trailing bytes should be rejected (EOF enforcement)");
	}

	#[test]
	fn coin_public_decode_rejects_invalid_hex() {
		let res = coin_public_decode("not-valid-hex!!");
		assert!(res.is_err(), "invalid hex should be rejected");
	}

	#[test]
	fn contract_address_decode_rejects_invalid_hex() {
		let res = contract_address_decode("zzzz");
		assert!(res.is_err(), "invalid hex should be rejected");
	}

	#[test]
	fn hex_ledger_untagged_decode_enforces_eof() {
		// HashOutput is 32 bytes; 33 bytes of data should fail
		let res = hex_ledger_untagged_decode::<HashOutput>(&"ab".repeat(33));
		assert!(res.is_err(), "trailing data in untagged decode should be rejected");
	}

	#[test]
	fn hex_ledger_untagged_decode_accepts_exact_length() {
		let res = hex_ledger_untagged_decode::<HashOutput>(&"00".repeat(32));
		assert!(res.is_ok(), "exact-length untagged input should succeed");
	}

	#[test]
	fn tagged_decode_rejects_trailing_bytes() {
		let key = CoinPublicKey(HashOutput([0u8; 32]));
		let mut tagged_hex = to_tagged_hex(&key);
		tagged_hex.push_str("ff");
		let res = coin_public_decode(&tagged_hex);
		assert!(res.is_err(), "tagged decode should reject trailing bytes");
	}
}

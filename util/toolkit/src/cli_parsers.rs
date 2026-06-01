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

pub fn coin_selection_strategy(input: &str) -> Result<CoinSelectionStrategy, clap::error::Error> {
	match input {
		"largest-first" => Ok(CoinSelectionStrategy::LargestFirst),
		"smallest-first" => Ok(CoinSelectionStrategy::SmallestFirst),
		other => {
			let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
			err.insert(
				clap::error::ContextKind::Custom,
				clap::error::ContextValue::String(format!(
					"invalid coin selection strategy '{}': expected 'largest-first' or 'smallest-first'",
					other
				)),
			);
			Err(err)
		},
	}
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

// ADR-0022: wallet keys and addresses (including contract addresses and coin
// public keys) use *untagged* serialization. They are surfaced to users as
// Bech32m, where the human-readable-part already plays the role of a tag.
// Switching to `hex_ledger_decode` (tagged) was tried and reverted in PR #853;
// do not re-introduce it without first updating ADR-0022. EOF enforcement in
// `hex_ledger_untagged_decode` is the audit-#307 hardening that closes the
// silent-fallback ambiguity surface without changing the wire format.
pub fn coin_public_decode(input: &str) -> Result<CoinPublicKey, clap::error::Error> {
	hex_ledger_untagged_decode(input)
}

// ADR-0022: see the comment on `coin_public_decode`. `ContractAddress` is in
// the same untagged set; switching to tagged decoding was reverted in PR #853
// and must not be re-introduced without first updating ADR-0022.
pub fn contract_address_decode(input: &str) -> Result<ContractAddress, clap::error::Error> {
	hex_ledger_untagged_decode(input)
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

/// A single per-destination output spec parsed from a `--output` flag value.
///
/// The address HRP determines whether this is a shielded or unshielded output.
/// `token_type` is optional; callers default it to the all-zeros token type
/// when not provided.
#[derive(Clone, Debug)]
pub struct OutputArg {
	pub address: WalletAddress,
	pub amount: u128,
	pub token_type: Option<[u8; 32]>,
}

fn value_validation_err(message: String) -> clap::Error {
	let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
	err.insert(clap::error::ContextKind::Custom, clap::error::ContextValue::String(message));
	err
}

/// Parse a single `--output` value of the form
/// `addr=<bech32>,amount=<u128>[,token=<32-byte-hex>]`.
///
/// Keys are matched case-sensitively. `addr`/`address` and `token`/`token_type`
/// are accepted as aliases. Order of keys does not matter. Whitespace around
/// keys and values is trimmed. Trailing or empty comma-separated segments are
/// ignored.
pub fn output_arg_decode(input: &str) -> Result<OutputArg, clap::Error> {
	let mut addr: Option<&str> = None;
	let mut amount_raw: Option<&str> = None;
	let mut token_raw: Option<&str> = None;

	for part in input.split(',') {
		let part = part.trim();
		if part.is_empty() {
			continue;
		}
		let (k, v) = part.split_once('=').ok_or_else(|| {
			value_validation_err(format!(
				"invalid --output segment '{part}': expected key=value (e.g. addr=mn_addr1...,amount=100)"
			))
		})?;
		let k = k.trim();
		let v = v.trim();
		match k {
			"addr" | "address" => {
				if addr.is_some() {
					return Err(value_validation_err(format!("--output has duplicate '{k}' key")));
				}
				addr = Some(v);
			},
			"amount" => {
				if amount_raw.is_some() {
					return Err(value_validation_err("--output has duplicate 'amount' key".into()));
				}
				amount_raw = Some(v);
			},
			"token" | "token_type" => {
				if token_raw.is_some() {
					return Err(value_validation_err(format!("--output has duplicate '{k}' key")));
				}
				token_raw = Some(v);
			},
			other => {
				return Err(value_validation_err(format!(
					"--output has unknown key '{other}'; expected one of: addr, amount, token"
				)));
			},
		}
	}

	let addr_str =
		addr.ok_or_else(|| value_validation_err("--output is missing required 'addr' key".into()))?;
	let amount_str = amount_raw
		.ok_or_else(|| value_validation_err("--output is missing required 'amount' key".into()))?;

	let address = WalletAddress::from_str(addr_str).map_err(|error| {
		value_validation_err(format!("--output has invalid address '{addr_str}': {error}"))
	})?;
	let amount = amount_str.parse::<u128>().map_err(|error| {
		value_validation_err(format!("--output has invalid amount '{amount_str}': {error}"))
	})?;
	let token_type = token_raw.map(hex_str_decode::<[u8; 32]>).transpose()?;

	Ok(OutputArg { address, amount, token_type })
}

#[cfg(test)]
mod tests {
	use super::*;

	// `coin_public_decode` — untagged per ADR-0022.

	#[test]
	fn coin_public_decode_accepts_untagged_input() {
		// 32-byte all-zeros payload — the untagged decoder consumes exactly 32 bytes.
		let res = coin_public_decode(&"00".repeat(32));
		assert!(res.is_ok(), "valid untagged 32-byte input should decode");
	}

	#[test]
	fn coin_public_decode_rejects_trailing_bytes() {
		// Valid 32-byte payload plus one extra byte — EOF enforcement should reject.
		let with_trailing = format!("{}00", "00".repeat(32));
		let res = coin_public_decode(&with_trailing);
		assert!(res.is_err(), "trailing bytes should be rejected (EOF enforcement)");
	}

	#[test]
	fn coin_public_decode_rejects_truncated_input() {
		// 30 bytes (60 hex chars) — short of the 32-byte payload.
		let res = coin_public_decode(&"00".repeat(30));
		assert!(res.is_err(), "truncated input should be rejected");
	}

	#[test]
	fn coin_public_decode_rejects_invalid_hex() {
		let res = coin_public_decode("not-valid-hex!!");
		assert!(res.is_err(), "invalid hex should be rejected");
	}

	// `contract_address_decode` — untagged per ADR-0022.

	#[test]
	fn contract_address_decode_accepts_untagged_input() {
		// Reuse the canonical untagged fixture also consumed by `generate_txs.rs`.
		let untagged_hex =
			include_str!("../../../res/test-contract/contract_address_undeployed.mn").trim();
		assert!(
			contract_address_decode(untagged_hex).is_ok(),
			"valid untagged ContractAddress hex should decode"
		);
	}

	#[test]
	fn contract_address_decode_rejects_trailing_bytes() {
		let untagged_hex =
			include_str!("../../../res/test-contract/contract_address_undeployed.mn").trim();
		let with_trailing = format!("{untagged_hex}00");
		let res = contract_address_decode(&with_trailing);
		assert!(res.is_err(), "trailing bytes should be rejected (EOF enforcement)");
	}

	#[test]
	fn contract_address_decode_rejects_truncated_input() {
		// 30 bytes — short of the 32-byte payload.
		let res = contract_address_decode(&"00".repeat(30));
		assert!(res.is_err(), "truncated input should be rejected");
	}

	#[test]
	fn contract_address_decode_rejects_invalid_hex() {
		let res = contract_address_decode("zzzz");
		assert!(res.is_err(), "invalid hex should be rejected");
	}

	// `hex_ledger_untagged_decode::<HashOutput>` — the audit-#307 EOF hardening.

	#[test]
	fn hex_ledger_untagged_decode_accepts_exact_length() {
		let res = hex_ledger_untagged_decode::<HashOutput>(&"00".repeat(32));
		assert!(res.is_ok(), "exact-length untagged input should succeed");
	}

	#[test]
	fn hex_ledger_untagged_decode_rejects_trailing_bytes() {
		// 33 bytes — one byte too many.
		let res = hex_ledger_untagged_decode::<HashOutput>(&"ab".repeat(33));
		assert!(res.is_err(), "trailing data in untagged decode should be rejected");
	}

	#[test]
	fn hex_ledger_untagged_decode_rejects_truncated_input() {
		// 30 bytes — short of the 32-byte payload.
		let res = hex_ledger_untagged_decode::<HashOutput>(&"ab".repeat(30));
		assert!(res.is_err(), "truncated input should be rejected");
	}

	// `output_arg_decode` — `--output addr=...,amount=...[,token=...]`.

	// Reused address fixtures (also used elsewhere in the toolkit test suite).
	const UNSHIELDED_ADDR: &str =
		"mn_addr_undeployed13h0e3c2m7rcfem6wvjljnyjmxy5rkg9kkwcldzt73ya5pv7c4p8skzgqwj";
	const SHIELDED_ADDR: &str = "mn_shield-addr_undeployed1tdu4jzhm7xn9qhzwweleyszxmhtt7fnzfhql42g87aay2jdjvau3fljgum7nqky8cj5mmm697rd33uyh6dnw42thuucjp7da74nje0sggh42d";

	#[test]
	fn output_arg_decode_minimum_required_fields() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=42");
		let out = output_arg_decode(&s).expect("addr+amount should suffice");
		assert_eq!(out.amount, 42);
		assert!(out.token_type.is_none(), "token should default to None when not provided");
	}

	#[test]
	fn output_arg_decode_with_token() {
		let token_hex = "0000000000000000000000000000000000000000000000000000000000000001";
		let s = format!("addr={SHIELDED_ADDR},amount=41,token={token_hex}");
		let out = output_arg_decode(&s).expect("full triple should parse");
		let mut expected = [0u8; 32];
		expected[31] = 1;
		assert_eq!(out.amount, 41);
		assert_eq!(out.token_type, Some(expected));
	}

	#[test]
	fn output_arg_decode_key_order_agnostic_and_aliases() {
		let s = format!("amount=7, address={UNSHIELDED_ADDR} , token_type={}", "00".repeat(32));
		let out = output_arg_decode(&s).expect("keys should be order-agnostic, aliases honoured");
		assert_eq!(out.amount, 7);
		assert_eq!(out.token_type, Some([0u8; 32]));
	}

	#[test]
	fn output_arg_decode_rejects_missing_addr() {
		let res = output_arg_decode("amount=10");
		assert!(res.is_err(), "missing addr must fail");
	}

	#[test]
	fn output_arg_decode_rejects_missing_amount() {
		let s = format!("addr={UNSHIELDED_ADDR}");
		let res = output_arg_decode(&s);
		assert!(res.is_err(), "missing amount must fail");
	}

	#[test]
	fn output_arg_decode_rejects_unknown_key() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,oops=1");
		let res = output_arg_decode(&s);
		assert!(res.is_err(), "unknown key must fail");
	}

	#[test]
	fn output_arg_decode_rejects_duplicate_key() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,amount=20");
		let res = output_arg_decode(&s);
		assert!(res.is_err(), "duplicate amount key must fail");
	}

	#[test]
	fn output_arg_decode_rejects_segment_without_equals() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,oops");
		let res = output_arg_decode(&s);
		assert!(res.is_err(), "segment missing '=' must fail");
	}

	#[test]
	fn output_arg_decode_rejects_invalid_amount() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=not_a_number");
		let res = output_arg_decode(&s);
		assert!(res.is_err(), "non-numeric amount must fail");
	}

	#[test]
	fn output_arg_decode_rejects_invalid_address() {
		let res = output_arg_decode("addr=not_a_bech32,amount=10");
		assert!(res.is_err(), "invalid bech32 address must fail");
	}

	#[test]
	fn output_arg_decode_rejects_invalid_token_length() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,token=00");
		let res = output_arg_decode(&s);
		assert!(res.is_err(), "non-32-byte token must fail");
	}

	#[test]
	fn hex_ledger_untagged_decode_rejects_invalid_hex() {
		let res = hex_ledger_untagged_decode::<HashOutput>("not-valid-hex!!");
		assert!(res.is_err(), "invalid hex should be rejected");
	}
}

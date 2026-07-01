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

//! Parsing for the `--output` flag of `generate-txs single-tx`.
//!
//! Each `--output` value is a comma-separated bag of `key=value` pairs
//! describing a single tx destination, parsed into [`OutputArg`]. Errors
//! are reported as [`DecodeError`] so this module stays free of the clap
//! dependency; the thin wrapper in `cli_parsers.rs` handles the
//! conversion to `clap::Error` for use as a `value_parser`.

use std::str::FromStr;

use midnight_node_ledger_helpers::WalletAddress;

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

/// Errors produced by [`decode`].
///
/// Display strings are stable user-facing CLI error text — the clap wrapper
/// in [`super::output_arg_decode`] forwards them verbatim into the
/// `clap::Error` it returns.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
	#[error(
		"invalid --output segment '{0}': expected key=value (e.g. addr=mn_addr1...,amount=100)"
	)]
	InvalidSegment(String),

	#[error("--output has duplicate '{0}' key")]
	DuplicateKey(String),

	#[error("--output has unknown key '{0}'; expected one of: addr, amount, token")]
	UnknownKey(String),

	#[error("--output is missing required '{0}' key")]
	MissingKey(&'static str),

	#[error("--output has invalid address '{input}': {message}")]
	InvalidAddress { input: String, message: String },

	#[error("--output has invalid amount '{input}': {message}")]
	InvalidAmount { input: String, message: String },

	#[error("--output has invalid token hex '{input}': {message}")]
	InvalidTokenHex { input: String, message: String },

	#[error("--output token must be 32 bytes; got {0}")]
	TokenWrongLength(usize),
}

/// Parse a single `--output` value of the form
/// `addr=<bech32>,amount=<u128>[,token=<32-byte-hex>]`.
///
/// Keys are matched case-sensitively. `addr`/`address` and `token`/`token_type`
/// are accepted as aliases. Order of keys does not matter. Whitespace around
/// keys and values is trimmed. Trailing or empty comma-separated segments are
/// ignored.
pub fn decode(input: &str) -> Result<OutputArg, DecodeError> {
	let mut addr: Option<&str> = None;
	let mut amount_raw: Option<&str> = None;
	let mut token_raw: Option<&str> = None;

	for part in input.split(',') {
		let part = part.trim();
		if part.is_empty() {
			continue;
		}
		let (k, v) = part
			.split_once('=')
			.ok_or_else(|| DecodeError::InvalidSegment(part.to_string()))?;
		let k = k.trim();
		let v = v.trim();
		match k {
			"addr" | "address" => {
				if addr.is_some() {
					return Err(DecodeError::DuplicateKey(k.to_string()));
				}
				addr = Some(v);
			},
			"amount" => {
				if amount_raw.is_some() {
					return Err(DecodeError::DuplicateKey(k.to_string()));
				}
				amount_raw = Some(v);
			},
			"token" | "token_type" => {
				if token_raw.is_some() {
					return Err(DecodeError::DuplicateKey(k.to_string()));
				}
				token_raw = Some(v);
			},
			other => {
				return Err(DecodeError::UnknownKey(other.to_string()));
			},
		}
	}

	let addr_str = addr.ok_or(DecodeError::MissingKey("addr"))?;
	let amount_str = amount_raw.ok_or(DecodeError::MissingKey("amount"))?;

	let address = WalletAddress::from_str(addr_str).map_err(|error| {
		DecodeError::InvalidAddress { input: addr_str.to_string(), message: error.to_string() }
	})?;
	let amount = amount_str.parse::<u128>().map_err(|error| DecodeError::InvalidAmount {
		input: amount_str.to_string(),
		message: error.to_string(),
	})?;
	let token_type = token_raw.map(decode_token_hex).transpose()?;

	Ok(OutputArg { address, amount, token_type })
}

fn decode_token_hex(input: &str) -> Result<[u8; 32], DecodeError> {
	let stripped = input.strip_prefix("0x").unwrap_or(input);
	let bytes = hex::decode(stripped).map_err(|error| DecodeError::InvalidTokenHex {
		input: input.to_string(),
		message: error.to_string(),
	})?;
	bytes.try_into().map_err(|v: Vec<u8>| DecodeError::TokenWrongLength(v.len()))
}

#[cfg(test)]
mod tests {
	use super::*;

	// Reused address fixtures (also used elsewhere in the toolkit test suite).
	const UNSHIELDED_ADDR: &str =
		"mn_addr_undeployed13h0e3c2m7rcfem6wvjljnyjmxy5rkg9kkwcldzt73ya5pv7c4p8skzgqwj";
	const SHIELDED_ADDR: &str = "mn_shield-addr_undeployed1tdu4jzhm7xn9qhzwweleyszxmhtt7fnzfhql42g87aay2jdjvau3fljgum7nqky8cj5mmm697rd33uyh6dnw42thuucjp7da74nje0sggh42d";

	#[test]
	fn decode_minimum_required_fields() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=42");
		let out = decode(&s).expect("addr+amount should suffice");
		assert_eq!(out.amount, 42);
		assert!(out.token_type.is_none(), "token should default to None when not provided");
	}

	#[test]
	fn decode_with_token() {
		let token_hex = "0000000000000000000000000000000000000000000000000000000000000001";
		let s = format!("addr={SHIELDED_ADDR},amount=41,token={token_hex}");
		let out = decode(&s).expect("full triple should parse");
		let mut expected = [0u8; 32];
		expected[31] = 1;
		assert_eq!(out.amount, 41);
		assert_eq!(out.token_type, Some(expected));
	}

	#[test]
	fn decode_key_order_agnostic_and_aliases() {
		let s = format!("amount=7, address={UNSHIELDED_ADDR} , token_type={}", "00".repeat(32));
		let out = decode(&s).expect("keys should be order-agnostic, aliases honoured");
		assert_eq!(out.amount, 7);
		assert_eq!(out.token_type, Some([0u8; 32]));
	}

	#[test]
	fn decode_rejects_missing_addr() {
		assert!(matches!(decode("amount=10"), Err(DecodeError::MissingKey("addr"))));
	}

	#[test]
	fn decode_rejects_missing_amount() {
		let s = format!("addr={UNSHIELDED_ADDR}");
		assert!(matches!(decode(&s), Err(DecodeError::MissingKey("amount"))));
	}

	#[test]
	fn decode_rejects_unknown_key() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,oops=1");
		assert!(matches!(decode(&s), Err(DecodeError::UnknownKey(ref k)) if k == "oops"));
	}

	#[test]
	fn decode_rejects_duplicate_key() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,amount=20");
		assert!(matches!(decode(&s), Err(DecodeError::DuplicateKey(ref k)) if k == "amount"));
	}

	#[test]
	fn decode_rejects_segment_without_equals() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,oops");
		assert!(matches!(decode(&s), Err(DecodeError::InvalidSegment(ref s)) if s == "oops"));
	}

	#[test]
	fn decode_rejects_invalid_amount() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=not_a_number");
		assert!(matches!(decode(&s), Err(DecodeError::InvalidAmount { .. })));
	}

	#[test]
	fn decode_rejects_invalid_address() {
		assert!(matches!(
			decode("addr=not_a_bech32,amount=10"),
			Err(DecodeError::InvalidAddress { .. })
		));
	}

	#[test]
	fn decode_rejects_invalid_token_length() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,token=00");
		assert!(matches!(decode(&s), Err(DecodeError::TokenWrongLength(1))));
	}

	#[test]
	fn decode_rejects_invalid_token_hex() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,token=zz");
		assert!(matches!(decode(&s), Err(DecodeError::InvalidTokenHex { .. })));
	}
}

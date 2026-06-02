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
//! describing a single tx destination, parsed into [`OutputArg`].

use std::str::FromStr;

use midnight_node_ledger_helpers::WalletAddress;

use super::hex_str_decode;

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
pub fn decode(input: &str) -> Result<OutputArg, clap::Error> {
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
		let res = decode("amount=10");
		assert!(res.is_err(), "missing addr must fail");
	}

	#[test]
	fn decode_rejects_missing_amount() {
		let s = format!("addr={UNSHIELDED_ADDR}");
		let res = decode(&s);
		assert!(res.is_err(), "missing amount must fail");
	}

	#[test]
	fn decode_rejects_unknown_key() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,oops=1");
		let res = decode(&s);
		assert!(res.is_err(), "unknown key must fail");
	}

	#[test]
	fn decode_rejects_duplicate_key() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,amount=20");
		let res = decode(&s);
		assert!(res.is_err(), "duplicate amount key must fail");
	}

	#[test]
	fn decode_rejects_segment_without_equals() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,oops");
		let res = decode(&s);
		assert!(res.is_err(), "segment missing '=' must fail");
	}

	#[test]
	fn decode_rejects_invalid_amount() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=not_a_number");
		let res = decode(&s);
		assert!(res.is_err(), "non-numeric amount must fail");
	}

	#[test]
	fn decode_rejects_invalid_address() {
		let res = decode("addr=not_a_bech32,amount=10");
		assert!(res.is_err(), "invalid bech32 address must fail");
	}

	#[test]
	fn decode_rejects_invalid_token_length() {
		let s = format!("addr={UNSHIELDED_ADDR},amount=10,token=00");
		let res = decode(&s);
		assert!(res.is_err(), "non-32-byte token must fail");
	}
}

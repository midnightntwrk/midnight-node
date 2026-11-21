// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Decoding of the datum and metadata CBOR served by mc-follower into the values the db-sync
//! data sources read from db-sync's JSON

use cardano_serialization_lib::{
	PlutusData, PlutusDatumSchema, TransactionMetadatum, TransactionMetadatumKind,
	decode_plutus_datum_to_json_str, encode_json_value_to_plutus_datum,
};
use serde_json::Value as JsonValue;

/// Deepest nesting of arrays, maps and tags in datum CBOR handed to the recursive decoders.
/// db-sync's datum JSON fails to decode when nested 128 levels or deeper (serde_json's limit).
/// In the detailed schema JSON every CBOR level adds at least 2/3 of a JSON level:
/// - a list, one CBOR array, is `{"list":[..]}`: two levels
/// - a map, one CBOR map, is `{"map":[{"k":..,"v":..}]}`: three levels
/// - a constructor, a tag and an array, or with tag 102 a tag and two arrays, is
///   `{"constructor":..,"fields":[..]}`: two levels
/// - a big integer, a tag, is `{"int":..}`: one level
///
/// So a datum nested 192 levels deep in CBOR is at least 128 deep in JSON, and fails either way.
const MAX_DATUM_CBOR_DEPTH: usize = 191;

/// Decodes datum CBOR through the detailed schema JSON and serde_json, as db-sync datums are
/// decoded, so that the same datums decode and to the same values (e.g. serde_json's nesting
/// limit applies alike)
pub(crate) fn plutus_data_like_db_sync(cbor: &[u8]) -> Result<PlutusData, String> {
	if nested_deeper_than(cbor, MAX_DATUM_CBOR_DEPTH) {
		return Err("datum nested too deeply".to_string());
	}
	let datum = PlutusData::from_bytes(cbor.to_vec()).map_err(|e| e.to_string())?;
	let json = decode_plutus_datum_to_json_str(&datum, PlutusDatumSchema::DetailedSchema)
		.map_err(|e| format!("{e:?}"))?;
	let value: JsonValue = serde_json::from_str(&json).map_err(|e| e.to_string())?;
	encode_json_value_to_plutus_datum(value, PlutusDatumSchema::DetailedSchema)
		.map_err(|e| format!("{e:?}"))
}

/// An undecodable datum is absent, as db-sync's `DbDatumIfValid`
pub(crate) fn plutus_data_if_valid(cbor: Option<&[u8]>) -> Option<PlutusData> {
	cbor.and_then(|cbor| plutus_data_like_db_sync(cbor).ok())
}

/// The bridge transfer metadatum as db-sync renders it: a list of one bytes or text element
/// becomes an array of one string (bytes as `0x` hex), anything else is `None`
pub(crate) fn c2m_metadata_like_db_sync(cbor: &[u8]) -> Option<JsonValue> {
	// Only a list of one bytes or text element is read, so anything nested deeper is `None`
	if nested_deeper_than(cbor, 1) {
		return None;
	}
	let metadatum = TransactionMetadatum::from_bytes(cbor.to_vec()).ok()?;
	let list = metadatum.as_list().ok()?;
	if list.len() != 1 {
		return None;
	}
	let element = list.get(0);
	let rendered = match element.kind() {
		TransactionMetadatumKind::Bytes => format!("0x{}", hex::encode(element.as_bytes().ok()?)),
		TransactionMetadatumKind::Text => element.as_text().ok()?,
		_ => return None,
	};
	Some(JsonValue::Array(vec![JsonValue::String(rendered)]))
}

/// Whether the first CBOR item of `cbor` nests arrays, maps and tags deeper than `max_depth`.
/// Reads iteratively, holding at most `max_depth` frames. Malformed or truncated CBOR is not
/// too deep, so that it fails to decode as before.
fn nested_deeper_than(cbor: &[u8], max_depth: usize) -> bool {
	// Items left in each open array, map or tag, `None` until a break when indefinite
	let mut open: Vec<Option<u64>> = Vec::new();
	// Whether the innermost open item is an indefinite length byte or text string
	let mut in_chunks = false;
	let mut pos = 0;
	loop {
		let Some(&initial) = cbor.get(pos) else { return false };
		pos += 1;
		let (major, info) = (initial >> 5, initial & 0x1f);
		if initial == 0xff {
			if in_chunks {
				in_chunks = false;
			} else if let Some(None) = open.last() {
				open.pop();
			} else {
				return false;
			}
		} else {
			let argument = match info {
				0..=23 => Some(u64::from(info)),
				24..=27 => {
					let len = 1 << (info - 24);
					let Some(bytes) = cbor.get(pos..pos + len) else { return false };
					pos += len;
					Some(bytes.iter().fold(0, |n, byte| n << 8 | u64::from(*byte)))
				},
				31 => None,
				_ => return false,
			};
			if in_chunks && (!matches!(major, 2 | 3) || argument.is_none()) {
				return false;
			}
			match (major, argument) {
				(0 | 1 | 7, Some(_)) => {},
				(2 | 3, Some(len)) => {
					match usize::try_from(len).ok().and_then(|len| pos.checked_add(len)) {
						Some(end) if end <= cbor.len() => pos = end,
						_ => return false,
					}
				},
				(2 | 3, None) => {
					in_chunks = true;
					continue;
				},
				(4 | 5, _) | (6, Some(_)) => {
					let items = match major {
						4 => argument,
						5 => argument.map(|pairs| pairs.saturating_mul(2)),
						_ => Some(1),
					};
					if items != Some(0) {
						if open.len() == max_depth {
							return true;
						}
						open.push(items);
						continue;
					}
				},
				_ => return false,
			}
		}
		if in_chunks {
			continue;
		}
		// An item is complete: count it off its enclosing items, closing those it completes
		loop {
			match open.last_mut() {
				None => return false,
				Some(Some(left)) => {
					*left -= 1;
					if *left > 0 {
						break;
					}
					open.pop();
				},
				Some(None) => break,
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn deeply_nested_cbor_fails_as_db_sync_json_does() {
		let nested = |open: &[u8], close: &[u8]| {
			[open.repeat(10_000), vec![0x00], close.repeat(10_000)].concat()
		};
		let cases = [
			("arrays", nested(&[0x81], &[])),
			("indefinite arrays", nested(&[0x9f], &[0xff])),
			("maps", nested(&[0xa1, 0x00], &[])),
			("indefinite maps", nested(&[0xbf, 0x00], &[0xff])),
			("tags", nested(&[0xd8, 0x79], &[])),
			("constructors", nested(&[0xd8, 0x79, 0x81], &[])),
			("tag 102 constructors", nested(&[0xd8, 0x66, 0x82, 0x00, 0x81], &[])),
		];
		for (name, cbor) in cases {
			assert!(plutus_data_like_db_sync(&cbor).is_err(), "{name}");
			assert_eq!(plutus_data_if_valid(Some(&cbor)), None, "{name}");
			assert_eq!(c2m_metadata_like_db_sync(&cbor), None, "{name}");
		}
	}
}

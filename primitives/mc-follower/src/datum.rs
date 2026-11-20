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

//! Decoding of the datum and metadata bytes served by mc-follower
//!
//! db-sync stores datums and transaction metadata as JSON, which the db-sync data sources
//! decode with serde_json. mc-follower serves the original CBOR instead. To produce the same
//! values, and the same "absent" or "undecodable" outcomes, datums are decoded through the same
//! JSON representation and the same JSON parser.

use cardano_serialization_lib::{
	PlutusData, PlutusDatumSchema, TransactionMetadatum, TransactionMetadatumKind,
	decode_plutus_datum_to_json_str, encode_json_value_to_plutus_datum,
};
use serde_json::Value as JsonValue;

/// Why a datum could not be decoded
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DatumError {
	/// The bytes are not CBOR Plutus data
	#[error("datum is not Plutus data: {0}")]
	NotPlutusData(String),
	/// The datum has no detailed schema JSON form
	#[error("datum cannot be rendered as JSON: {0}")]
	NotRenderable(String),
	/// serde_json rejects the JSON form of the datum, for example because it nests too deeply
	#[error("JSON form of the datum is rejected: {0}")]
	JsonRejected(String),
	/// The JSON form of the datum does not encode back to Plutus data
	#[error("JSON form of the datum is not Plutus data: {0}")]
	NotEncodable(String),
}

/// Decodes datum CBOR into the Plutus data that db-sync's JSON form of the datum decodes to
///
/// The db-sync data sources read `datum.value`, the datum in cardano-api's detailed schema JSON,
/// parse it with serde_json and encode it with [encode_json_value_to_plutus_datum]. This renders
/// the datum in the same detailed schema JSON and takes the same last two steps, so the 128
/// level nesting limit of serde_json and the integer handling of the JSON route apply alike.
pub fn plutus_data_like_db_sync(cbor: &[u8]) -> Result<PlutusData, DatumError> {
	let datum = PlutusData::from_bytes(cbor.to_vec())
		.map_err(|e| DatumError::NotPlutusData(e.to_string()))?;
	let json = decode_plutus_datum_to_json_str(&datum, PlutusDatumSchema::DetailedSchema)
		.map_err(|e| DatumError::NotRenderable(format!("{e:?}")))?;
	let value: JsonValue =
		serde_json::from_str(&json).map_err(|e| DatumError::JsonRejected(e.to_string()))?;
	encode_json_value_to_plutus_datum(value, PlutusDatumSchema::DetailedSchema)
		.map_err(|e| DatumError::NotEncodable(format!("{e:?}")))
}

/// Datum of a row decoded leniently, as db-sync's `DbDatumIfValid`: an undecodable datum is
/// absent, and the row is kept
pub fn plutus_data_if_valid(cbor: Option<&[u8]>) -> Option<PlutusData> {
	cbor.and_then(|cbor| plutus_data_like_db_sync(cbor).ok())
}

/// The token bridge transfer metadatum as the bridge data source reads it from db-sync
///
/// The bridge only accepts, as the JSON db-sync renders the metadatum in, an array of one
/// string that is `0x` followed by the hex encoding of the recipient. db-sync renders
/// metadata bytes as such a string and metadata text as a JSON string, so a metadatum that is
/// a list of one element, either bytes or text, is returned as an array of one string, and
/// any other metadatum, which the bridge treats as an invalid recipient, as `None`.
pub fn c2m_metadata_like_db_sync(cbor: &[u8]) -> Option<JsonValue> {
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

#[cfg(test)]
mod tests {
	use super::*;
	use cardano_serialization_lib::{BigInt, MetadataList, PlutusList};

	/// The db-sync route of the data sources (`DbDatumIfValid`): db-sync JSON parsed by
	/// serde_json, then encoded as Plutus data, absent on any failure
	fn db_sync_route(db_sync_json: &str) -> Option<PlutusData> {
		let value: JsonValue = serde_json::from_str(db_sync_json).ok()?;
		encode_json_value_to_plutus_datum(value, PlutusDatumSchema::DetailedSchema).ok()
	}

	fn nested_lists_json(depth: usize) -> String {
		"{ \"list\": [".repeat(depth) + "{ \"int\": 1 }" + &"] }".repeat(depth)
	}

	/// CBOR of `depth` nested one-element definite lists around the integer 1
	fn nested_lists_cbor(depth: usize) -> Vec<u8> {
		let mut cbor = vec![0x81; depth];
		cbor.push(0x01);
		cbor
	}

	// db-sync JSON datums that decode on the db-sync route: those of the fixtures in
	// partner-chains/toolkit/data-sources/db-sync/testdata/migrations-tx-in-enabled/
	// 6_insert_transactions.sql (some shortened), plus a map and a negative wide integer.
	const DECODABLE_FIXTURE_DATUMS: &[&str] = &[
		r#"{ "constructor": 0, "fields": [{ "int": 1 }] }"#,
		r#"{"list": [{"int": 1}, {"int": 2}]}"#,
		r#"{"list": [{"list": [{"list": [{"bytes": "bb11"}, {"bytes": "cc11"}, {"bytes": "dd11"}]}]}]}"#,
		r#"{"list": [{"bytes": "aa112233445566aa112233445566aa112233445566aa112233445566"},
			{"constructor": 0, "fields": [
				{"constructor": 0, "fields": [
					{"bytes": "cfbee74ab533f40979101057f96de62e95233f2a5216eb16b54106f09fd7350d"},
					{"bytes": "28d1c3b7df297a60d24a3f88bc53d7029a8af35e8dd876764fd9e7a24203a348"}]},
				{"constructor": 0, "fields": [
					{"constructor": 0, "fields": [{"bytes": "cdefe62b0a0016c2ccf8124d7dda71f6865283667850cc7b471f761d2bc1eb13"}]},
					{"int": 1}]},
				{"map": [{"k": {"bytes": "61757261"}, "v": {"int": -5}}, {"k": {"int": 7}, "v": {"list": []}}]}]},
			{"int": 0}]}"#,
		r#"{ "int": 18446744073709551616 }"#,
		r#"{ "int": -340282366920938463463374607431768211456 }"#,
	];

	#[test]
	fn decodable_fixture_datums_decode_as_on_the_db_sync_route() {
		for json in DECODABLE_FIXTURE_DATUMS {
			let expected = db_sync_route(json).expect("fixture decodes on the db-sync route");
			let cbor = expected.to_bytes();
			assert_eq!(plutus_data_like_db_sync(&cbor).as_ref(), Ok(&expected), "{json}");
			assert_eq!(plutus_data_if_valid(Some(&cbor)), Some(expected), "{json}");
		}
	}

	#[test]
	fn integer_wider_than_64_bits_decodes_as_on_the_db_sync_route() {
		// Tag 2 bignum 2^64, as a Cardano transaction carries it.
		let cbor = hex_literal::hex!("c249010000000000000000");
		let expected = db_sync_route(r#"{ "int": 18446744073709551616 }"#);
		assert!(expected.is_some());
		assert_eq!(plutus_data_if_valid(Some(&cbor)), expected);
		assert_eq!(
			plutus_data_if_valid(Some(&cbor)),
			Some(PlutusData::new_integer(&BigInt::from_str("18446744073709551616").unwrap()))
		);
	}

	#[test]
	fn datum_nested_past_serde_json_limit_is_absent_as_on_the_db_sync_route() {
		// The fixture of 64 nested lists: 128 JSON levels, rejected by serde_json.
		assert_eq!(db_sync_route(&nested_lists_json(64)), None);
		assert!(matches!(
			plutus_data_like_db_sync(&nested_lists_cbor(64)),
			Err(DatumError::JsonRejected(_))
		));
		assert_eq!(plutus_data_if_valid(Some(&nested_lists_cbor(64))), None);
	}

	#[test]
	fn datum_nested_just_below_serde_json_limit_decodes_as_on_the_db_sync_route() {
		let expected = db_sync_route(&nested_lists_json(63));
		assert!(expected.is_some());
		assert_eq!(plutus_data_if_valid(Some(&nested_lists_cbor(63))), expected);
	}

	#[test]
	fn bytes_that_are_not_plutus_data_are_absent_or_an_error() {
		assert!(matches!(plutus_data_like_db_sync(&[0xff]), Err(DatumError::NotPlutusData(_))));
		assert_eq!(plutus_data_if_valid(Some(&[0xff])), None);
		assert_eq!(plutus_data_if_valid(None), None);
	}

	#[test]
	fn decoded_datum_does_not_keep_original_encoding() {
		// Indefinite length list, whereas the JSON route produces a value without
		// original bytes: both must be indistinguishable to the data sources.
		let mut list = PlutusList::new();
		list.add(&PlutusData::new_integer(&BigInt::from(1)));
		let expected = PlutusData::new_list(&list);
		let decoded = plutus_data_like_db_sync(&hex_literal::hex!("9f01ff")).unwrap();
		assert_eq!(decoded, expected);
		assert_eq!(decoded.to_bytes(), expected.to_bytes());
	}

	fn metadatum_list(elements: &[TransactionMetadatum]) -> Vec<u8> {
		let mut list = MetadataList::new();
		for element in elements {
			list.add(element);
		}
		TransactionMetadatum::new_list(&list).to_bytes()
	}

	#[test]
	fn bridge_metadata_bytes_render_as_hex_string() {
		let cbor =
			metadatum_list(&[TransactionMetadatum::new_bytes(vec![0xab, 0x01, 0xff]).unwrap()]);
		assert_eq!(c2m_metadata_like_db_sync(&cbor), Some(serde_json::json!(["0xab01ff"])));
	}

	#[test]
	fn bridge_metadata_text_renders_as_string() {
		let cbor = metadatum_list(&[TransactionMetadatum::new_text("0xab01".into()).unwrap()]);
		assert_eq!(c2m_metadata_like_db_sync(&cbor), Some(serde_json::json!(["0xab01"])));
	}

	#[test]
	fn other_bridge_metadata_is_not_a_recipient() {
		let bytes = TransactionMetadatum::new_bytes(vec![1]).unwrap();
		let int = TransactionMetadatum::new_int(&cardano_serialization_lib::Int::new_i32(1));
		assert_eq!(c2m_metadata_like_db_sync(&metadatum_list(&[])), None);
		assert_eq!(c2m_metadata_like_db_sync(&metadatum_list(&[bytes.clone(), bytes])), None);
		assert_eq!(c2m_metadata_like_db_sync(&metadatum_list(std::slice::from_ref(&int))), None);
		assert_eq!(c2m_metadata_like_db_sync(&int.to_bytes()), None);
		assert_eq!(c2m_metadata_like_db_sync(&[0xff]), None);
	}
}

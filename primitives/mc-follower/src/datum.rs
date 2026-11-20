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

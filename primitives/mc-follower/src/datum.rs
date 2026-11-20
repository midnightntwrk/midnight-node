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

/// Decodes datum CBOR through the detailed schema JSON and serde_json, as db-sync datums are
/// decoded, so that the same datums decode and to the same values (e.g. serde_json's nesting
/// limit applies alike)
pub(crate) fn plutus_data_like_db_sync(cbor: &[u8]) -> Result<PlutusData, String> {
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

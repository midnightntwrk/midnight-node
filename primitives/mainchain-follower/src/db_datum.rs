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

use cardano_serialization_lib::{
	PlutusData, PlutusDatumSchema::DetailedSchema, encode_json_value_to_plutus_datum,
};
use sqlx::database::HasValueRef;
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgTypeInfo, types::Oid};
use sqlx::types::JsonValue;
use sqlx::{Decode, FromRow, Postgres, Row, postgres::PgRow};

// Oid for jsonb type
const JSONB_OID: u32 = 3801;

/// Wraps PlutusData to provide sqlx::Decode and sqlx::Type implementations
#[derive(Debug, Clone, PartialEq)]
pub struct DbDatum(pub PlutusData);

impl sqlx::Type<Postgres> for DbDatum {
	fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
		PgTypeInfo::with_oid(Oid(JSONB_OID))
	}
}

impl<'r> sqlx::Decode<'r, Postgres> for DbDatum
where
	JsonValue: Decode<'r, Postgres>,
{
	fn decode(value: <Postgres as HasValueRef<'r>>::ValueRef) -> Result<Self, BoxDynError> {
		let value: JsonValue = <JsonValue as Decode<Postgres>>::decode(value)?;
		let datum = encode_json_value_to_plutus_datum(value, DetailedSchema);
		Ok(DbDatum(datum?))
	}
}

impl<'r> FromRow<'r, PgRow> for DbDatum {
	fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
		let json_val: JsonValue = row.try_get("full_datum")?;
		let datum = encode_json_value_to_plutus_datum(json_val, DetailedSchema)
			.map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
		Ok(DbDatum(datum))
	}
}

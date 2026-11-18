use cardano_serialization_lib::{
	PlutusData, PlutusDatumSchema::DetailedSchema, encode_json_value_to_plutus_datum,
};
use sqlx::database::Database;
use sqlx::error::BoxDynError;
use sqlx::types::JsonValue;
use sqlx::{Decode, Postgres, ValueRef};

/// Wraps PlutusData to provide sqlx::Decode and sqlx::Type implementations
#[derive(Debug, Clone, PartialEq)]
pub struct DbDatum(pub PlutusData);

impl sqlx::Type<Postgres> for DbDatum {
	fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
		<JsonValue as sqlx::Type<Postgres>>::type_info()
	}

	fn compatible(ty: &<Postgres as sqlx::Database>::TypeInfo) -> bool {
		<JsonValue as sqlx::Type<Postgres>>::compatible(ty)
	}
}

impl<'r> sqlx::Decode<'r, Postgres> for DbDatum {
	fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
		let value: JsonValue = <JsonValue as Decode<Postgres>>::decode(value)?;
		let datum = encode_json_value_to_plutus_datum(value, DetailedSchema);
		Ok(DbDatum(datum?))
	}
}

/// Decodes a datum that is NULL or not valid Plutus data as `None`, for addresses anyone can pay
/// to, where one bad datum must not fail the whole query.
pub(crate) struct DbDatumIfValid(Option<PlutusData>);

impl sqlx::Type<Postgres> for DbDatumIfValid {
	fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
		<JsonValue as sqlx::Type<Postgres>>::type_info()
	}

	fn compatible(ty: &<Postgres as sqlx::Database>::TypeInfo) -> bool {
		<JsonValue as sqlx::Type<Postgres>>::compatible(ty)
	}
}

impl<'r> sqlx::Decode<'r, Postgres> for DbDatumIfValid {
	fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
		if value.is_null() {
			return Ok(Self(None));
		}
		// serde_json rejects JSON nested past 128 levels, which a datum of 64 nested lists reaches.
		let Ok(value) = <JsonValue as Decode<Postgres>>::decode(value) else {
			return Ok(Self(None));
		};
		Ok(Self(encode_json_value_to_plutus_datum(value, DetailedSchema).ok()))
	}
}

impl From<DbDatumIfValid> for Option<DbDatum> {
	fn from(datum: DbDatumIfValid) -> Self {
		datum.0.map(DbDatum)
	}
}

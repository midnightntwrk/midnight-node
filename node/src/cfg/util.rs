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

use super::error::CfgError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(crate) fn get_keys<T: Serialize>(struct_val: T) -> Result<Vec<String>, CfgError> {
	let value = serde_json::to_value(struct_val).map_err(CfgError::GetKeysError)?;
	Ok(value
		.as_object()
		.map(|m| m.keys().cloned().collect::<Vec<String>>())
		.unwrap_or_default())
}

/// `deserialize_with` helper that accepts any casing for unit-variant enums.
///
/// `config` 0.15 made enum-variant matching case-sensitive, but our presets
/// use lowercase values (e.g. `chainspec_chain_type = "live"`). Restores the
/// pre-0.15 behaviour by trying the input as-is, then lowercase, then
/// PascalCase before giving up.
pub(crate) fn case_insensitive_enum<'de, D, T>(d: D) -> Result<T, D::Error>
where
	D: serde::Deserializer<'de>,
	T: DeserializeOwned,
{
	let s = String::deserialize(d)?;
	deserialize_case_insensitive::<T, D::Error>(&s)
}

/// `Option`-aware variant of [`case_insensitive_enum`].
pub(crate) fn case_insensitive_enum_opt<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
	D: serde::Deserializer<'de>,
	T: DeserializeOwned,
{
	let raw: Option<String> = Option::deserialize(d)?;
	raw.map(|s| deserialize_case_insensitive::<T, D::Error>(&s)).transpose()
}

fn deserialize_case_insensitive<T, E>(s: &str) -> Result<T, E>
where
	T: DeserializeOwned,
	E: serde::de::Error,
{
	use serde::de::value::StrDeserializer;
	let try_de = |candidate: &str| -> Result<T, E> {
		T::deserialize::<StrDeserializer<'_, E>>(StrDeserializer::new(candidate))
	};

	if let Ok(v) = try_de(s) {
		return Ok(v);
	}
	let lower = s.to_lowercase();
	if lower != s
		&& let Ok(v) = try_de(&lower)
	{
		return Ok(v);
	}
	let mut chars = lower.chars();
	if let Some(first) = chars.next() {
		let pascal: String = first.to_ascii_uppercase().to_string() + chars.as_str();
		if pascal != s
			&& let Ok(v) = try_de(&pascal)
		{
			return Ok(v);
		}
	}
	// Fall back to the original input so the error message references the user's value.
	try_de(s)
}

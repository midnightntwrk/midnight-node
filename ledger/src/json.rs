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

use serde_json::{Map, Value, json};

fn is_only_u8(v: &[Value]) -> bool {
	let max_u8_asu64 = u64::from(u8::MAX);
	v.iter().all(
		|e| matches!(e, Value::Number(n) if n.is_u64() && n.as_u64().unwrap_or(u64::MAX) <= max_u8_asu64),
	)
}

fn is_verification_key(key: &str, arr: &[Value]) -> bool {
	key == "vk"
		&& matches!(arr, [Value::Array(arr1), Value::Array(arr2)] if
								arr1.iter().all(|v| v.is_number()) && arr2.iter().all(|v| v.is_number()))
}

//transforms every array of numbers in json into hex string
pub fn transform(v: Value) -> Value {
	match v {
		//do this only if you have an array of u8
		Value::Array(a) if !a.is_empty() && is_only_u8(&a) => {
			let bytes: Vec<_> = a
				.iter()
				.filter_map(|e| if let Value::Number(n) = e { n.as_u64() } else { None })
				.map(|n| {
					//as the number is <= u8::MAX only first byte encodes the number
					let [number, ..] = n.to_le_bytes();
					number
				})
				.collect();

			let json = hex::encode(bytes);
			Value::String(format!("0x{json}"))
		},
		//rules for transforming nested Array
		Value::Array(a) => {
			let transformed: Vec<_> = a.iter().map(|e| transform(e.clone())).collect();
			Value::Array(transformed)
		},
		//rules for transforming nested Object
		Value::Object(ref a) => {
			let transformed: Map<_, _> = a
				.iter()
				.map(|entry| match entry {
					//case for converting verification key properly
					(k, Value::Array(arr)) if is_verification_key(k, arr) => {
						if let [vv @ Value::Array(_), vvv @ Value::Array(_)] = arr.as_slice() {
							let hex = transform(vv.clone());
							let transformed = json!([hex, vvv]);
							(k.clone(), transformed)
						} else {
							//if for some reason it was recognized as verification key but does not
							// match the pattern should never happen as the pattern is the same
							(k.clone(), transform(v.clone()))
						}
					},
					//case for converting everything else
					(k, v) => (k.clone(), transform(v.clone())),
				})
				.collect();
			Value::Object(transformed)
		},
		a => a,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use serde_json::json;

	#[test]
	fn should_convert_u8_array_to_hex_string() {
		let json_arr = json!([0x24, 0x42, 0xa4, 0x2a]);
		let hex = transform(json_arr);
		assert_eq!(Value::String("0x2442a42a".to_string()), hex);
	}

	#[test]
	fn should_not_convert_empty_array() {
		let json_arr = json!([]);
		let transformed = transform(json_arr.clone());
		assert_eq!(json_arr, transformed);
	}

	#[test]
	fn should_not_convert_non_u8_array_to_hex_string() {
		let json_arr = json!([0x24, 0x4242, "42", 0x2a]);
		let transformed = transform(json_arr.clone());
		assert_eq!(json_arr, transformed);
	}

	#[test]
	fn should_convert_nested_u8_array_to_hex_string() {
		let json_arr = json!([0x24, 0x4242, "42", [0x24, 0x42]]);
		let expected_json_arr = json!([0x24, 0x4242, "42", "0x2442"]);
		let transformed = transform(json_arr.clone());
		assert_eq!(expected_json_arr, transformed);
	}

	#[test]
	fn should_convert_u8_array_in_object() {
		let json_obj = json!({
			"a": "42",
			"b": 0x24,
			"c": [0x24,0x42],
			"d": {
				"a": [0x25,0x52],
			}
		});
		let expected_json_obj = json!({
			"a": "42",
			"b": 0x24,
			"c": "0x2442",
			"d": {
				"a": "0x2552",
			}
		});
		let transformed = transform(json_obj.clone());
		assert_eq!(expected_json_obj, transformed);
	}

	#[test]
	fn should_not_convert_vk_array_in_object() {
		let json_obj = json!({
			"a": "42",
			"b": 0x24,
			"c": [0x24,0x42],
			"vk": [[0x25,0x25],[0x52,0x52]],
		});
		let expected_json_obj = json!({
			"a": "42",
			"b": 0x24,
			"c": "0x2442",
			"vk": ["0x2525",[0x52,0x52]],
		});
		let transformed = transform(json_obj.clone());
		assert_eq!(expected_json_obj, transformed);
	}
}

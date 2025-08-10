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

#![allow(dead_code)]

use documented::{Documented, DocumentedFields};
use documented_types::FieldInfo;

#[derive(Documented)]
/// My Struct
struct MyStruct {
	/// This is documented
	pub a: bool,
	/// This too!
	/// Yes!
	#[doc_tag(secret)]
	pub oh_my: bool,
	pub c: String,
}

#[test]
fn test_simple() {
	assert_eq!(
		MyStruct::field_docs(),
		&[
			FieldInfo {
				name: "a".to_string(),
				field_type: "bool".to_string(),
				doc: "This is documented".to_string(),
				tags: Vec::new()
			},
			FieldInfo {
				name: "oh_my".to_string(),
				field_type: "bool".to_string(),
				doc: "This too!\nYes!".to_string(),
				tags: vec!["secret".to_string()]
			},
			FieldInfo {
				name: "c".to_string(),
				field_type: "String".to_string(),
				doc: "".to_string(),
				tags: Vec::new()
			}
		]
	);
}

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

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DependencyValue {
	String(String),
	Object {
		_git: Option<String>,
		_package: Option<String>,
		version: Option<String>,
		tag: Option<String>,
		rev: Option<String>,
		_path: Option<String>,
		_features: Option<Vec<String>>,
		_default_features: Option<bool>,
		branch: Option<String>,
	},
}

#[derive(Debug, Deserialize)]
struct CargoToml {
	workspace: Workspace,
}

#[derive(Debug, Deserialize)]
struct Workspace {
	dependencies: HashMap<String, DependencyValue>,
}

pub fn find_crate_version(crate_name: &str) -> Option<Vec<u8>> {
	let cargo_toml_raw = include_str!("../../Cargo.toml");
	let cargo_toml: Result<CargoToml, _> = toml::from_str(cargo_toml_raw);

	if let Ok(data) = cargo_toml
		&& let Some(value) = data.workspace.dependencies.get(crate_name)
	{
		return match value {
			DependencyValue::String(version) => Some(version.to_owned().into_bytes()),
			DependencyValue::Object { version: Some(version), .. } => {
				Some(version.to_owned().into_bytes())
			},
			DependencyValue::Object { tag: Some(tag), .. } => Some(tag.to_owned().into_bytes()),
			DependencyValue::Object { rev: Some(rev), .. } => Some(rev.to_owned().into_bytes()),
			DependencyValue::Object { branch: Some(branch), .. } => {
				Some(branch.to_owned().into_bytes())
			},
			_ => None,
		};
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_find_crate_version() {
		let version = find_crate_version("mn-ledger");
		assert!(version.is_some());
	}

	#[test]
	fn should_return_none_for_missing_crate() {
		let version = find_crate_version("mn-ldgr");
		assert_eq!(version, None);
	}
}

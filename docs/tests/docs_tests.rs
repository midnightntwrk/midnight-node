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

#[derive(Deserialize)]
struct Manifest {
	package: Package,
}

#[derive(Deserialize)]
struct Package {
	version: String,
}

#[test]
fn check_doc_files_are_linked_in_readme() {
	let readme_str = std::fs::read_to_string("../README.md").unwrap();
	let paths = std::fs::read_dir("./").unwrap();

	for path in paths {
		let path = path.unwrap().path();
		if path.is_file()
			&& path.extension().map(|e| e.to_string_lossy().to_string()) == Some("md".to_string())
		{
			// Ensure it's linked in the README
			assert!(
				readme_str.contains(path.file_name().unwrap().to_string_lossy().as_ref()),
				"missing link to {} in readme!",
				path.to_string_lossy()
			);
		}
	}
}

#[test]
fn check_spec_version_matches_node_version() {
	let node_manifest_str = std::fs::read_to_string("../node/Cargo.toml").unwrap();
	let node_manifest: Manifest =
		toml::from_str(&node_manifest_str).expect("Failed to parse node Cargo.toml");

	let mut found = false;
	let runtime_lib_str = std::fs::read_to_string("../runtime/src/lib.rs").unwrap();
	for line in runtime_lib_str.lines() {
		if line.trim_start().starts_with("spec_version") {
			let v_end = line.chars().take_while(|c| *c != ',').count();
			let v_rev: String =
				line[..v_end].chars().rev().take_while(|c| *c != ' ').collect::<String>();
			let v: String = v_rev.chars().rev().collect();
			let v: Vec<u32> = v.split('_').map(|s| s.parse().unwrap()).collect();
			let v = format!("{}.{}.{}", v[0], v[1], v[2]);

			assert_eq!(
				node_manifest.package.version, v,
				"Spec version does not match node version"
			);
			found = true;
			break;
		}
	}

	assert!(found, "Spec version not found in runtime/src/lib.rs");
}

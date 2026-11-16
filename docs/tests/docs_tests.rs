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

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
	package: Package,
}

#[derive(Deserialize)]
struct Package {
	version: String,
}

fn get_runtime_spec_version() -> String {
	let runtime_lib_str = std::fs::read_to_string("../runtime/src/lib.rs").unwrap();
	for line in runtime_lib_str.lines() {
		if line.trim_start().starts_with("spec_version") {
			let v_end = line.chars().take_while(|c| *c != ',').count();
			let v_rev: String =
				line[..v_end].chars().rev().take_while(|c| *c != ' ').collect::<String>();
			let v: String = v_rev.chars().rev().collect();
			return v;
		}
	}
	panic!("runtime spec version not found (runtime/src/lib.rs)");
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
fn check_metadata_package_version_matches_runtime_version() {
	// The metadata crate embeds the runtime's metadata, so it is keyed by the
	// runtime, not the node: a node-only patch release leaves both untouched.
	let runtime_manifest_str = std::fs::read_to_string("../runtime/Cargo.toml").unwrap();
	let runtime_manifest: Manifest =
		toml::from_str(&runtime_manifest_str).expect("Failed to parse runtime Cargo.toml");

	let metadata_manifest_str = std::fs::read_to_string("../metadata/Cargo.toml").unwrap();
	let metadata_manifest: Manifest =
		toml::from_str(&metadata_manifest_str).expect("Failed to parse metadata Cargo.toml");

	assert_eq!(
		metadata_manifest.package.version, runtime_manifest.package.version,
		"metadata/Cargo.toml version must match runtime/Cargo.toml version"
	);
}

#[test]
fn check_node_version_is_at_or_after_spec_version() {
	let node_manifest_str = std::fs::read_to_string("../node/Cargo.toml").unwrap();
	let node_manifest: Manifest =
		toml::from_str(&node_manifest_str).expect("Failed to parse node Cargo.toml");

	let spec: Vec<u32> =
		get_runtime_spec_version().split('_').map(|s| s.parse().unwrap()).collect();

	// Strip pre-release suffix (e.g., "-rc.1") from node version for comparison,
	// since spec_version can only encode major.minor.patch
	let node: Vec<u32> = node_manifest
		.package
		.version
		.split('-')
		.next()
		.expect("Node version should have at least the base version")
		.split('.')
		.map(|s| s.parse().unwrap())
		.collect();

	// A client-only release ships a newer node over an unchanged runtime, so the
	// node may run ahead of spec_version on the patch digit - never behind it,
	// and never on a different major.minor.
	assert_eq!(
		(node[0], node[1]),
		(spec[0], spec[1]),
		"node major.minor must match runtime spec_version major.minor"
	);
	assert!(
		node[2] >= spec[2],
		"node patch version {} is behind runtime spec_version patch {}",
		node[2],
		spec[2]
	);
}

#[test]
fn check_runtime_package_version_matches_spec_version() {
	let runtime_manifest_str = std::fs::read_to_string("../runtime/Cargo.toml").unwrap();
	let runtime_manifest: Manifest =
		toml::from_str(&runtime_manifest_str).expect("Failed to parse runtime Cargo.toml");

	let runtime_spec_version = get_runtime_spec_version();
	let v: Vec<u32> = runtime_spec_version.split('_').map(|s| s.parse().unwrap()).collect();
	let spec_version = format!("{}.{}.{}", v[0], v[1], v[2]);

	assert_eq!(
		runtime_manifest.package.version, spec_version,
		"runtime/Cargo.toml version must match runtime/src/lib.rs spec_version"
	);
}

#[test]
fn check_toolkit_supports_new_node_version() {
	let toolkit_runtimes_src =
		std::fs::read_to_string("../util/toolkit/src/fetcher/runtimes.rs").unwrap();

	assert!(
		toolkit_runtimes_src.contains(&get_runtime_spec_version()),
		"Failed to find spec_version in toolkit runtimes.rs",
	);
}

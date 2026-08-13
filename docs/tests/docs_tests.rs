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

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
	package: Package,
}

#[derive(Deserialize)]
struct Package {
	version: String,
}

/// Workspace root, resolved independently of the test's working directory:
/// cargo runs from the crate dir (`docs/`), buck2 from the workspace root. Walk
/// up until `res/cfg/default.toml` is found: the anchor both layouts share.
fn root() -> PathBuf {
	// buck2 runs tests from the project root in a hermetic sandbox lacking the repo
	// tree; MN_WORKSPACE_ROOT points at a staged fixtures root (see the buck2 test
	// target's resources). Unset under cargo, so the upward walk is the default.
	if let Some(root) = std::env::var_os("MN_WORKSPACE_ROOT") {
		return PathBuf::from(root);
	}
	let mut dir = std::env::current_dir().expect("cwd");
	loop {
		if dir.join("res/cfg/default.toml").exists() {
			return dir;
		}
		if !dir.pop() {
			panic!("could not locate workspace root (res/cfg/default.toml not found above cwd)");
		}
	}
}

fn get_runtime_spec_version() -> String {
	let runtime_lib_str = std::fs::read_to_string(root().join("runtime/src/lib.rs")).unwrap();
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
	let readme_str = std::fs::read_to_string(root().join("README.md")).unwrap();
	let paths = std::fs::read_dir(root().join("docs")).unwrap();

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
fn check_metadata_package_version_matches_node_version() {
	let node_manifest_str = std::fs::read_to_string(root().join("node/Cargo.toml")).unwrap();
	let node_manifest: Manifest =
		toml::from_str(&node_manifest_str).expect("Failed to parse node Cargo.toml");

	let metadata_manifest_str =
		std::fs::read_to_string(root().join("metadata/Cargo.toml")).unwrap();
	let metadata_manifest: Manifest =
		toml::from_str(&metadata_manifest_str).expect("Failed to parse metadata Cargo.toml");

	assert_eq!(node_manifest.package.version, metadata_manifest.package.version);
}

#[test]
fn check_spec_version_matches_node_version() {
	let node_manifest_str = std::fs::read_to_string(root().join("node/Cargo.toml")).unwrap();
	let node_manifest: Manifest =
		toml::from_str(&node_manifest_str).expect("Failed to parse node Cargo.toml");

	let runtime_spec_version = get_runtime_spec_version();

	// Parse each part, separate with '.'
	let v: Vec<u32> = runtime_spec_version.split('_').map(|s| s.parse().unwrap()).collect();
	let spec_version = format!("{}.{}.{}", v[0], v[1], v[2]);

	// Strip pre-release suffix (e.g., "-rc.1") from node version for comparison,
	// since spec_version can only encode major.minor.patch
	let node_version = node_manifest
		.package
		.version
		.split('-')
		.next()
		.expect("Node version should have at least the base version");

	assert_eq!(
		node_version, spec_version,
		"Spec version does not match node version (ignoring pre-release suffix)"
	);
}

#[test]
fn check_toolkit_supports_new_node_version() {
	let toolkit_runtimes_src =
		std::fs::read_to_string(root().join("util/toolkit/src/fetcher/runtimes.rs")).unwrap();

	assert!(
		toolkit_runtimes_src.contains(&get_runtime_spec_version()),
		"Failed to find spec_version in toolkit runtimes.rs",
	);
}

// This file is part of midnight-indexer.
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

use std::{env, fs, path::Path};

const NODE_VERSION_FILE: &str = "../NODE_VERSION";

fn main() {
    let node_version = read_node_version();

    let metadata_path = Path::new("..")
        .join(".node")
        .join(&node_version)
        .join("metadata.scale");
    if !metadata_path.exists() {
        panic!("metadata file not found at {}", metadata_path.display());
    }

    // Extract version for module name (replace dots and hyphens with underscores).
    // E.g. "0.16.0-da0b6c69" becomes "0_16".
    let module_suffix = node_version
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join("_");

    // Generate the code with the subxt macro call.
    let generated_code = format!(
        r#"
            #[subxt::subxt(
                runtime_metadata_path = "{}",
                derive_for_type(
                    path = "sp_consensus_slots::Slot",
                    derive = "parity_scale_codec::Encode, parity_scale_codec::Decode",
                    recursive
                )
            )]
            pub mod runtime_{module_suffix} {{}}
        "#,
        metadata_path.display()
    );

    // Write generated code to file in OUT_DIR.
    let out_dir = env::var("OUT_DIR").expect("env var OUT_DIR is set");
    let runtime_file = Path::new(&out_dir).join("generated_runtime.rs");
    fs::write(&runtime_file, generated_code).expect("generated runtime file can be written");

    // Tell cargo to rerun build script if:
    // 1. The NODE_VERSION file changes.
    println!("cargo:rerun-if-changed={}", NODE_VERSION_FILE);
    // 2. The metadata file itself changes.
    println!("cargo:rerun-if-changed={}", metadata_path.display());
    // 3. The .node directory structure changes.
    println!("cargo:rerun-if-changed=../.node");

    // Output information for debugging.
    println!("cargo:rustc-env=USED_NODE_VERSION={}", node_version);
}

fn read_node_version() -> String {
    if !Path::new(NODE_VERSION_FILE).exists() {
        panic!("{NODE_VERSION_FILE} file not found");
    }

    // Read and validate/sanitize the version string.
    match fs::read_to_string(NODE_VERSION_FILE) {
        Ok(version) => {
            let version = version.trim().to_string();

            if version.is_empty() {
                panic!("{NODE_VERSION_FILE} file is empty");
            }

            validate_and_sanitize_version(&version)
        }

        Err(error) => {
            panic!("cannot read {NODE_VERSION_FILE} file: {error}");
        }
    }
}

fn validate_and_sanitize_version(version: &str) -> String {
    const MAX_VERSION_LENGTH: usize = 64;
    if version.len() > MAX_VERSION_LENGTH {
        panic!(
            "node version must have less than {MAX_VERSION_LENGTH} characters, but had {}",
            version.len()
        );
    }

    const PERMITTED_SPECIAL_CHARS: [char; 3] = ['.', '-', '_'];
    let allowed_chars =
        |c: char| -> bool { c.is_ascii_alphanumeric() || PERMITTED_SPECIAL_CHARS.contains(&c) };
    if !version.chars().all(allowed_chars) {
        panic!(
            "invalid characters in node version {}",
            version
                .chars()
                .filter(|c| !allowed_chars(*c))
                .collect::<String>()
        );
    }

    if version.starts_with(PERMITTED_SPECIAL_CHARS) || version.ends_with(PERMITTED_SPECIAL_CHARS) {
        panic!(
            "node version must not start or end with {PERMITTED_SPECIAL_CHARS:?}, but got: '{}'",
            version
        );
    }

    version.to_string()
}

// This file is part of midnight-indexer.
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

//! Best-effort population of build-time metadata consumed by
//! `indexer-common::version`. Both pieces are optional; reproducible Nix
//! builds without git or `date` simply omit them.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=MIDNIGHT_INDEXER_GIT_SHA");
    println!("cargo:rerun-if-env-changed=MIDNIGHT_INDEXER_BUILD_DATE");
    println!("cargo:rerun-if-changed=build.rs");

    if let Some(sha) = pick("MIDNIGHT_INDEXER_GIT_SHA", git_sha) {
        println!("cargo:rustc-env=MIDNIGHT_INDEXER_GIT_SHA={sha}");
    }
    if let Some(date) = pick("MIDNIGHT_INDEXER_BUILD_DATE", build_date) {
        println!("cargo:rustc-env=MIDNIGHT_INDEXER_BUILD_DATE={date}");
    }
}

/// External env var wins; otherwise fall back to `compute`. Empty/whitespace
/// values are treated as missing so callers can clear via `VAR=`.
fn pick(env_var: &str, compute: fn() -> Option<String>) -> Option<String> {
    std::env::var(env_var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(compute)
}

fn git_sha() -> Option<String> {
    run("git", &["rev-parse", "--short=8", "HEAD"])
}

fn build_date() -> Option<String> {
    run("date", &["-u", "+%Y-%m-%d"])
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

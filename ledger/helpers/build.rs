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

//! Bakes the *resolved* ledger crate versions into `LEDGER_{7,8,9}_VERSION` env vars from
//! `Cargo.lock`, so the toolkit `version` command reports what was actually built rather than the
//! `Cargo.toml` version spec (LeastAuthority audit, "Prefer Cargo.lock For Build-Time Crate
//! Versions").
//!
//! Why `cargo metadata` and not a hand-rolled lock parse: the resolve graph keys each dependency by
//! its *workspace alias* (`mn_ledger`, `mn_ledger_8`, `mn_ledger_9`), so two aliases that rename the
//! same crate (`midnight-ledger` 7.0.3 and 8.1.0) are distinct nodes — no version-spec
//! disambiguation guesswork. `--locked` keeps the resolve deterministic (no lockfile mutation);
//! `--offline` is deliberately *not* used, since the registry/git cache may not be warm when the
//! build script runs.

use cargo_metadata::MetadataCommand;

/// Workspace dependency alias (as cargo normalises it: hyphens → underscores) → env var name.
const LEDGER_ALIASES: [(&str, &str); 3] = [
	("mn_ledger", "LEDGER_7_VERSION"),
	("mn_ledger_8", "LEDGER_8_VERSION"),
	("mn_ledger_9", "LEDGER_9_VERSION"),
];

fn main() {
	let meta = MetadataCommand::new()
		.other_options(["--locked".to_owned()])
		.exec()
		.expect("cargo metadata failed - is Cargo.lock committed and current?");

	// Rebuild when the resolved versions (Cargo.lock) or the aliases (workspace Cargo.toml) change.
	println!("cargo:rerun-if-changed={}/Cargo.lock", meta.workspace_root);
	println!("cargo:rerun-if-changed={}/Cargo.toml", meta.workspace_root);

	let resolve = meta.resolve.as_ref().expect("cargo metadata returned no resolve graph");
	let helpers = resolve
		.nodes
		.iter()
		.find(|n| meta[&n.id].name == "midnight-node-ledger-helpers")
		.expect("helpers package absent from resolve graph");

	for (alias, env_var) in LEDGER_ALIASES {
		let dep =
			helpers.deps.iter().find(|d| d.name == alias).unwrap_or_else(|| {
				panic!("dependency alias `{alias}` not found - was it renamed?")
			});
		let pkg = &meta[&dep.pkg];
		let source = pkg.source.as_ref().map(|s| s.repr.as_str());
		println!("cargo:rustc-env={env_var}={}", format_version(&pkg.version.to_string(), source));
	}
}

/// Human-readable resolved version. Registry (or path) deps: bare semver (`7.0.3`). Git deps: semver
/// plus the locked tag/branch label *and* the full commit SHA (`1.0.0 (tag: crate-ledger-9.1.0.0-rc.3,
/// rev: 85e7...)`) - the tag is context, the SHA is the immutable build identity.
///
/// `source` is cargo's repr, e.g. `git+https://host/repo?tag=NAME#FULL_SHA`. cargo_metadata exposes
/// only this string, so the git ref is parsed here.
fn format_version(version: &str, source: Option<&str>) -> String {
	let Some(git) = source.and_then(|s| s.strip_prefix("git+")) else {
		return version.to_owned(); // registry / path
	};
	let (locator, commit) = git.split_once('#').unwrap_or((git, "unknown"));
	let label = locator
		.split_once('?')
		.and_then(|(_, query)| {
			query.split('&').find(|p| p.starts_with("tag=") || p.starts_with("branch="))
		})
		.map(|p| format!("{}, ", p.replacen('=', ": ", 1)))
		.unwrap_or_default();
	format!("{version} ({label}rev: {commit})")
}

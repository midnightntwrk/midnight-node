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

// Resolved ledger versions, baked in at compile time by `build.rs` from `Cargo.lock`
// (`cargo metadata --locked`). This replaces the runtime `Cargo.toml` parse the
// LeastAuthority audit flagged ("Prefer Cargo.lock For Build-Time Crate Versions"): the constants
// reflect what was actually resolved and built, and git deps carry their locked tag + commit SHA.
const LEDGER_8_VERSION: &str = env!("LEDGER_8_VERSION");
const LEDGER_9_VERSION: &str = env!("LEDGER_9_VERSION");

/// Resolved version of a workspace ledger dependency alias, embedded at compile time from
/// `Cargo.lock`.
///
/// Registry deps return the bare semver (`"7.0.3"`); git deps include the locked ref, e.g.
/// `"1.0.0 (tag: ledger-9.1.0.0-rc.4, rev: cd652d7f97b34b805bb5f0310ce7434eb883af38)"`.
/// Returns `None` for an unknown alias.
pub fn find_dependency_version(alias: &str) -> Option<String> {
	match alias {
		"mn-ledger-8" => Some(LEDGER_8_VERSION.to_owned()),
		"mn-ledger-9" => Some(LEDGER_9_VERSION.to_owned()),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn resolves_registry_dependency_to_bare_semver() {
		// Ledger crates come from crates.io, so the locked source is the registry and the
		// version carries no git annotation. (`build.rs::format_version` still annotates git
		// deps with their tag and commit SHA; no workspace alias is git-pinned today.)
		for alias in ["mn-ledger-8", "mn-ledger-9"] {
			let v =
				find_dependency_version(alias).unwrap_or_else(|| panic!("{alias} should resolve"));
			assert!(
				!v.contains("tag:") && !v.contains("rev:"),
				"{alias}: unexpected git annotation in {v:?}"
			);
			let parts: Vec<&str> = v.split('.').collect();
			assert!(
				parts.len() == 3 && parts.iter().all(|p| p.parse::<u64>().is_ok()),
				"{alias}: expected a bare semver, got {v:?}"
			);
		}
	}

	#[test]
	fn returns_none_for_missing_crate() {
		assert_eq!(find_dependency_version("mn-ldgr"), None);
	}
}

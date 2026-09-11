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
	fn annotates_git_dependency_with_tag_and_rev() {
		// A git dep's tag is mutable; the locked commit SHA is the immutable build identity - both.
		let v = find_dependency_version("mn-ledger-9").expect("mn-ledger-9 should resolve");
		assert!(v.contains("tag:"), "expected tag annotation, got {v:?}");
		let rev = v
			.split("rev:")
			.nth(1)
			.expect("expected rev annotation")
			.trim()
			.trim_end_matches(')');
		assert!(rev.len() >= 16, "rev hash too short ({} chars): {rev:?}", rev.len());
	}

	#[test]
	fn returns_none_for_missing_crate() {
		assert_eq!(find_dependency_version("mn-ldgr"), None);
	}
}

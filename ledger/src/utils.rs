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

/// Resolved version (UTF-8 bytes) of a ledger generation's backing crate, for the
/// `get_ledger_version` runtime API / RPC.
///
/// Delegates to [`midnight_node_ledger_helpers::find_dependency_version`], which reads the version
/// resolved in `Cargo.lock` rather than the requested spec in `Cargo.toml` (LeastAuthority audit,
/// "Prefer Cargo.lock For Build-Time Crate Versions"). Registry deps yield the bare semver; git
/// deps include the locked tag and commit SHA.
pub fn find_crate_version(crate_name: &str) -> Option<Vec<u8>> {
	midnight_node_ledger_helpers::find_dependency_version(crate_name).map(String::into_bytes)
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

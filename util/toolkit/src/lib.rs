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

pub mod cli;
pub mod cli_parsers;
pub mod client;
pub mod commands;
pub mod fetcher;
pub mod genesis_generator;
pub mod progress;
pub mod remote_prover;
pub mod sender;
pub mod serde_def;
pub mod toolkit_js;
pub mod tx_generator;
pub mod utils;

/// Test-only path helpers. Fixtures live under the workspace's `res/`, which
/// tests historically reached via `../../res/...` assuming cargo's CWD = crate
/// dir. Buck2 runs tests from the workspace root, so anchor to the root instead:
/// walk up until `res/cfg/default.toml` is found (works under both).
#[cfg(test)]
pub(crate) mod test_paths {
	use std::path::PathBuf;

	pub fn workspace_root() -> PathBuf {
		// buck2 runs tests from the project root in a hermetic sandbox lacking the
		// repo tree; MN_WORKSPACE_ROOT points at a staged fixtures root (see the test
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
				panic!(
					"could not locate workspace root (res/cfg/default.toml not found above cwd)"
				);
			}
		}
	}

	/// CARGO_MANIFEST_DIR resolved at RUNTIME (cargo and buck2 both set it; buck2 runs
	/// tests from the project root so the value is relative). Compile-time env!() bakes
	/// buck2's ephemeral build-sandbox path, which is gone when the test runs on another
	/// RE worker — so test-data reads must go through this, not env!().
	pub fn manifest_dir() -> String {
		std::env::var("CARGO_MANIFEST_DIR")
			.unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string())
	}

	/// Absolute path to a `res/`-relative fixture, e.g. `res("dev/ics-config.json")`.
	pub fn res(rel: &str) -> String {
		workspace_root().join("res").join(rel).to_string_lossy().into_owned()
	}
}

use progress::{Progress, Spin};
use rand::{SeedableRng, rngs::StdRng};
use subxt::utils::H256;
use tx_generator::*;

use midnight_node_ledger_helpers::*;

// Conditionally define the type alias `ProofType` and `SignatureType`
#[cfg(not(feature = "erase-proof"))]
pub type ProofType = ProofMarker;

#[cfg(not(feature = "erase-proof"))]
pub type SignatureType = Signature;

#[cfg(feature = "erase-proof")]
pub type ProofType = ();

#[cfg(feature = "erase-proof")]
pub type SignagtureType = ();

pub fn hash_to_str(h: H256) -> String {
	format!("0x{}", hex::encode(h.as_bytes()))
}

// A default token used for zswap tests
pub fn t_token() -> ShieldedTokenType {
	Default::default()
}

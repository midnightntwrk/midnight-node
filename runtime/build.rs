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

fn main() {
	// `fork_transition` reads these through `option_env!`, which cargo does not
	// track on its own: without this, changing forks and rebuilding can silently
	// reuse a wasm built for the previous fork.
	#[cfg(feature = "fork-transition")]
	{
		println!("cargo::rerun-if-env-changed=MIDNIGHT_FORK_HEIGHT");
		println!("cargo::rerun-if-env-changed=MIDNIGHT_FORK_AURA_AUTHORITIES");
		check_fork_transition_config();
	}

	#[cfg(feature = "std")]
	{
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}
}

/// Refuse to build `fork-transition` without a complete, well-formed fork
/// configuration.
///
/// The feature is meant to be enabled deliberately, for one fork, with both
/// values copied from `fork-bundle.json`. Enabled any other way -- most likely
/// by cargo feature unification, where a single crate in the build asking for
/// it turns it on for every crate -- it should fail here, loudly, rather than
/// quietly produce a runtime that is merely inert today.
#[cfg(feature = "fork-transition")]
fn check_fork_transition_config() {
	const HINT: &str = "The `fork-transition` feature must be enabled on purpose, for a single \
		fork, with both values taken from `mock-authorities`' fork-bundle.json. If you did not \
		mean to enable it, a crate in this build is turning it on through feature unification: \
		find it with `cargo tree -e features -i midnight-node-runtime`. A runtime built with \
		this feature must never be released.";

	let height = std::env::var("MIDNIGHT_FORK_HEIGHT").unwrap_or_default();
	if height.trim().parse::<u32>().is_err() {
		panic!("MIDNIGHT_FORK_HEIGHT must be a decimal block number, got {height:?}.\n{HINT}");
	}

	let authorities = std::env::var("MIDNIGHT_FORK_AURA_AUTHORITIES").unwrap_or_default();
	let keys: Vec<&str> =
		authorities.split(',').map(str::trim).filter(|key| !key.is_empty()).collect();
	let is_hex32 = |key: &str| {
		let key = key.strip_prefix("0x").unwrap_or(key);
		key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
	};
	if keys.is_empty() || !keys.iter().all(|key| is_hex32(key)) {
		panic!(
			"MIDNIGHT_FORK_AURA_AUTHORITIES must be a comma-separated list of 32-byte hex keys, \
			 got {authorities:?}.\n{HINT}"
		);
	}
}

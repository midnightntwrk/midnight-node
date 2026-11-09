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

//! Landlock filesystem sandbox for the running node, applied in two stages.

use landlock::{
	ABI, Access, AccessFs, CompatLevel, Compatible, RestrictSelfAttr, Ruleset, RulesetAttr,
	RulesetCreated, RulesetCreatedAttr, RulesetError, RulesetStatus, path_beneath_rules,
};
use std::path::PathBuf;

// Pinned: V9 adds `ResolveUnix`, which would block a unix-socket db-sync connection.
const ABI: ABI = ABI::V5;

/// What the node still opens after start-up: DNS and TLS config, NSS modules, `/proc` and
/// cgroup stats.
const RUNTIME_READABLE: &[&str] = &[
	"/etc",
	// Often symlinks out of /etc; a rule follows the link to its target.
	"/etc/hosts",
	"/etc/resolv.conf",
	"/usr",
	"/lib",
	"/lib64",
	"/proc",
	"/sys",
	"/dev",
];

fn rules(
	ruleset: Ruleset,
	readable: impl IntoIterator<Item = PathBuf>,
	writable: &[PathBuf],
) -> Result<RulesetCreated, RulesetError> {
	ruleset
		.handle_access(AccessFs::from_all(ABI))?
		.create()?
		.add_rules(path_beneath_rules(readable, AccessFs::ReadFile | AccessFs::ReadDir))?
		.add_rules(path_beneath_rules(writable, AccessFs::from_all(ABI) & !AccessFs::Execute))
}

/// Stage 1: reads anywhere, writes only beneath `writable`, no `execve`.
///
/// Landlock binds the calling thread and the threads it spawns afterwards, so this must run
/// before the tokio runtime starts. Paths that don't exist are skipped.
pub fn restrict(writable: &[PathBuf]) -> Result<RulesetStatus, RulesetError> {
	Ok(rules(Ruleset::default(), [PathBuf::from("/")], writable)?
		.restrict_self()?
		.ruleset)
}

/// Stage 2, once start-up has read its config: narrows reads to [`RUNTIME_READABLE`],
/// `readable` and `writable`, on every thread at once.
///
/// Errors on kernels before Landlock ABI v8, which can't restrict sibling threads.
pub fn restrict_reads(
	readable: Vec<PathBuf>,
	writable: &[PathBuf],
) -> Result<RulesetStatus, RulesetError> {
	let readable = RUNTIME_READABLE
		.iter()
		.map(PathBuf::from)
		.chain(
			["SSL_CERT_FILE", "SSL_CERT_DIR"]
				.into_iter()
				.filter_map(std::env::var_os)
				.map(PathBuf::from),
		)
		// For panic backtrace symbolization.
		.chain(std::env::current_exe().ok())
		.chain(readable);
	let ruleset = Ruleset::default().set_compatibility(CompatLevel::HardRequirement);
	Ok(rules(ruleset, readable, writable)?.all_threads(true)?.restrict_self()?.ruleset)
}

#[cfg(test)]
mod tests {
	use super::*;

	// Stage 1 only binds the spawned thread, so the rest of the test binary is unaffected.
	// Stage 2 sandboxes the whole process and is tested in `tests/landlock_reads.rs`.
	#[test]
	fn stage1_confines_writes_and_exec() {
		let allowed = tempfile::tempdir().unwrap();
		let denied = tempfile::tempdir().unwrap();
		let writable = vec![allowed.path().to_path_buf()];
		let denied_path = denied.path().to_path_buf();
		std::fs::write(denied_path.join("secret"), b"x").unwrap();

		std::thread::spawn(move || {
			if restrict(&writable).unwrap() == RulesetStatus::NotEnforced {
				return;
			}
			std::fs::write(writable[0].join("ok"), b"x").unwrap();
			assert!(std::fs::write(denied_path.join("no"), b"x").is_err());
			assert!(std::fs::read(denied_path.join("secret")).is_ok());
			assert!(std::process::Command::new("/bin/true").status().is_err());
		})
		.join()
		.unwrap();
	}
}

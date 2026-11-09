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

//! Its own test binary: stage 2 sandboxes every thread of the process.

#![cfg(target_os = "linux")]

use midnight_node::landlock::restrict_reads;

#[test]
fn stage2_restricts_reads_on_threads_it_did_not_spawn() {
	let allowed = tempfile::tempdir().unwrap();
	let denied = tempfile::tempdir().unwrap();
	std::fs::write(allowed.path().join("ok"), b"x").unwrap();
	let secret = denied.path().join("secret");
	std::fs::write(&secret, b"x").unwrap();
	let writable = vec![allowed.path().to_path_buf()];

	let applied = std::thread::spawn(move || restrict_reads(vec![], &writable)).join().unwrap();
	if let Err(e) = applied {
		eprintln!("skipping, kernel lacks Landlock ABI v8: {e}");
		return;
	}
	assert!(std::fs::read(&secret).is_err());
	assert!(std::fs::read(allowed.path().join("ok")).is_ok());
	assert!(std::fs::read_to_string("/proc/self/status").is_ok());
}

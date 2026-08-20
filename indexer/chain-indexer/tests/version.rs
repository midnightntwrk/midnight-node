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

#![cfg(feature = "cloud")]

use std::process::Command;

/// `--version` must work without a config file and report the workspace version.
#[test]
fn prints_version_without_config() {
    let bin = env!("CARGO_BIN_EXE_chain-indexer");
    for flag in ["--version", "-v"] {
        let output = Command::new(bin)
            .arg(flag)
            .env_remove("CONFIG_FILE")
            .output()
            .expect("spawn chain-indexer");

        assert!(
            output.status.success(),
            "exit status was {:?} for {flag}: stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
        let first_line = stdout.lines().next().unwrap_or_default();
        assert!(
            first_line.starts_with(&format!("chain-indexer {}", env!("CARGO_PKG_VERSION"))),
            "unexpected output for {flag}: {first_line:?}"
        );
    }
}

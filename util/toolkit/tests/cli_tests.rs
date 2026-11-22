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

#[test]
fn cli_tests() {
	// Absolutize the toolkit bin path before the chdir below: buck2 sets
	// CARGO_BIN_EXE_* relative to the workspace root, cargo sets it absolute.
	let bin = std::env::var_os("CARGO_BIN_EXE_midnight-node-toolkit")
		.map(|p| std::env::current_dir().unwrap().join(p));

	// tests/cmd/, README.md and the tomls' `../../res/...` resolve from the crate
	// dir. cargo runs tests there; buck2 runs them from the workspace root, so
	// chdir. Read at run time: `env!` would bake in buck2's build-sandbox path.
	// Safe: cli_tests is the only test in this binary.
	let cwd = std::env::var("CARGO_MANIFEST_DIR")
		.unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
	std::env::set_current_dir(&cwd).unwrap();
	// Create directory to put test outputs in
	std::fs::create_dir_all("out").unwrap();

	let cases = trycmd::TestCases::new();
	if let Some(bin) = bin {
		cases.register_bin("midnight-node-toolkit", bin);
	}
	cases.case("tests/cmd/*.toml").case("README.md");
}

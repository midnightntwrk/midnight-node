// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
	println!("cargo::rustc-check-cfg=cfg(hardfork_test)");
	println!("cargo:re-run-if-env-changed=HARDFORK_TEST");
	if std::env::var("HARDFORK_TEST").is_ok() {
		println!("cargo:rustc-cfg=hardfork_test");
		unsafe {
			std::env::set_var("FORCE_WASM_BUILD", "true");
		}
	}

	println!("cargo::rustc-check-cfg=cfg(hardfork_test_rollback)");
	println!("cargo:re-run-if-env-changed=HARDFORK_TEST_ROLLBACK");
	if std::env::var("HARDFORK_TEST_ROLLBACK").is_ok() {
		println!("cargo:rustc-cfg=hardfork_test_rollback");
		unsafe {
			std::env::set_var("FORCE_WASM_BUILD", "true");
		}
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

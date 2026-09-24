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

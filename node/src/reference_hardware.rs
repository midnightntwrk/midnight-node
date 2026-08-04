// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use sc_sysinfo::Requirements;
use std::sync::LazyLock;

// Regenerate via scripts/benchmark/generate-reference-hardware.sh.
pub static MIDNIGHT_REFERENCE_HARDWARE: LazyLock<Requirements> = LazyLock::new(|| {
	let raw = include_bytes!("midnight_reference_hardware.json").as_ref();
	serde_json::from_reader(raw).expect("Bundled reference hardware JSON is valid; qed")
});

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reference_hardware_json_parses() {
		let reqs = &*MIDNIGHT_REFERENCE_HARDWARE;
		assert!(!reqs.0.is_empty(), "reference hardware requirements must not be empty");
	}
}

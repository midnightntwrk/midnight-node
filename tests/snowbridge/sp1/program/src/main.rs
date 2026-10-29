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

#![no_main]
sp1_zkvm::entrypoint!(main);
use midnight_beefy::{decode_all, verify_batch, BatchInput};
use parity_scale_codec::Encode;
pub fn main() {
	let bytes = sp1_zkvm::io::read_vec();
	assert!(bytes.len() <= 4 * 1024 * 1024, "oversized input");
	let input: BatchInput = decode_all(&bytes).expect("invalid input");
	let output = verify_batch(&input).expect("invalid BEEFY batch");
	sp1_zkvm::io::commit_slice(&output.encode());
}

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

use sp_runtime_interface::runtime_interface;

// TODO: this custom host function exists only because the stock transaction pool's
// `validate_transaction` path registers no offchain extension, so `sp_io::offchain::timestamp()`
// is unavailable there. When we introduce a custom transaction pool, drop this and register a
// timestamp (offchain) extension scoped to the validation call instead, then use
// `sp_io::offchain::timestamp()` in `validate_unsigned`.
#[runtime_interface]
pub trait WallClock {
	/// Current UNIX time in milliseconds, from the node's system clock.
	/// Non-deterministic — only for the tx-pool validation path, never consensus.
	fn now_millis(&mut self) -> u64 {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|d| d.as_millis() as u64)
			.unwrap_or(0)
	}
}

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

pub mod time_units {
	use crate::BlockNumber;

	/// Milliseconds between Polkadot-like chain blocks.
	pub const MILLISECS_PER_BLOCK: u64 = 6000;

	/// A minute, expressed in Polkadot-like chain blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	/// A hour, expressed in Polkadot-like chain blocks.
	pub const HOURS: BlockNumber = MINUTES * 60;
	/// A day, expressed in Polkadot-like chain blocks.
	pub const DAYS: BlockNumber = HOURS * 24;
}

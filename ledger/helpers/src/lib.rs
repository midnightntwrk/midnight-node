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

mod utils;

pub use utils::find_dependency_version;
pub mod extract_tx_with_context;

/// Process-wide counters of replayed transactions that did not fully apply,
/// shared by every ledger generation's `LedgerContext`.
pub mod replay_stats {
	use std::sync::atomic::AtomicU64;

	pub static PARTIALLY_FAILED_TXS: AtomicU64 = AtomicU64::new(0);
	pub static FAILED_TXS: AtomicU64 = AtomicU64::new(0);
}

/// Strategy for ordering candidate coins/UTXOs during input selection.
///
/// Defined at the crate root (not inside `ledger_8`/`ledger_9`) so that all versions
/// see the same type, allowing it to flow through the toolkit's version-dispatched
/// builders unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoinSelectionStrategy {
	/// Use the largest coins/UTXOs first. Minimizes the number of inputs.
	#[default]
	LargestFirst,
	/// Use the smallest coins/UTXOs first. Consolidates dust.
	SmallestFirst,
}

/// Struct to store serialized verifying key bytes
/// To be deserialized when constructing ContractOperations
pub struct ContractVerifyingKeyBytes(pub Vec<u8>);

pub mod ledger_8;
pub mod ledger_9;

pub use ledger_9 as latest;

pub mod fork;

pub use latest::*;

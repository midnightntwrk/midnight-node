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

#[path = "versions"]
pub mod hard_fork_test {
	#[cfg(feature = "can-panic")]
	pub use super::extract_tx_with_context::extract_tx_with_context_ledger_hf as extract_tx_with_context;
	pub use {
		base_crypto_hf as base_crypto, coin_structure_hf as coin_structure,
		ledger_storage_hf as ledger_storage, midnight_serialize_hf as midnight_serialize,
		mn_ledger_hf as mn_ledger, onchain_runtime_hf as onchain_runtime,
		transient_crypto_hf as transient_crypto, zkir_hf as zkir, zswap_hf as zswap,
	};

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod ledger_7 {
	#[cfg(feature = "can-panic")]
	pub use super::extract_tx_with_context::extract_tx_with_context_ledger_7 as extract_tx_with_context;
	pub use {
		base_crypto, coin_structure, ledger_storage, midnight_serialize, mn_ledger,
		onchain_runtime, transient_crypto, zkir, zswap,
	};

	#[path = "block_context/pre_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod ledger_8 {
	#[cfg(feature = "can-panic")]
	pub use super::extract_tx_with_context::extract_tx_with_context_ledger_8 as extract_tx_with_context;
	pub use {
		base_crypto, coin_structure_ledger_8 as coin_structure,
		ledger_storage_ledger_8 as ledger_storage, midnight_serialize, mn_ledger_8 as mn_ledger,
		onchain_runtime_ledger_8 as onchain_runtime, transient_crypto_ledger_8 as transient_crypto,
		zkir_ledger_8 as zkir, zswap_ledger_8 as zswap,
	};

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

pub use ledger_8 as latest;

pub mod fork;

#[cfg(hardfork_test)]
pub use hard_fork_test::*;

#[cfg(not(hardfork_test))]
pub use latest::*;

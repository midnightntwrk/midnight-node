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

mod utils;

pub use utils::find_dependency_version;

#[path = "versions"]
pub mod hard_fork_test {
	#[cfg(feature = "std")]
	pub use {
		base_crypto_hf as base_crypto, coin_structure_hf as coin_structure,
		ledger_storage_hf as ledger_storage, midnight_serialize_hf as midnight_serialize,
		mn_ledger_hf as mn_ledger, onchain_runtime_hf as onchain_runtime,
		transient_crypto_hf as transient_crypto, zkir_hf as zkir, zswap_hf as zswap,
	};

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod latest {
	#[cfg(feature = "std")]
	pub use {
		base_crypto, coin_structure, ledger_storage, midnight_serialize, mn_ledger,
		onchain_runtime, transient_crypto, zkir, zswap,
	};

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[cfg(hardfork_test)]
pub use hard_fork_test::*;

#[cfg(not(hardfork_test))]
pub use latest::*;

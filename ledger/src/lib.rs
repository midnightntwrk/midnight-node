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

//! The Ledger crate provide host functions for the Node runtime
//!
//! We make use of module-parameterization here, an un-intentional feature of Rust
//! See this example code: https://www.reddit.com/r/rust/comments/yrihwb/comment/ivuzmgt
//!
//! This means we can use the same code for two different versions of the ledger crate
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod json;

#[cfg(feature = "std")]
mod storage;
#[cfg(feature = "std")]
mod utils;

#[cfg(feature = "std")]
pub use storage::*;

pub mod host_api;

#[path = "versions"]
pub mod hard_fork_test {
	#[cfg(feature = "std")]
	pub(crate) use {
		base_crypto_hf as base_crypto_local, coin_structure_hf as coin_structure_local,
		ledger_storage_hf as ledger_storage_local,
		midnight_node_ledger_helpers::hard_fork_test as helpers_local,
		midnight_serialize_hf as midnight_serialize_local, mn_ledger_hf as mn_ledger_local,
		onchain_runtime_hf as onchain_runtime_local, transient_crypto_hf as transient_crypto_local,
		zswap_hf as zswap_local,
	};

	pub const CRATE_NAME: &str = "mn-ledger-hf";
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod latest {
	#[cfg(feature = "std")]
	pub(crate) use {
		base_crypto as base_crypto_local, coin_structure as coin_structure_local,
		ledger_storage as ledger_storage_local,
		midnight_node_ledger_helpers::latest as helpers_local,
		midnight_serialize as midnight_serialize_local, mn_ledger as mn_ledger_local,
		onchain_runtime as onchain_runtime_local, transient_crypto as transient_crypto_local,
		zswap as zswap_local,
	};

	pub const CRATE_NAME: &str = "mn-ledger";
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

mod common;

pub mod types {
	pub use super::common::types::*;

	#[cfg(hardfork_test)]
	pub use super::hard_fork_test::types as active_version;
	#[cfg(hardfork_test)]
	pub use super::host_api::ledger_bridge_hf as active_ledger_bridge;

	#[cfg(not(hardfork_test))]
	pub use super::host_api::ledger_bridge as active_ledger_bridge;
	#[cfg(not(hardfork_test))]
	pub use super::latest::types as active_version;
}

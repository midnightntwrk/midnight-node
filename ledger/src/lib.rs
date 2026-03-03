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

//! The Ledger crate provide host functions for the Node runtime
//!
//! We make use of module-parameterization here, an un-intentional feature of Rust
//! See this example code: https://www.reddit.com/r/rust/comments/yrihwb/comment/ivuzmgt
//!
//! This means we can use the same code for two different versions of the ledger crate
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
pub mod json;

#[cfg(feature = "std")]
mod utils;

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

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	pub const CRATE_NAME: &str = "mn-ledger-hf";
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod ledger_7 {
	#[cfg(feature = "std")]
	pub(crate) use {
		base_crypto as base_crypto_local, coin_structure as coin_structure_local,
		ledger_storage as ledger_storage_local,
		midnight_node_ledger_helpers::ledger_7 as helpers_local,
		midnight_serialize as midnight_serialize_local, mn_ledger as mn_ledger_local,
		onchain_runtime as onchain_runtime_local, transient_crypto as transient_crypto_local,
		zswap as zswap_local,
	};

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/pre_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	pub const CRATE_NAME: &str = "mn-ledger";
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod ledger_8 {
	#[cfg(feature = "std")]
	pub(crate) use {
		base_crypto_ledger_8 as base_crypto_local, coin_structure_ledger_8 as coin_structure_local,
		ledger_storage_ledger_8 as ledger_storage_local,
		midnight_node_ledger_helpers::ledger_8 as helpers_local,
		midnight_serialize_ledger_8 as midnight_serialize_local, mn_ledger_8 as mn_ledger_local,
		onchain_runtime_ledger_8 as onchain_runtime_local,
		transient_crypto_ledger_8 as transient_crypto_local, zswap_ledger_8 as zswap_local,
	};

	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	pub const CRATE_NAME: &str = "mn-ledger-8";
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

pub use ledger_8 as latest;

#[cfg(feature = "std")]
fn drop_all_default_storage() {
	ledger_7::storage::drop_default_storage_if_exists();
	hard_fork_test::storage::drop_default_storage_if_exists();
	ledger_8::storage::drop_default_storage_if_exists();
}

mod common;

pub mod types {
	pub use super::common::types::*;

	#[cfg(hardfork_test)]
	pub use super::hard_fork_test::types as active_version;
	#[cfg(hardfork_test)]
	pub use super::host_api::ledger_hf::ledger_bridge_hf as active_ledger_bridge;

	#[cfg(not(hardfork_test))]
	pub use super::host_api::ledger_8::ledger_8_bridge as active_ledger_bridge;
	#[cfg(not(hardfork_test))]
	pub use super::latest::types as active_version;
}

#[cfg(test)]
mod tests {
	use frame_support::assert_ok;
	use ledger_storage_hf::{
		Storage as StorageHF, db::ParityDb as ParityDbHF,
		storage::set_default_storage as set_default_storage_hf,
	};
	use ledger_storage_ledger_8::{
		Storage,
		db::ParityDb,
		storage::{set_default_storage, try_get_default_storage, unsafe_drop_default_storage},
	};
	use std::path::PathBuf;

	#[test]
	fn set_and_drop_default_storage() {
		let mut db_path: PathBuf = std::env::temp_dir();
		db_path.push("node/chain");

		{
			// Set default storage
			let res = set_default_storage(|| {
				std::fs::create_dir_all(&db_path).unwrap_or_else(|err| {
					panic!("Failed to create dir {}, err {}", db_path.display(), err)
				});

				let db = ParityDb::<sha2::Sha256>::open(&db_path);

				Storage::new(0, db)
			});

			assert_ok!(res);
		}

		// Drop default storage
		unsafe_drop_default_storage::<ParityDb>();
		assert!(try_get_default_storage::<ParityDb>().is_none());

		// Reset default storage reusing the same `db_path`
		let res = set_default_storage_hf(|| {
			let db = ParityDbHF::<sha2::Sha256>::open(&db_path);
			StorageHF::new(0, db)
		});

		assert_ok!(res);
	}
}

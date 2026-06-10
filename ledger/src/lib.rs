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

	#[allow(clippy::duplicate_mod)]
	#[path = "error_ext/ledger_7.rs"]
	mod error_ext;

	#[path = "system_tx/ledger_7.rs"]
	mod system_tx;

	pub const CRATE_NAME: &str = "mn-ledger";
	#[cfg(feature = "std")]
	pub(crate) type TransactionSignature = base_crypto_local::signatures::Signature;
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod ledger_8 {
	#[cfg(feature = "std")]
	pub(crate) use {
		base_crypto as base_crypto_local, coin_structure as coin_structure_local,
		ledger_storage_ledger_8 as ledger_storage_local,
		midnight_node_ledger_helpers::ledger_8 as helpers_local,
		midnight_serialize as midnight_serialize_local, mn_ledger_8 as mn_ledger_local,
		onchain_runtime_ledger_8 as onchain_runtime_local,
		transient_crypto as transient_crypto_local, zswap_ledger_8 as zswap_local,
	};

	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[path = "error_ext/ledger_8.rs"]
	mod error_ext;

	#[path = "system_tx/ledger_8.rs"]
	mod system_tx;

	pub const CRATE_NAME: &str = "mn-ledger-8";
	#[cfg(feature = "std")]
	pub(crate) type TransactionSignature = base_crypto_local::signatures::Signature;
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

#[path = "versions"]
pub mod ledger_9 {
	#[cfg(feature = "std")]
	pub(crate) use {
		base_crypto as base_crypto_local, coin_structure as coin_structure_local,
		ledger_storage_ledger_8 as ledger_storage_local,
		midnight_node_ledger_helpers::ledger_9 as helpers_local,
		midnight_serialize as midnight_serialize_local, mn_ledger_9 as mn_ledger_local,
		onchain_runtime_ledger_9 as onchain_runtime_local,
		transient_crypto as transient_crypto_local, zswap_ledger_9 as zswap_local,
	};

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[path = "error_ext/ledger_9.rs"]
	mod error_ext;

	#[path = "system_tx/ledger_9.rs"]
	mod system_tx;

	pub const CRATE_NAME: &str = "mn-ledger-9";
	#[cfg(feature = "std")]
	pub(crate) type TransactionSignature = mn_ledger_local::structure::Signature;
	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;
}

pub use ledger_9 as latest;

#[cfg(feature = "std")]
/// Drops all versioned default ledger storages.
///
/// Intended to be called from the embedding application shutdown path (for
/// example after Tokio/node shutdown completes) to ensure DB-backed storage is
/// released deterministically.
pub fn drop_all_default_storage() {
	ledger_7::storage::drop_default_storage_if_exists();
	ledger_8::storage::drop_default_storage_if_exists();
	ledger_9::storage::drop_default_storage_if_exists();
}

#[cfg(feature = "std")]
/// Serialize the ledger arena snapshot at `state_key` into the canonical, `Ledger`-rooted warp
/// transfer blob (trustless warp ledger-sync, server side).
///
/// `unified` selects the ParityDb instantiation, matching the operator's `storage_separation`
/// config: the two modes register `default_storage` under different `D` type ids (separate = column
/// offset 0; unified = offset `NUM_COLUMNS_POLKADOT`, sharing substrate's parity-db). The blob bytes
/// are identical across modes.
///
/// Uses the latest ledger version (`ledger_9`): warp-sync targets are near the chain tip, where the
/// active ledger version is the latest. (Assumption — deferred: a node warp-syncing to a block
/// governed by an *older* ledger version would need per-version dispatch here; not reachable today
/// since warp always targets the tip.)
pub fn serialize_ledger_snapshot(
	unified: bool,
	state_key: &[u8],
) -> Result<Vec<u8>, ledger_9::api::LedgerApiError> {
	type Sig = ledger_9::TransactionSignature;
	type DbSeparate = ledger_9::ledger_storage_local::db::ParityDb;
	type DbUnified = ledger_9::ledger_storage_local::db::ParityDb<
		sha2::Sha256,
		ledger_9::ledger_storage_local::db::paritydb::OwnedDb,
		{ midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET },
	>;

	if unified {
		ledger_9::Bridge::<Sig, DbUnified>::serialize_ledger_snapshot(state_key)
	} else {
		ledger_9::Bridge::<Sig, DbSeparate>::serialize_ledger_snapshot(state_key)
	}
}

/// Failure modes of [`import_verified_ledger_snapshot`]. All are non-fatal to the chain: the caller
/// discards the data, reports the peer, and retries from another.
#[cfg(feature = "std")]
#[derive(Debug)]
pub enum SnapshotImportError {
	/// The on-chain `StateKey` bytes failed to decode to a `TypedArenaKey<Ledger>` (the inner
	/// `LedgerApiError` is version-specific, so it is rendered to a string here).
	StateKeyDecode(String),
	/// The transferred blob failed the arena's native (multi-pass, untrusted-safe) deserialization
	/// — malformed, truncated, or internally inconsistent node graph.
	Deserialize(std::io::Error),
	/// The blob deserialized cleanly but its recomputed root key does **not** equal the on-chain
	/// `StateKey`: the peer served a different (or tampered) ledger. **Never persisted.**
	RootMismatch,
}

#[cfg(feature = "std")]
impl core::fmt::Display for SnapshotImportError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			SnapshotImportError::StateKeyDecode(e) => {
				write!(f, "failed to decode on-chain StateKey: {e}")
			},
			SnapshotImportError::Deserialize(e) => {
				write!(f, "failed to deserialize ledger snapshot: {e}")
			},
			SnapshotImportError::RootMismatch => {
				write!(f, "ledger snapshot root key does not match on-chain StateKey")
			},
		}
	}
}

#[cfg(feature = "std")]
impl std::error::Error for SnapshotImportError {}

#[cfg(feature = "std")]
/// Verify a `Ledger`-rooted warp snapshot `blob` against the on-chain `expected_state_key` and, on
/// success, persist it into the already-open arena backend so `get_lazy(StateKey)` resolves (warp
/// ledger-sync verification + import). `unified` selects the DB instantiation, as in
/// [`serialize_ledger_snapshot`]. Uses the latest ledger version (`ledger_9`) — same near-tip
/// assumption noted there.
///
/// The caller must hold the authoring/import gate (the arena is single-writer).
pub fn import_verified_ledger_snapshot(
	unified: bool,
	blob: &[u8],
	expected_state_key: &[u8],
) -> Result<(), SnapshotImportError> {
	type Sig = ledger_9::TransactionSignature;
	type DbSeparate = ledger_9::ledger_storage_local::db::ParityDb;
	type DbUnified = ledger_9::ledger_storage_local::db::ParityDb<
		sha2::Sha256,
		ledger_9::ledger_storage_local::db::paritydb::OwnedDb,
		{ midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET },
	>;

	if unified {
		ledger_9::Bridge::<Sig, DbUnified>::import_verified_ledger_snapshot(
			blob,
			expected_state_key,
		)
	} else {
		ledger_9::Bridge::<Sig, DbSeparate>::import_verified_ledger_snapshot(
			blob,
			expected_state_key,
		)
	}
}

mod common;

pub mod types {
	pub use super::common::types::*;

	pub use super::host_api::ledger_9::ledger_9_bridge as active_ledger_bridge;
	pub use super::latest::types as active_version;
}

#[cfg(test)]
mod tests {
	use frame_support::assert_ok;
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
	}
}

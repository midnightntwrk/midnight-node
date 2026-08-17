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

	#[path = "guaranteed_validation/ledger_8.rs"]
	mod guaranteed_validation;

	#[path = "post_block_update/ledger_8.rs"]
	mod post_block_update;

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
		base_crypto as base_crypto_local, coin_structure_ledger_9 as coin_structure_local,
		ledger_storage_ledger_8 as ledger_storage_local,
		midnight_node_ledger_helpers::ledger_9 as helpers_local,
		midnight_serialize as midnight_serialize_local, mn_ledger_9 as mn_ledger_local,
		onchain_runtime_ledger_9 as onchain_runtime_local,
		transient_crypto_ledger_9 as transient_crypto_local, zswap_ledger_9 as zswap_local,
	};

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[path = "error_ext/ledger_9.rs"]
	mod error_ext;

	#[path = "system_tx/ledger_9.rs"]
	mod system_tx;

	#[path = "guaranteed_validation/ledger_9.rs"]
	mod guaranteed_validation;

	#[path = "post_block_update/ledger_9.rs"]
	mod post_block_update;

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
	ledger_8::storage::drop_default_storage_if_exists();
	ledger_9::storage::drop_default_storage_if_exists();
}

/// Parse the `vNN` from a `ledger-state[vNN]` tag embedded in a tagged blob (a `StateKey` or a
/// genesis_state). Used to dispatch warp serialize/import (and genesis-init) to the ledger module
/// whose `LedgerState` serialization matches: **v13 → `ledger_8`, v16/v17/v18 → `ledger_9`**.
/// A warp-syncing node can target a chain governed by an *older* ledger version than this build's
/// latest (e.g. a real devnet whose arena is still v13), so the version is read from the data, not
/// assumed to be the tip's.
#[cfg(feature = "std")]
pub fn ledger_state_tag_version(tagged: &[u8]) -> Option<u32> {
	const NEEDLE: &[u8] = b"ledger-state[v";
	let start = tagged.windows(NEEDLE.len()).position(|w| w == NEEDLE)? + NEEDLE.len();
	let rest = &tagged[start..];
	let end = rest.iter().position(|&b| b == b']')?;
	core::str::from_utf8(&rest[..end]).ok()?.parse().ok()
}

/// Expand to the `(DbSeparate, DbUnified)`-parameterized call of a `Bridge` arena method on the given
/// ledger version module (`ledger_8`/`ledger_9`), picking the DB instantiation by `unified`.
#[cfg(feature = "std")]
macro_rules! bridge_arena_call {
	($ver:ident, $unified:expr, $method:ident ( $($arg:expr),* )) => {{
		type DbSeparate = $ver::ledger_storage_local::db::ParityDb;
		type DbUnified = $ver::ledger_storage_local::db::ParityDb<
			sha2::Sha256,
			$ver::ledger_storage_local::db::paritydb::OwnedDb,
			{ midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET },
		>;
		if $unified {
			$ver::Bridge::<$ver::TransactionSignature, DbUnified>::$method( $($arg),* )
		} else {
			$ver::Bridge::<$ver::TransactionSignature, DbSeparate>::$method( $($arg),* )
		}
	}};
}

/// Serialize the ledger arena snapshot at `state_key` into the canonical, `Ledger`-rooted warp
/// transfer blob (trustless warp ledger-sync, server side). `unified` selects the ParityDb
/// instantiation (separate = column offset 0; unified = offset `NUM_COLUMNS_POLKADOT`); the blob is
/// identical across modes.
///
/// Dispatches to the ledger module matching the `StateKey`'s `ledger-state[vNN]` tag (see
/// [`ledger_state_tag_version`]) — so a warp node can serve an arena governed by an older ledger
/// version than this build's latest. Error rendered to `String` (the underlying `LedgerApiError` is
/// version-specific).
#[cfg(feature = "std")]
pub fn serialize_ledger_snapshot(unified: bool, state_key: &[u8]) -> Result<Vec<u8>, String> {
	match ledger_state_tag_version(state_key) {
		Some(16..=18) => {
			bridge_arena_call!(ledger_9, unified, serialize_ledger_snapshot(state_key))
				.map_err(|e| format!("{e:?}"))
		},
		Some(13) => bridge_arena_call!(ledger_8, unified, serialize_ledger_snapshot(state_key))
			.map_err(|e| format!("{e:?}")),
		other => Err(format!("unsupported ledger-state version {other:?} in StateKey")),
	}
}

/// Whether the local ledger arena holds the ledger state `state_key` points to (the `Ledger` root
/// node is readable). Cheap — a single arena root lookup, no DAG traversal.
///
/// Used by warp ledger-sync's recovery monitor to decide whether arena recovery is needed at all:
/// a node restarted *after* a completed recovery (or a normally full-synced node) already has the
/// state and must not re-fetch or re-gate; a node restarted *mid*-recovery does not, and must.
/// Returns `false` for an unsupported/undecodable `StateKey` (recovery will then verify against it
/// and fail loudly rather than silently skipping).
#[cfg(feature = "std")]
pub fn has_ledger_state(unified: bool, state_key: &[u8]) -> bool {
	match ledger_state_tag_version(state_key) {
		Some(16..=18) => {
			bridge_arena_call!(ledger_9, unified, get_ledger_state_root(state_key)).is_ok()
		},
		Some(13) => bridge_arena_call!(ledger_8, unified, get_ledger_state_root(state_key)).is_ok(),
		_ => false,
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
/// ledger-sync verification + import). `unified` selects the DB instantiation, and dispatch on the
/// `StateKey`'s `ledger-state[vNN]` tag picks the ledger module, as in
/// [`serialize_ledger_snapshot`].
///
/// The caller must hold the authoring/import gate (the arena is single-writer).
pub fn import_verified_ledger_snapshot(
	unified: bool,
	blob: &[u8],
	expected_state_key: &[u8],
) -> Result<(), SnapshotImportError> {
	// Dispatch on the `StateKey`'s ledger-state version (the underlying method returns the shared
	// `SnapshotImportError` for every version, so no error mapping is needed).
	match ledger_state_tag_version(expected_state_key) {
		Some(16..=18) => {
			bridge_arena_call!(
				ledger_9,
				unified,
				import_verified_ledger_snapshot(blob, expected_state_key)
			)
		},
		Some(13) => {
			bridge_arena_call!(
				ledger_8,
				unified,
				import_verified_ledger_snapshot(blob, expected_state_key)
			)
		},
		other => Err(SnapshotImportError::StateKeyDecode(format!(
			"unsupported ledger-state version {other:?} in StateKey"
		))),
	}
}

/// Seed the (separate) ledger arena from a genesis `LedgerState` blob, using the
/// deserializer that matches the blob's `ledger-state[vN]` header tag.
///
/// A node may boot on a chain-spec produced by an older runtime — notably the
/// ledger 8->9 hardfork, where a ledger-9 node starts from a ledger-8
/// (`ledger-state[v13]`) genesis and only upgrades to v9 later via the runtime
/// migration. Seeding must therefore match the genesis version (the genesis
/// block runs under the old WASM and expects the old-format arena root), not the
/// latest. v8 and v9 share one storage backend, so a v8-seeded arena is exactly
/// what the post-migration v9 runtime reads. Unrecognized tags fall back to the
/// latest version (`ledger_9`), preserving the prior default behaviour.
#[cfg(feature = "std")]
pub fn init_ledger_storage_separate<P: AsRef<std::path::Path>>(
	dir: P,
	genesis_state: &[u8],
	cache_size: usize,
) -> alloc::vec::Vec<u8> {
	if ledger_8::storage::genesis_matches_this_version(genesis_state) {
		ledger_8::storage::init_storage_paritydb_separate(dir, genesis_state, cache_size)
	} else {
		ledger_9::storage::init_storage_paritydb_separate(dir, genesis_state, cache_size)
	}
}

/// Unified-DB counterpart of [`init_ledger_storage_separate`].
#[cfg(feature = "std")]
pub fn init_ledger_storage_unified<
	D: core::ops::Deref<Target = parity_db::Db> + Default + Send + Sync + 'static,
	const COLUMN_OFFSET: u8,
>(
	db_instance: D,
	genesis_state: &[u8],
	cache_size: usize,
) -> alloc::vec::Vec<u8> {
	if ledger_8::storage::genesis_matches_this_version(genesis_state) {
		ledger_8::storage::init_storage_paritydb_unified::<D, COLUMN_OFFSET>(
			db_instance,
			genesis_state,
			cache_size,
		)
	} else {
		ledger_9::storage::init_storage_paritydb_unified::<D, COLUMN_OFFSET>(
			db_instance,
			genesis_state,
			cache_size,
		)
	}
}

/// Returns true if `state_key` is a ledger-8 arena root, i.e. a tagged-serialized
/// `TypedArenaKey<ledger_8::api::Ledger<_>, _>`.
#[cfg(feature = "std")]
pub(crate) fn is_ledger_8_state_key(state_key: &[u8]) -> bool {
	use ledger_storage_ledger_8::{DefaultDB, arena::TypedArenaKey, db::DB};
	use midnight_serialize::Tagged;

	type Ledger8Root = TypedArenaKey<ledger_8::api::Ledger<DefaultDB>, <DefaultDB as DB>::Hasher>;

	let expected = <Ledger8Root as Tagged>::tag();
	match midnight_serialize::peek_tag(&mut std::io::Cursor::new(state_key)) {
		Ok(tag) => tag.as_str() == expected.as_ref(),
		Err(_) => false,
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

	/// `is_ledger_8_state_key` is what the ledger-9 host API dispatches on to read the
	/// `set_code` block of the 8->9 hardfork, whose `StateKey` is one version behind
	/// its `:code` (GH #1959). It has to tell a ledger-8 arena root from a ledger-9
	/// one from the header tag alone.
	#[test]
	fn ledger_8_state_key_tag_is_recognised() {
		use ledger_storage_ledger_8::DefaultDB;
		use midnight_serialize::{GLOBAL_TAG, Tagged};

		// A `StateKey` is `tagged_serialize(&Sp<Ledger<D>, D>::as_typed_key())`, and
		// `TypedArenaKey`'s tag wraps its referent's — which for `Ledger` is just
		// `LedgerState`'s. Only the header matters here; `peek_tag` never reads the body.
		fn header<T: Tagged>() -> Vec<u8> {
			format!("{GLOBAL_TAG}storage-key({}):", T::tag()).into_bytes()
		}
		let v8 = header::<mn_ledger_8::structure::LedgerState<DefaultDB>>();
		let v9 = header::<mn_ledger_9::structure::LedgerState<DefaultDB>>();
		assert_ne!(v8, v9, "v8 and v9 ledger states must not share a tag");

		assert!(super::is_ledger_8_state_key(&v8));
		assert!(!super::is_ledger_8_state_key(&v9));

		// An unset `StateKey`, or anything else untagged, is not a ledger-8 root: the
		// host API must take its ordinary ledger-9 path rather than guess.
		assert!(!super::is_ledger_8_state_key(&[]));
		assert!(!super::is_ledger_8_state_key(b"not-tagged-at-all"));
	}
}

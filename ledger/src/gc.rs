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

//! Garbage collection for the ledger storage arena.
//!
//! Every ledger host call that mutates state (`apply_transaction`,
//! `apply_system_transaction`, `apply_post_block_update`, ...) persists the
//! resulting state root in the storage arena, and nothing ever unpersists
//! them. Left alone, the arena therefore accumulates every per-transaction
//! and per-block state root since genesis — including roots from discarded
//! block proposals — regardless of the node's pruning configuration
//! (see <https://github.com/midnightntwrk/midnight-node/issues/1983>).
//!
//! This module exposes [`collect_garbage`], which runs the storage crate's
//! incremental, time-budgeted mark-and-sweep collector
//! ([`StorageBackend::gc_override_gc_roots`]) against an explicit set of
//! *live* state keys supplied by the caller. Anything unreachable from those
//! roots (or from in-flight, in-memory objects, which the backend tracks
//! itself via `live_inserts`) is deleted, and any persisted root not in the
//! set is effectively unpersisted. The node derives the live set from the
//! `Midnight::StateKey` storage value of every block whose Substrate state
//! is still retained, so the arena's retention exactly matches the node's
//! state-pruning window.
//!
//! Using an override root set (rather than matched `unpersist` calls)
//! sidesteps the persist reference-counting problem: the same root may be
//! persisted several times (block authoring and import both execute the
//! block; intermediate per-transaction roots are superseded within the same
//! block) and would otherwise need an exactly matched number of unpersists.
//!
//! Only the ledger-8/9 storage backend (`ledger-storage-ledger-8`, shared by
//! both ledger versions) supports garbage collection; the legacy ledger-7
//! storage crate predates the collector. Callers must therefore only invoke
//! this while the node is not re-executing ledger-7-era blocks (in practice:
//! not during major sync).

use std::time::Duration;

use midnight_primitives_ledger::LedgerStorageExt;

use ledger_storage_ledger_8::{
	arena::{ArenaHash, ArenaKey, TypedArenaKey},
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::try_get_default_storage,
};
use midnight_serialize::tagged_deserialize;

const LOG_TARGET: &str = "midnight::ledger_gc";

// The two `ParityDb` instantiations under which the ledger-8/9 storage may be
// registered, depending on the operator's `storage_separation` config (see
// `host_api/ledger_8.rs`).
type DbSeparate = ParityDb;
type DbUnified = ParityDb<sha2::Sha256, OwnedDb, { LedgerStorageExt::COLUMN_OFFSET }>;

/// Result of one garbage-collection slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcOutcome {
	/// Number of arena nodes deleted from storage during this slice.
	pub culled: usize,
	/// Number of live root hashes derived from the supplied state keys.
	pub live_roots: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcError {
	/// A supplied state key failed to deserialize as a ledger-8 or ledger-9
	/// state key. The GC slice is aborted: culling with an incomplete root
	/// set could delete live data.
	UnrecognizedStateKey,
	/// No GC-capable ledger storage (ledger-8/9) is initialized yet.
	StorageNotInitialized,
}

impl core::fmt::Display for GcError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			GcError::UnrecognizedStateKey => {
				write!(f, "state key is not a recognized ledger-8/9 state key")
			},
			GcError::StorageNotInitialized => {
				write!(f, "ledger-8/9 storage is not initialized")
			},
		}
	}
}

/// Convert serialized `Midnight::StateKey` values into arena root hashes.
///
/// A state key is a tagged `TypedArenaKey<Ledger<D>>`; the tag differs
/// between ledger versions, so both are tried (a retention window spanning
/// the 8->9 hardfork contains keys of both tags). Small states may be
/// serialized as a `Direct` key embedding the object inline — in that case
/// the key itself holds the data and the roots to protect are its referenced
/// children.
fn state_key_roots<D: DB>(state_keys: &[Vec<u8>]) -> Result<Vec<ArenaHash<D::Hasher>>, GcError> {
	let mut roots = Vec::with_capacity(state_keys.len());
	for bytes in state_keys {
		let key: ArenaKey<D::Hasher> = if let Ok(key) = tagged_deserialize::<
			TypedArenaKey<crate::ledger_9::api::Ledger<D>, D::Hasher>,
		>(&mut &bytes[..])
		{
			key.into()
		} else if let Ok(key) = tagged_deserialize::<
			TypedArenaKey<crate::ledger_8::api::Ledger<D>, D::Hasher>,
		>(&mut &bytes[..])
		{
			key.into()
		} else {
			// Expected while the live window still contains pre-ledger-8
			// blocks (initial sync): their state keys carry older tags. The
			// caller skips the slice and retries later.
			log::debug!(
				target: LOG_TARGET,
				"Aborting GC slice: unrecognized state key ({} bytes)",
				bytes.len()
			);
			return Err(GcError::UnrecognizedStateKey);
		};
		roots.extend(key.refs().into_iter().cloned());
	}
	Ok(roots)
}

fn gc_storage<D: DB + std::any::Any>(
	budget: Duration,
	state_keys: &[Vec<u8>],
) -> Result<Option<GcOutcome>, GcError> {
	let Some(storage) = try_get_default_storage::<D>() else {
		return Ok(None);
	};
	let roots = state_key_roots::<D>(state_keys)?;
	let live_roots = roots.len();
	let culled = storage
		.arena
		.with_backend(|backend| backend.gc_override_gc_roots(budget, move |_| roots));
	Ok(Some(GcOutcome { culled, live_roots }))
}

/// Run one time-budgeted garbage-collection slice on the default ledger-8/9
/// storage, treating `live_state_keys` (serialized `Midnight::StateKey`
/// values of all blocks the node can still serve) as the complete set of
/// persisted roots to keep.
///
/// **Warning**: any persisted root *not* reachable from `live_state_keys` is
/// irreversibly unpersisted and its exclusively-owned data deleted. The
/// caller is responsible for supplying every root it still needs.
///
/// The collector is incremental: a slice that exhausts `budget` parks its
/// progress and resumes on the next call, so a return of `culled == 0` does
/// not mean no progress was made. The backend lock is held for roughly
/// `budget`, which blocks concurrent ledger host calls — keep budgets small
/// (hundreds of milliseconds).
pub fn collect_garbage(
	budget: Duration,
	live_state_keys: &[Vec<u8>],
) -> Result<GcOutcome, GcError> {
	// Exactly one of the two instantiations is registered in practice
	// (separate vs unified storage); run on whichever exists.
	if let Some(outcome) = gc_storage::<DbSeparate>(budget, live_state_keys)? {
		return Ok(outcome);
	}
	if let Some(outcome) = gc_storage::<DbUnified>(budget, live_state_keys)? {
		return Ok(outcome);
	}
	Err(GcError::StorageNotInitialized)
}

#[cfg(test)]
mod tests {
	use super::*;
	use ledger_storage_ledger_8::{DefaultDB, storage::default_storage};
	use midnight_serialize::tagged_serialize;

	// These tests use the process-global `InMemoryDB` default storage (the
	// same one the api tests use, auto-initialized on first use). That is
	// safe with concurrently running tests: their live objects are held via
	// `Sp`s, which the collector protects through the backend's in-memory
	// root tracking, and no other test in this crate persists roots into the
	// InMemoryDB storage.

	fn alloc_persisted_ledger_9(
		network_id: &str,
	) -> (
		ledger_storage_ledger_8::arena::Sp<crate::ledger_9::api::Ledger<DefaultDB>, DefaultDB>,
		Vec<u8>,
	) {
		use crate::ledger_9::api::Ledger;
		use mn_ledger_9::structure::LedgerState;

		let state: LedgerState<DefaultDB> = LedgerState::new(network_id);
		let mut sp = default_storage::<DefaultDB>().arena.alloc(Ledger::new(state));
		sp.persist();
		let mut key = vec![];
		tagged_serialize(&sp.as_typed_key(), &mut key).expect("state key serializes");
		(sp, key)
	}

	#[test]
	fn state_key_roots_resolves_ledger_9_keys() {
		let (sp, key) = alloc_persisted_ledger_9("gc-test-parse");
		let roots = state_key_roots::<DefaultDB>(&[key]).expect("key parses");
		// After `persist()` the Sp's child repr is a Ref, so the parsed
		// root set is exactly the persisted root hash.
		assert_eq!(roots, vec![sp.hash()]);
	}

	#[test]
	fn state_key_roots_rejects_garbage() {
		assert_eq!(
			state_key_roots::<DefaultDB>(&[b"not a state key".to_vec()]),
			Err(GcError::UnrecognizedStateKey)
		);
	}

	#[test]
	fn gc_culls_roots_missing_from_live_set_and_keeps_live_ones() {
		use crate::ledger_9::api::Ledger;
		use ledger_storage_ledger_8::arena::TypedArenaKey;

		let (sp_live, key_live) = alloc_persisted_ledger_9("gc-test-live");
		let (sp_dead, key_dead) = alloc_persisted_ledger_9("gc-test-dead");
		let typed_live: TypedArenaKey<Ledger<DefaultDB>, _> = sp_live.as_typed_key();
		let typed_dead: TypedArenaKey<Ledger<DefaultDB>, _> = sp_dead.as_typed_key();
		default_storage::<DefaultDB>().with_backend(|b| b.flush_all_changes_to_db());
		// Drop in-memory refs so only persistence keeps the states alive.
		drop(sp_live);
		drop(sp_dead);

		// GC with only the live key in the root set; a generous budget lets
		// a full mark + sweep cycle finish in one slice.
		let outcome =
			gc_storage::<DefaultDB>(std::time::Duration::from_secs(3600), &[key_live.clone()])
				.expect("keys parse")
				.expect("in-memory storage is initialized");
		assert_eq!(outcome.live_roots, 1);
		assert!(outcome.culled >= 1, "the dead state's root node should be culled");

		// The live state survives and is fully loadable. `Arena::get` loads
		// eagerly (unlike `get_lazy`, which succeeds without touching the
		// DB and only fails on later access).
		let live = default_storage::<DefaultDB>()
			.arena
			.get(&typed_live)
			.expect("live state still loadable");
		assert_eq!(live.state.network_id.as_str(), "gc-test-live");

		// The dead state's root is gone.
		assert!(
			default_storage::<DefaultDB>().arena.get(&typed_dead).is_err(),
			"dead state should have been culled"
		);
		// `key_dead` no longer parses to anything loadable, but the GC input
		// path still accepts it (parsing is independent of liveness).
		assert!(state_key_roots::<DefaultDB>(&[key_dead]).is_ok());
	}
}

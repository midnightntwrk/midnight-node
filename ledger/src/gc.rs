// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Ledger storage garbage collection primitives (single-scan release shape).
//!
//! Every function here operates on whichever default ledger storage is
//! initialised (`DbSeparate`, then `DbUnified`, then the in-memory `DefaultDB`
//! used by tests). Each takes the arena mutex (`Arena::with_backend`) **at most
//! once**, and returns how long it held it so the caller (the node worker) can
//! decide about logging and metrics. This module never logs above `debug!`.
//!
//! # Shape
//!
//! The node worker is stateless across slices: every slice runs [`scan`] (one
//! `get_roots` hold, then the wrappers' tags in batches of at most 1000 reads per
//! hold), decides liveness with **no lock held**, pins the wrappers it saw
//! ([`pin_roots`]), runs one incremental sweep slice ([`collect_garbage`]) and
//! finally releases the dead wrappers ([`release_to_zero`]: one hold doing nothing
//! but `unpersist` x `count` on the hashes handed in from the same slice's scan).
//! Per-slice work is proportional to the current root set, which on a pruned
//! node is the pruning window plus forks. GC does not run on a database that holds
//! a legacy (pre-wrapper) root: [`scan`] stops at the first one, so the root set
//! never contains the unbounded history of an upgraded-in-place node.
//!
//! [`root_snapshot`] and [`classify`] are the two building blocks of [`scan`].
//!
//! # Over-release is fatal, under-release self-heals
//!
//! `StorageBackend::unpersist` is a blind decrement whose result is asserted
//! non-negative when the staged changes are flushed, so releasing a wrapper more
//! often than it was persisted **panics the node at the next flush**. Hence:
//!
//! * this module is the only code that ever unpersists a tagged wrapper;
//! * a release list carries counts from the same slice's scan, is consumed once,
//!   and is never persisted or retried;
//! * between the scan and the release the count can only grow (a concurrent
//!   `persist` of the same `(tag, inner)`), so the leftover is picked up by a later
//!   slice.

use crate::tagged_root::{TaggedRoot, classify_batch, root_snapshot as arena_root_snapshot};
use ledger_storage_ledger_8::{
	DefaultDB,
	arena::{Arena, ArenaHash, ArenaKey, TypedArenaKey},
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::{default_storage, try_get_default_storage},
};
use midnight_primitives_ledger::LedgerStorageExt;
use std::{
	any::Any,
	collections::HashMap,
	time::{Duration, Instant},
};

/// Hashes classified per arena hold by [`classify`].
const CLASSIFY_BATCH: usize = 1000;
/// Hashes whose presence is checked per arena hold by [`pin_roots`].
const PIN_BATCH: usize = 500;

const LOG_TARGET: &str = "midnight::ledger_gc";

type DbSeparate = ParityDb;
type DbUnified = ParityDb<sha2::Sha256, OwnedDb, { LedgerStorageExt::COLUMN_OFFSET }>;

/// Run `$f::<D>($args)` against the first initialised default storage, probing
/// `DbSeparate`, `DbUnified`, `DefaultDB` in that order. Evaluates to `None`
/// when no storage is initialised.
///
/// Under `cfg(test)` the in-memory `DefaultDB` is probed first: other tests in this
/// crate initialise a parity-db default storage, which would otherwise shadow it.
macro_rules! dispatch {
	($f:ident($($arg:expr),*)) => {{
		#[cfg(test)]
		let in_memory_first = try_get_default_storage::<DefaultDB>().is_some();
		#[cfg(not(test))]
		let in_memory_first = false;
		if in_memory_first {
			Some($f::<DefaultDB>($($arg),*))
		} else if try_get_default_storage::<DbSeparate>().is_some() {
			Some($f::<DbSeparate>($($arg),*))
		} else if try_get_default_storage::<DbUnified>().is_some() {
			Some($f::<DbUnified>($($arg),*))
		} else if try_get_default_storage::<DefaultDB>().is_some() {
			Some($f::<DefaultDB>($($arg),*))
		} else {
			None
		}
	}};
}

/// A tagged-root wrapper found by [`tagged_roots`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaggedRootInfo {
	/// The wrapper's 32-byte arena hash.
	pub wrapper: Vec<u8>,
	/// The opaque tag (the parent block hash of the producing block).
	pub tag: Vec<u8>,
	/// How many times the wrapper is currently persisted.
	pub count: u32,
}

/// Opaque handle keeping a tagged-root wrapper in the backend's `live_inserts`.
///
/// Dropping it releases the pin. See [`pin_roots`].
pub struct PinnedRoot {
	_sp: Box<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for PinnedRoot {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PinnedRoot").finish_non_exhaustive()
	}
}

fn hash_from_bytes<D: DB>(bytes: &[u8]) -> Option<ArenaHash<D::Hasher>> {
	let mut hash = ArenaHash::<D::Hasher>::default();
	if hash.0.len() != bytes.len() {
		return None;
	}
	hash.0.copy_from_slice(bytes);
	Some(hash)
}

/// A GC root that is not a tagged-root wrapper: written before tagged roots, or
/// unreadable. It can never be released, so GC does not run on a database that
/// holds one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyRoot;

/// Every GC root with its tag and persist count, or `Err(LegacyRoot)` as soon as
/// one root is not a wrapper (the remaining roots are not read). One
/// [`root_snapshot`] hold plus [`classify`] holds of at most 1000 reads each, the
/// arena mutex released between them. Also returns the total time the arena was
/// held. Uninitialised storage yields an empty scan and `Duration::ZERO`.
pub fn scan() -> (Result<Vec<TaggedRootInfo>, LegacyRoot>, Duration) {
	let (snapshot, mut hold) = root_snapshot();
	let hashes: Vec<Vec<u8>> = snapshot.keys().cloned().collect();
	let mut out = Vec::with_capacity(hashes.len());
	for batch in hashes.chunks(CLASSIFY_BATCH) {
		let start = Instant::now();
		let verdicts = classify(batch);
		hold += start.elapsed();
		for (wrapper, tag) in verdicts {
			let Some(tag) = tag else { return (Err(LegacyRoot), hold) };
			let count = snapshot[&wrapper];
			out.push(TaggedRootInfo { wrapper, tag, count });
		}
	}
	(Ok(out), hold)
}

/// Every tagged-root wrapper, skipping non-wrapper roots instead of stopping at
/// them (tests and tooling; the worker uses [`scan`]).
pub fn tagged_roots() -> (Vec<TaggedRootInfo>, Duration) {
	let (snapshot, mut hold) = root_snapshot();
	let hashes: Vec<Vec<u8>> = snapshot.keys().cloned().collect();
	let mut out = Vec::new();
	for batch in hashes.chunks(CLASSIFY_BATCH) {
		let start = Instant::now();
		let verdicts = classify(batch);
		hold += start.elapsed();
		out.extend(verdicts.into_iter().filter_map(|(wrapper, tag)| {
			let count = snapshot[&wrapper];
			tag.map(|tag| TaggedRootInfo { wrapper, tag, count })
		}));
	}
	(out, hold)
}

/// Every GC root's 32-byte hash with its persist count: ONE arena hold doing only
/// `get_roots()` (O(roots) column iteration, no point reads), plus the measured hold.
/// Uninitialised storage yields an empty map and `Duration::ZERO`.
pub fn root_snapshot() -> (HashMap<Vec<u8>, u32>, Duration) {
	fn snapshot_in<D: DB + 'static>() -> (HashMap<Vec<u8>, u32>, Duration) {
		let (roots, hold) = arena_root_snapshot(&default_storage::<D>().arena);
		(roots.into_iter().map(|(h, c)| (h.0.to_vec(), c)).collect(), hold)
	}
	dispatch!(snapshot_in()).unwrap_or_default()
}

/// Classify roots by hash: `Some(tag)` = tagged-root wrapper, `None` = legacy
/// (anything else, including a malformed hash or an unreadable node, which the
/// caller must then never release). Works in batches of at most 1000 hashes per
/// arena hold and releases the mutex between batches. The output has one entry per
/// input hash, in input order.
pub fn classify(hashes: &[Vec<u8>]) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
	fn classify_in<D: DB + 'static>(hashes: &[Vec<u8>]) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
		let arena = &default_storage::<D>().arena;
		let mut out = Vec::with_capacity(hashes.len());
		for batch in hashes.chunks(CLASSIFY_BATCH) {
			let parsed: Vec<Option<ArenaHash<D::Hasher>>> =
				batch.iter().map(|b| hash_from_bytes::<D>(b)).collect();
			let valid: Vec<_> = parsed.iter().flatten().cloned().collect();
			let mut verdicts = classify_batch(arena, &valid).into_iter();
			for (bytes, hash) in batch.iter().zip(&parsed) {
				let tag = match hash {
					Some(_) => verdicts.next().and_then(|(_, tag)| tag),
					None => None,
				};
				out.push((bytes.clone(), tag));
			}
		}
		out
	}
	if hashes.is_empty() {
		return Vec::new();
	}
	dispatch!(classify_in(hashes))
		.unwrap_or_else(|| hashes.iter().map(|h| (h.clone(), None)).collect())
}

/// Unpersist each target wrapper exactly `count` times. ONE arena hold, stage
/// only (no flush; durability is the next block-boundary flush), and no reads of
/// any kind inside the hold. Returns `(wrappers released, hold)`.
///
/// `targets` must carry the counts of a root scan (a count can only have grown
/// since); the list must be consumed once and never retried
/// (over-release panics at flush, see the module docs). Targets whose hash is
/// not 32 bytes and targets with count 0 are skipped. A list with nothing left to release
/// takes no lock.
pub fn release_to_zero(targets: &[(Vec<u8>, u32)]) -> (usize, Duration) {
	fn release<D: DB + 'static>(targets: &[(Vec<u8>, u32)]) -> (usize, Duration) {
		let hashes: Vec<(ArenaHash<D::Hasher>, u32)> = targets
			.iter()
			.filter(|(_, count)| *count > 0)
			.filter_map(|(bytes, count)| {
				let hash = hash_from_bytes::<D>(bytes);
				if hash.is_none() {
					log::debug!(target: LOG_TARGET, "skipping release of malformed wrapper hash");
				}
				hash.map(|h| (h, *count))
			})
			.collect();
		if hashes.is_empty() {
			return (0, Duration::ZERO);
		}
		let storage = default_storage::<D>();
		let start = Instant::now();
		// Hold: unpersist only. No get_roots, no get.
		storage.with_backend(|backend| {
			for (hash, count) in &hashes {
				for _ in 0..*count {
					backend.unpersist(hash);
				}
			}
		});
		(hashes.len(), start.elapsed())
	}
	if targets.is_empty() {
		return (0, Duration::ZERO);
	}
	dispatch!(release(targets)).unwrap_or_default()
}

/// Hold the given wrappers (lazily loaded) so that they stay in the backend's
/// `live_inserts` across `gc()` slices; the crate re-reads on-disk roots only
/// on rescan, so a wrapper persisted after a sweep began would otherwise be
/// invisible to it. Unknown or non-wrapper hashes are skipped (`debug!` on load failure).
///
/// The wrapper is loaded as `TaggedRoot<Ledger<D>, D>` via `get_lazy`. The
/// wrapper's `Storable` implementation only reads the tag and takes the single
/// child key; the inner value is never forced, so the type parameter does not
/// restrict this to one ledger version (verified by the tests, which pin a
/// ledger-9 wrapper).
///
/// The returned handles are `Send + 'static`; the worker holds them across its sweep.
/// Each handle is returned with the hash it pins (targets that are absent or not
/// wrappers have no entry). Presence is checked in holds of at most [`PIN_BATCH`]
/// hashes, releasing the arena mutex in between, so pinning a large set never stalls
/// block import for long.
pub fn pin_roots(wrappers: &[Vec<u8>]) -> Vec<(Vec<u8>, PinnedRoot)> {
	fn pin<D: DB + 'static>(wrappers: &[Vec<u8>]) -> Vec<(Vec<u8>, PinnedRoot)> {
		use crate::ledger_9::api::Ledger;
		let arena: &Arena<D> = &default_storage::<D>().arena;
		let mut out = Vec::with_capacity(wrappers.len());
		for batch in wrappers.chunks(PIN_BATCH) {
			let hashes: Vec<_> = batch.iter().filter_map(|b| hash_from_bytes::<D>(b)).collect();
			// `get_lazy` does not check that the node exists, and a pin on an absent or
			// foreign node would hand gc a bogus root. One short hold confirms each
			// target in the batch is a stored wrapper before it is pinned.
			let present: Vec<_> = default_storage::<D>().with_backend(|backend| {
				hashes
					.into_iter()
					.filter(|h| {
						backend.get(h).and_then(crate::tagged_root::parse_wrapper).is_some()
					})
					.collect()
			});
			for hash in present {
				let bytes = hash.0.to_vec();
				let key: TypedArenaKey<TaggedRoot<Ledger<D>, D>, _> =
					TypedArenaKey::from(ArenaKey::Ref(hash));
				match arena.get_lazy(&key) {
					Ok(sp) => out.push((bytes, PinnedRoot { _sp: Box::new(sp) })),
					Err(e) => {
						log::debug!(target: LOG_TARGET, "cannot pin wrapper: {e}");
					},
				}
			}
		}
		out
	}
	if wrappers.is_empty() {
		return Vec::new();
	}
	dispatch!(pin(wrappers)).unwrap_or_default()
}

/// Run incremental arena GC for roughly `budget`. ONE arena hold. Sweeps only
/// when the write cache is empty (reachability is DB based, staged state is
/// invisible to it); otherwise returns `Ok((0, hold))`. Returns
/// `(culled nodes, hold)`; `Err` only when no storage is initialised.
pub fn collect_garbage(budget: Duration) -> Result<(usize, Duration), String> {
	fn gc<D: DB + 'static>(budget: Duration) -> (usize, Duration) {
		let storage = default_storage::<D>();
		let start = Instant::now();
		let culled = storage.with_backend(|backend| {
			if backend.get_write_cache_len() > 0 { 0 } else { backend.gc(budget) }
		});
		(culled, start.elapsed())
	}
	dispatch!(gc(budget)).ok_or_else(|| "ledger storage is not initialized".to_string())
}

/// Diagnostic: current write-cache length (one short hold). `None` when no
/// storage is initialised.
pub fn write_cache_len() -> Option<usize> {
	fn len<D: DB + 'static>() -> usize {
		default_storage::<D>().with_backend(|backend| backend.get_write_cache_len())
	}
	dispatch!(len())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ledger_9::api::Ledger,
		tagged_root::{persist_tagged, tagged_pin_count},
	};
	use ledger_storage_ledger_8::{Storable, storable::Loader};
	use mn_ledger_9::structure::LedgerState;
	use std::{
		io::{Error, Read, Write},
		sync::{Mutex, MutexGuard},
	};

	/// A leaf large enough to be a real (non-inlined) node, unlike a small ledger state.
	#[derive(Clone)]
	struct FatLeaf(u64);

	impl<D: DB> Storable<D> for FatLeaf {
		fn children(&self) -> Vec<ArenaKey<D::Hasher>> {
			Vec::new()
		}

		fn to_binary_repr<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
			writer.write_all(&[0u8; 1024])?;
			writer.write_all(&self.0.to_le_bytes())
		}

		fn from_binary_repr<R: Read>(
			reader: &mut R,
			_child_nodes: &mut impl Iterator<Item = ArenaKey<D::Hasher>>,
			loader: &impl Loader<D>,
		) -> Result<Self, Error> {
			let mut pad = [0u8; 1024];
			reader.read_exact(&mut pad)?;
			let mut value = [0u8; 8];
			reader.read_exact(&mut value)?;
			loader.do_check(FatLeaf(u64::from_le_bytes(value)))
		}
	}

	/// The default arena is process-global and gc reads the whole root set, so
	/// serialize these tests.
	static SERIAL: Mutex<()> = Mutex::new(());

	fn serial() -> MutexGuard<'static, ()> {
		SERIAL.lock().unwrap_or_else(|e| e.into_inner())
	}

	fn arena() -> Arena<DefaultDB> {
		default_storage::<DefaultDB>().arena.clone()
	}

	fn flush() {
		default_storage::<DefaultDB>().with_backend(|b| b.flush_all_changes_to_db());
	}

	fn inner(network_id: &str) -> ledger_storage_ledger_8::arena::Sp<Ledger<DefaultDB>, DefaultDB> {
		arena().alloc(Ledger::new(LedgerState::new(network_id)))
	}

	fn tag(n: u32) -> Vec<u8> {
		let mut t = vec![0xc0; 28];
		t.extend_from_slice(&n.to_le_bytes());
		t
	}

	fn info(wrapper: &[u8]) -> Option<TaggedRootInfo> {
		tagged_roots().0.into_iter().find(|r| r.wrapper == wrapper)
	}

	fn wrapper_is_root(wrapper: &[u8]) -> bool {
		let hash = hash_from_bytes::<DefaultDB>(wrapper).unwrap();
		default_storage::<DefaultDB>().with_backend(|b| b.get_roots().contains_key(&hash))
	}

	#[test]
	fn release_to_zero_releases_the_full_count() {
		let _g = serial();
		let inner = inner("gc-full-count");
		let mut wrapper = None;
		for _ in 0..3 {
			wrapper = Some(persist_tagged(&arena(), tag(1001), &inner));
		}
		let w = wrapper.unwrap().hash().0.to_vec();
		flush();

		let found = info(&w).expect("wrapper listed");
		assert_eq!(found.count, 3);
		assert_eq!(found.tag, tag(1001));
		assert_eq!(tagged_pin_count(&arena(), &inner.hash()), Some(3));

		let (n, _) = release_to_zero(&[(w.clone(), found.count)]);
		assert_eq!(n, 1);
		flush();
		assert_eq!(tagged_pin_count(&arena(), &inner.hash()), None);
		assert!(!wrapper_is_root(&w));
		// A replay has nothing to do: the wrapper is no longer listed.
		assert!(info(&w).is_none());
	}

	#[test]
	fn release_to_zero_ignores_zero_counts() {
		let _g = serial();
		let inner = inner("gc-zero-count");
		let w = persist_tagged(&arena(), tag(1007), &inner).hash().0.to_vec();
		flush();

		// A zero-count target releases nothing and takes no lock: hold the arena mutex on
		// another thread and require the call to return regardless.
		let storage = default_storage::<DefaultDB>();
		let (tx, rx) = std::sync::mpsc::channel();
		std::thread::scope(|s| {
			let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
			s.spawn(move || {
				storage.with_backend(|_| {
					tx.send(()).unwrap();
					release_rx.recv().unwrap();
				});
			});
			rx.recv().unwrap();
			assert_eq!(release_to_zero(&[(w.clone(), 0)]), (0, Duration::ZERO));
			release_tx.send(()).unwrap();
		});
		flush();
		assert_eq!(
			tagged_pin_count(&arena(), &inner.hash()),
			Some(1),
			"count 0 unpersists nothing"
		);
		assert!(wrapper_is_root(&w));

		// Mixed list: the zero entry is skipped, the real one released in full.
		let other = persist_tagged(&arena(), tag(1008), &inner).hash().0.to_vec();
		flush();
		assert_eq!(release_to_zero(&[(w.clone(), 0), (other.clone(), 1)]).0, 1);
		flush();
		assert!(wrapper_is_root(&w));
		assert!(!wrapper_is_root(&other));
	}

	#[test]
	fn release_is_staged_until_flush() {
		let _g = serial();
		let inner = inner("gc-staged");
		let w = persist_tagged(&arena(), tag(1002), &inner).hash().0.to_vec();
		flush();

		let (n, _) = release_to_zero(&[(w.clone(), 1)]);
		assert_eq!(n, 1);
		// (Not asserting an exact length: other tests share the DefaultDB write cache.)
		assert!(write_cache_len().unwrap() > 0, "release must be staged, not written");
		flush();
		assert!(!wrapper_is_root(&w));
	}

	#[test]
	fn release_of_empty_list_takes_no_lock() {
		let _g = serial();
		// Hold the arena mutex on another thread: an empty release must return
		// immediately instead of blocking on it.
		let storage = default_storage::<DefaultDB>();
		let (tx, rx) = std::sync::mpsc::channel();
		std::thread::scope(|s| {
			let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
			s.spawn(move || {
				storage.with_backend(|_| {
					tx.send(()).unwrap();
					release_rx.recv().unwrap();
				});
			});
			rx.recv().unwrap();
			assert_eq!(release_to_zero(&[]), (0, Duration::ZERO));
			release_tx.send(()).unwrap();
		});
		// Read-freedom of the non-empty hold is structural (the closure only
		// calls `unpersist`); the storage crate exposes no read counter.
	}

	#[test]
	fn pin_roots_keeps_fresh_wrapper_through_gc_slice() {
		let _g = serial();
		let inner_hash;
		let w;
		{
			let inner = arena().alloc(FatLeaf(0x91));
			inner_hash = inner.hash();
			w = persist_tagged(&arena(), tag(1003), &inner).hash().0.to_vec();
		}
		flush();
		crate::ledger_9::clear_ledger_state_caches();

		let pins = pin_roots(std::slice::from_ref(&w));
		assert_eq!(pins.len(), 1);
		assert_eq!(pins[0].0, w);
		assert!(pin_roots(&[vec![7u8; 32], vec![1, 2, 3]]).is_empty());

		// Un-root the wrapper on disk; only the pin keeps it reachable.
		assert_eq!(release_to_zero(&[(w.clone(), 1)]).0, 1);
		flush();
		assert!(!wrapper_is_root(&w));
		let inner_present =
			|| default_storage::<DefaultDB>().with_backend(|b| b.get(&inner_hash).is_some());
		// Other (non-gc) tests share the arena and may leave the write cache dirty,
		// which makes a slice a no-op: flush and retry a bounded number of slices.
		let run_slices = |until_gone: bool| {
			for _ in 0..200 {
				flush();
				collect_garbage(Duration::from_secs(1)).expect("storage initialised");
				if until_gone && !inner_present() {
					return;
				}
			}
		};
		run_slices(false);
		assert!(inner_present(), "pinned wrapper must keep the inner state alive");

		drop(pins);
		run_slices(true);
		assert!(!inner_present(), "inner node must be gone after the pin is dropped");
	}

	/// storage-core forces a root rescan on every flush that changes a root count,
	/// and `gc()` marks every unmarked root before it resumes sweeping. So a wrapper
	/// persisted and flushed in the middle of a multi-slice sweep survives that
	/// sweep with no `Sp` (pin or keep-alive cache) holding it. The worker's pins
	/// are a defence on top of this; if this test fails after a storage-core bump,
	/// the pins are load-bearing.
	#[test]
	fn wrapper_flushed_mid_sweep_survives_without_pins() {
		let _g = serial();
		let present = |h: &ArenaHash<<DefaultDB as DB>::Hasher>| {
			default_storage::<DefaultDB>().with_backend(|b| b.get(h).is_some())
		};
		// Enough on-disk garbage that one call cannot sweep it all (the scan batch
		// is capped at 4096 nodes per budget check).
		let garbage: Vec<_> = (0..20_000u64)
			.map(|i| {
				let mut leaf = arena().alloc(FatLeaf(0x6000_0000 + i));
				leaf.persist();
				leaf.hash()
			})
			.collect();
		flush();
		default_storage::<DefaultDB>().with_backend(|b| {
			for h in &garbage {
				b.unpersist(h);
			}
		});
		flush();
		crate::ledger_9::clear_ledger_state_caches();
		let remaining = || garbage.iter().filter(|h| present(h)).count();
		assert_eq!(remaining(), garbage.len());

		// Drive the sweep until it has started culling but is not finished. The
		// mark phase needs enough budget to get through the roots other tests left
		// in the shared arena, so the budget grows until a call culls something.
		let mut budget = Duration::from_micros(200);
		let mut started = false;
		'outer: for _ in 0..12 {
			for _ in 0..200 {
				flush();
				let (culled, _) = collect_garbage(budget).expect("storage initialised");
				if culled > 0 {
					started = true;
					break 'outer;
				}
			}
			budget *= 2;
		}
		assert!(started, "the sweep never culled anything");
		let left = remaining();
		assert!(left > 0 && left < garbage.len(), "sweep must be mid-cycle, {left} left");

		// Mid-sweep: persist a new wrapper, flush it, and hold nothing.
		let inner_hash;
		let w;
		{
			let inner = arena().alloc(FatLeaf(0x61));
			inner_hash = inner.hash();
			w = persist_tagged(&arena(), tag(1010), &inner).hash();
		}
		flush();
		crate::ledger_9::clear_ledger_state_caches();

		// Finish this cycle and run a few more full slices.
		for _ in 0..2_000 {
			flush();
			collect_garbage(budget).expect("storage initialised");
			if remaining() == 0 {
				break;
			}
		}
		assert_eq!(remaining(), 0, "the garbage must be fully swept");
		for _ in 0..3 {
			flush();
			collect_garbage(Duration::from_secs(1)).expect("storage initialised");
		}
		assert!(present(&w), "wrapper flushed mid-sweep must not be culled");
		assert!(present(&inner_hash), "its inner state must not be culled");
		assert!(wrapper_is_root(&w.0));

		// Tidy up: release it so later tests see a smaller root set.
		assert_eq!(release_to_zero(&[(w.0.to_vec(), 1)]).0, 1);
		flush();
	}

	#[test]
	fn collect_garbage_skips_dirty_cache() {
		let _g = serial();
		let inner = inner("gc-dirty");
		let _w = persist_tagged(&arena(), tag(1004), &inner);
		assert!(write_cache_len().unwrap() > 0);
		assert_eq!(collect_garbage(Duration::from_millis(50)).unwrap().0, 0);
		flush();
		// With a clean cache the slice runs (it may legitimately cull nothing).
		assert!(collect_garbage(Duration::from_millis(50)).is_ok());
	}

	#[test]
	fn tagged_roots_reports_counts() {
		let _g = serial();
		let inner = inner("gc-counts");
		let a = persist_tagged(&arena(), tag(1005), &inner).hash().0.to_vec();
		let b = persist_tagged(&arena(), tag(1006), &inner).hash().0.to_vec();
		let _ = persist_tagged(&arena(), tag(1006), &inner);
		flush();

		let (roots, _hold) = tagged_roots();
		let ra: Vec<_> = roots.iter().filter(|r| r.wrapper == a).collect();
		let rb: Vec<_> = roots.iter().filter(|r| r.wrapper == b).collect();
		assert_eq!(ra.len(), 1);
		assert_eq!(ra[0].count, 1);
		assert_eq!(ra[0].tag, tag(1005));
		assert_eq!(rb.len(), 1);
		assert_eq!(rb[0].count, 2);
		assert_eq!(rb[0].tag, tag(1006));
		assert_ne!(a, b);
	}

	#[test]
	fn scan_stops_at_a_legacy_root() {
		let _g = serial();
		let leaf = arena().alloc(FatLeaf(0x51));
		let mut raw = leaf.clone();
		raw.persist();
		let w = persist_tagged(&arena(), tag(1009), &leaf).hash().0.to_vec();
		flush();
		let (scan, _hold) = scan();
		assert_eq!(scan, Err(LegacyRoot), "the raw persisted leaf is a legacy root");
		// `tagged_roots` keeps going past it.
		let (roots, _hold) = tagged_roots();
		assert!(roots.iter().any(|r| r.wrapper == w && r.tag == tag(1009)));
		assert!(!roots.iter().any(|r| r.wrapper == raw.hash().0.to_vec()));
	}

	#[test]
	fn classify_batches_and_separates_legacy() {
		let _g = serial();
		// 1500 hashes (two batches): every third is a raw persisted leaf (legacy), the
		// rest are wrappers; plus one malformed hash.
		let mut hashes = Vec::new();
		let mut expect = Vec::new();
		for i in 0..1500u32 {
			let leaf = arena().alloc(FatLeaf(0x7000_0000 + u64::from(i)));
			if i % 3 == 0 {
				let mut raw = leaf.clone();
				raw.persist();
				hashes.push(raw.hash().0.to_vec());
				expect.push(None);
			} else {
				let t = tag(2000 + i);
				hashes.push(persist_tagged(&arena(), t.clone(), &leaf).hash().0.to_vec());
				expect.push(Some(t));
			}
		}
		hashes.push(vec![1, 2, 3]);
		expect.push(None);
		flush();

		let got = classify(&hashes);
		assert_eq!(got.len(), hashes.len());
		for ((h, tag), (eh, et)) in got.iter().zip(hashes.iter().zip(&expect)) {
			assert_eq!(h, eh);
			assert_eq!(tag, et);
		}
		assert!(classify(&[]).is_empty());

		let (snap, _hold) = root_snapshot();
		for h in hashes.iter().take(1500) {
			assert!(snap.contains_key(h));
		}
	}
}

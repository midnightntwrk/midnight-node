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

//! GC-root tagging as ordinary content-addressed data.
//!
//! A [`TaggedRoot`] pins a value together with an opaque tag (e.g. a block
//! hash) by wrapping it in a regular DAG node and persisting the *wrapper* as
//! the GC root. The tag is part of the node's content, so it is staged,
//! flushed, and crash-consistent through the existing node and root-count
//! machinery: the tag and the persist that it describes commit in the same
//! flush by construction, and no dedicated metadata storage is involved.
//!
//! [`release_tagged`] later releases wrappers given only their tags — one
//! backend borrow, one scan of the root set — after which the GC collects
//! each wrapper and, transitively, its inner value, unless something else
//! still pins it.
//!
//! Because wrappers are content-addressed, distinct tags for the same inner
//! value are distinct roots: two blocks pinning the same ledger state each
//! hold their own wrapper, released independently. There is no
//! one-tag-per-root limitation.
//!
//! # Pruning usage
//!
//! - Tag known at persist time: `persist_tagged(&arena, tag, &sp)`.
//! - Tag known later (e.g. a block hash after the state is persisted): persist
//!   `sp` normally first; once the tag is known, [`swap_raw_pin_for_tagged`]
//!   stages `persist_tagged` plus `unpersist` together. Do not flush the shared
//!   write cache from the import watcher — the next `on_finalize` flush
//!   commits after that block's tip is rooted. A crash before that flush
//!   leaves the raw pin; restart re-tags genesis and best.
//! - Pruning: `release_tagged(&arena, &tags)`, then flush, then let the GC run.
//!
//! # Matching
//!
//! [`release_tagged`] and [`tagged_roots`] recognize wrapper nodes by a
//! payload magic prefix plus their single-child shape, without deserializing
//! arbitrary root types. A non-wrapper root whose payload happens to start
//! with the magic *and* has exactly one child *and* parses to a submitted tag
//! would be misidentified; the magic makes this practically impossible for
//! honest data, and roots are only ever created by trusted in-process code.

use super::ledger_storage_local::{
	Storable,
	arena::{Arena, ArenaHash, ArenaKey, Sp},
	backend::{OnDiskObject, StorageBackend},
	db::DB,
	storable::{Loader, WellBehavedHasher},
};
use super::midnight_serialize_local::{Deserializable, Serializable, Tagged};
use std::io::{Error, ErrorKind, Read, Write};

/// Payload prefix identifying wrapper nodes.
const MAGIC: [u8; 15] = *b"tagged-root[v1]";

/// Sanity cap on tag length when parsing candidate wrapper payloads.
const MAX_TAG_LEN: usize = 1 << 16;

/// Persist tag used for an anchored ledger tip: the block's header hash.
///
/// Distinct hashes are distinct wrappers, so sibling forks at the same height
/// are released independently. The hash is not known in `on_finalize` (it
/// includes the state root); callers persist the inner state first and swap
/// the raw pin for this tag once the block is imported — see
/// [`swap_raw_pin_for_tagged`].
pub fn persist_tag_from_block_hash(hash: &[u8]) -> std::vec::Vec<u8> {
	hash.to_vec()
}

/// A GC-root wrapper pinning `inner` together with an opaque `tag`.
///
/// See the [module docs](self).
pub struct TaggedRoot<T: Storable<D>, D: DB> {
	/// The opaque tag, e.g. a block hash from [`persist_tag_from_block_hash`].
	pub tag: std::vec::Vec<u8>,
	/// The pinned value.
	pub inner: Sp<T, D>,
}

impl<T: Storable<D>, D: DB> Clone for TaggedRoot<T, D> {
	fn clone(&self) -> Self {
		TaggedRoot { tag: self.tag.clone(), inner: self.inner.clone() }
	}
}

impl<T: Storable<D>, D: DB> std::fmt::Debug for TaggedRoot<T, D> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("TaggedRoot")
			.field("tag", &hex::encode(&self.tag))
			.finish_non_exhaustive()
	}
}

impl<T: Storable<D>, D: DB> Storable<D> for TaggedRoot<T, D> {
	fn children(&self) -> std::vec::Vec<ArenaKey<D::Hasher>> {
		vec![self.inner.as_child()]
	}

	fn to_binary_repr<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
		writer.write_all(&MAGIC)?;
		write_tag(&self.tag, writer)
	}

	fn from_binary_repr<R: Read>(
		reader: &mut R,
		child_nodes: &mut impl Iterator<Item = ArenaKey<D::Hasher>>,
		loader: &impl Loader<D>,
	) -> Result<Self, Error> {
		let mut magic = [0u8; MAGIC.len()];
		reader.read_exact(&mut magic)?;
		if magic != MAGIC {
			return Err(Error::new(ErrorKind::InvalidData, "not a tagged-root node"));
		}
		let tag = read_tag(reader)?;
		let inner = loader.get_next(child_nodes)?;
		loader.do_check(TaggedRoot { tag, inner })
	}
}

impl<T: Storable<D>, D: DB> Tagged for TaggedRoot<T, D> {
	fn tag() -> std::borrow::Cow<'static, str> {
		std::borrow::Cow::Borrowed("tagged-root[v1]")
	}
	fn tag_unique_factor() -> String {
		"tagged-root[v1]".into()
	}
}

fn write_tag<W: Write>(tag: &[u8], writer: &mut W) -> Result<(), Error> {
	let len =
		u32::try_from(tag.len()).map_err(|_| Error::new(ErrorKind::InvalidData, "tag too long"))?;
	writer.write_all(&len.to_le_bytes())?;
	writer.write_all(tag)
}

fn read_tag<R: Read>(reader: &mut R) -> Result<std::vec::Vec<u8>, Error> {
	let mut len = [0u8; 4];
	reader.read_exact(&mut len)?;
	let len = u32::from_le_bytes(len) as usize;
	if len > MAX_TAG_LEN {
		return Err(Error::new(ErrorKind::InvalidData, "tag too long"));
	}
	let mut tag = vec![0u8; len];
	reader.read_exact(&mut tag)?;
	Ok(tag)
}

/// Extract the wrapper payload via the public `OnDiskObject` serialization
/// (`data` is crate-private in storage-core).
fn payload_bytes<H: WellBehavedHasher>(obj: &OnDiskObject<H>) -> Option<std::vec::Vec<u8>> {
	let mut buf = std::vec::Vec::new();
	obj.serialize(&mut buf).ok()?;
	Vec::<u8>::deserialize(&mut &buf[..], 0).ok()
}

/// Parse `(tag, inner hash)` out of a raw node, if the node is a wrapper.
pub(crate) fn parse_wrapper<H: WellBehavedHasher>(
	obj: &OnDiskObject<H>,
) -> Option<(std::vec::Vec<u8>, ArenaHash<H>)> {
	if obj.children.len() != 1 {
		return None;
	}
	let rest = payload_bytes(obj)?.strip_prefix(&MAGIC[..])?.to_vec();
	let tag = read_tag(&mut rest.as_slice()).ok()?;
	Some((tag, obj.children[0].hash().clone()))
}

/// Persist `inner` pinned under `tag`, returning the wrapper.
///
/// The wrapper is the GC root; `inner` is pinned transitively for as long as
/// the wrapper is persisted. Calling this twice with the same `(tag, inner)`
/// yields the same content-addressed wrapper, incrementing its persist count.
pub fn persist_tagged<T: Storable<D>, D: DB>(
	arena: &Arena<D>,
	tag: std::vec::Vec<u8>,
	inner: &Sp<T, D>,
) -> Sp<TaggedRoot<T, D>, D> {
	let mut sp = arena.alloc(TaggedRoot { tag, inner: inner.clone() });
	sp.persist();
	sp
}

/// True if a wrapper with `tag` already pins `inner_hash`.
pub fn is_tagged<D: DB>(arena: &Arena<D>, tag: &[u8], inner_hash: &ArenaHash<D::Hasher>) -> bool {
	arena.with_backend(|backend| {
		for hash in backend.get_roots().into_keys() {
			let Some((existing, child)) = backend.get(&hash).and_then(parse_wrapper) else {
				continue;
			};
			if existing.as_slice() == tag && &child == inner_hash {
				return true;
			}
		}
		false
	})
}

/// Swap a raw `persist()` pin on `inner` for a tagged wrapper.
///
/// Used when the tag (block hash) is not known at persist time: `on_finalize`
/// persists `inner` as a GC root; after import, this stages `persist_tagged`
/// plus `unpersist` of the raw pin together. Idempotent if this `(tag, inner)`
/// wrapper already exists and the raw pin is gone. A second call with a
/// *different* tag (sibling fork sharing the same state) adds a second wrapper
/// and does not unpersist again.
///
/// Returns `true` if anything was staged (new wrapper and/or leftover raw pin
/// dropped). This does **not** flush: the shared write cache may hold
/// in-flight block allocations, and flushing it would empty the cache so the
/// arena sweep could run mid-execution. Durability is the next block-boundary
/// `flush_storage` (after that tip is rooted). A crash before that flush
/// leaves the raw pin on disk; restart re-tags genesis and best.
pub fn swap_raw_pin_for_tagged<T: Storable<D>, D: DB>(
	arena: &Arena<D>,
	tag: std::vec::Vec<u8>,
	inner: &Sp<T, D>,
) -> bool {
	let already = is_tagged(arena, &tag, &inner.hash());
	if !already {
		persist_tagged(arena, tag, inner);
	}
	let dropped_raw = arena.with_backend(|backend| {
		if backend.get_roots().contains_key(&inner.hash()) {
			backend.unpersist(&inner.hash());
			true
		} else {
			false
		}
	});
	!already || dropped_raw
}

/// Unpersist every tagged root whose tag equals one of `tags`, decrementing
/// each matching wrapper once. Returns the number of wrappers released.
///
/// The scan and the decrements run under a single backend borrow. Roots whose
/// persist count already reached zero are not listed as roots, so
/// re-submitting the same tags after a release is a safe no-op.
///
/// The count change is not durable until the caller flushes (same as
/// `unpersist`).
pub fn release_tagged<D: DB, M: AsRef<[u8]>>(arena: &Arena<D>, tags: &[M]) -> usize {
	if tags.is_empty() {
		return 0;
	}
	arena.with_backend(|backend| release_on_backend(backend, tags))
}

/// [`release_tagged`] plus a durability flush, only when the write cache is
/// empty so the flush writes isolated GC decrements.
///
/// A dirty cache returns `Err` without changing root counts; the caller
/// retries. Replay after a successful flush is a no-op (wrappers are gone
/// from the root set).
pub fn release_tagged_if_quiescent<D: DB, M: AsRef<[u8]>>(
	arena: &Arena<D>,
	tags: &[M],
) -> Result<usize, &'static str> {
	if tags.is_empty() {
		return Ok(0);
	}
	arena.with_backend(|backend| {
		if backend.get_write_cache_len() > 0 {
			return Err("ledger write cache busy; tip reclaim deferred");
		}
		let n = release_on_backend(backend, tags);
		backend.flush_all_changes_to_db();
		Ok(n)
	})
}

fn release_on_backend<D: DB, M: AsRef<[u8]>>(backend: &mut StorageBackend<D>, tags: &[M]) -> usize {
	let mut matched = std::vec::Vec::new();
	for hash in backend.get_roots().into_keys() {
		let Some((tag, _)) = backend.get(&hash).and_then(parse_wrapper) else {
			continue;
		};
		if tags.iter().any(|t| t.as_ref() == tag.as_slice()) {
			matched.push(hash);
		}
	}
	for hash in &matched {
		backend.unpersist(hash);
	}
	matched.len()
}

/// Enumerate all tagged roots, as `(wrapper hash, tag)` pairs.
pub fn tagged_roots<D: DB>(
	arena: &Arena<D>,
) -> std::vec::Vec<(ArenaHash<D::Hasher>, std::vec::Vec<u8>)> {
	arena.with_backend(|backend| {
		let mut result = std::vec::Vec::new();
		for hash in backend.get_roots().into_keys() {
			if let Some((tag, _)) = backend.get(&hash).and_then(parse_wrapper) {
				result.push((hash, tag));
			}
		}
		result
	})
}

/// Sum of persist counts of wrappers whose inner child is `inner_hash`.
///
/// `None` if no such wrapper is currently a GC root.
pub fn tagged_pin_count<D: DB>(arena: &Arena<D>, inner_hash: &ArenaHash<D::Hasher>) -> Option<u32> {
	arena.with_backend(|backend| {
		let mut total = 0u32;
		for (hash, count) in backend.get_roots() {
			let Some((_, child)) = backend.get(&hash).and_then(parse_wrapper) else {
				continue;
			};
			if &child == inner_hash {
				total = total.saturating_add(count);
			}
		}
		(total > 0).then_some(total)
	})
}

#[cfg(test)]
mod tests {
	use super::super::ledger_storage_local::{DefaultHasher, Storage, db::InMemoryDB};
	use super::*;

	type D = InMemoryDB<DefaultHasher>;

	/// A leaf large enough to be a real (non-inlined) node.
	#[derive(Clone)]
	struct FatLeaf(u64);

	impl<DBImpl: DB> Storable<DBImpl> for FatLeaf {
		fn children(&self) -> std::vec::Vec<ArenaKey<DBImpl::Hasher>> {
			std::vec::Vec::new()
		}

		fn to_binary_repr<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
			writer.write_all(&[0u8; 1024])?;
			writer.write_all(&self.0.to_le_bytes())
		}

		fn from_binary_repr<R: Read>(
			reader: &mut R,
			_child_nodes: &mut impl Iterator<Item = ArenaKey<DBImpl::Hasher>>,
			loader: &impl Loader<DBImpl>,
		) -> Result<Self, Error> {
			let mut pad = [0u8; 1024];
			reader.read_exact(&mut pad)?;
			let mut value = [0u8; 8];
			reader.read_exact(&mut value)?;
			loader.do_check(FatLeaf(u64::from_le_bytes(value)))
		}
	}

	fn new_arena() -> Arena<D> {
		Storage::new(64, D::default()).arena
	}

	fn leaf(arena: &Arena<D>, v: u64) -> Sp<FatLeaf, D> {
		arena.alloc(FatLeaf(v))
	}

	fn inner_root_count(arena: &Arena<D>, inner: &Sp<FatLeaf, D>) -> Option<u32> {
		arena.with_backend(|b| b.get_roots().get(&inner.hash()).copied())
	}

	#[test]
	fn persist_tagged_pins_wrapper_not_inner() {
		let arena = new_arena();
		let inner = leaf(&arena, 7);
		let tag = persist_tag_from_block_hash(&[1u8; 32]);
		let wrapper = persist_tagged(&arena, tag.clone(), &inner);

		assert_eq!(inner_root_count(&arena, &inner), None, "inner is not a raw GC root");
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));
		assert_eq!(tagged_roots(&arena), vec![(wrapper.hash(), tag)]);
	}

	#[test]
	fn release_tagged_decrements_once_and_is_idempotent() {
		let arena = new_arena();
		let inner = leaf(&arena, 1);
		let tag = vec![1u8; 4];
		let _w1 = persist_tagged(&arena, tag.clone(), &inner);
		let _w2 = persist_tagged(&arena, tag.clone(), &inner);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(2));

		assert_eq!(release_tagged(&arena, &[&tag]), 1);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));
		assert_eq!(release_tagged(&arena, &[&tag]), 1);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), None);
		assert_eq!(release_tagged(&arena, &[&tag]), 0);
	}

	#[test]
	fn shared_inner_survives_partial_release() {
		let arena = new_arena();
		let (t1, t2) = (vec![1u8; 32], vec![2u8; 32]);
		let inner = leaf(&arena, 42);
		let _w1 = persist_tagged(&arena, t1.clone(), &inner);
		let _w2 = persist_tagged(&arena, t2.clone(), &inner);
		assert_eq!(tagged_roots(&arena).len(), 2);

		assert_eq!(release_tagged(&arena, &[&t1]), 1);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));
		assert_eq!(tagged_roots(&arena).len(), 1);

		assert_eq!(release_tagged(&arena, &[&t2]), 1);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), None);
		assert_eq!(tagged_roots(&arena).len(), 0);
	}

	#[test]
	fn late_tagging_swap() {
		let arena = new_arena();
		let mut inner = leaf(&arena, 7);
		inner.persist();
		assert_eq!(inner_root_count(&arena, &inner), Some(1));

		let hash_a = persist_tag_from_block_hash(&[9u8; 32]);
		assert!(swap_raw_pin_for_tagged(&arena, hash_a.clone(), &inner));
		assert!(!swap_raw_pin_for_tagged(&arena, hash_a.clone(), &inner), "idempotent");
		assert_eq!(inner_root_count(&arena, &inner), None);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));

		// Sibling fork sharing the same inner state, different block hash:
		// a second wrapper, inner stays un-rooted as a raw persist.
		let hash_b = persist_tag_from_block_hash(&[8u8; 32]);
		assert!(swap_raw_pin_for_tagged(&arena, hash_b.clone(), &inner));
		assert_eq!(inner_root_count(&arena, &inner), None);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(2));

		assert_eq!(release_tagged(&arena, &[&hash_a]), 1);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));
		assert_eq!(release_tagged(&arena, &[&hash_b]), 1);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), None);
	}

	#[test]
	fn late_tagging_drops_leftover_raw_pin() {
		let arena = new_arena();
		let mut inner = leaf(&arena, 7);
		inner.persist();
		let tag = persist_tag_from_block_hash(&[9u8; 32]);
		let _wrapper = persist_tagged(&arena, tag.clone(), &inner);
		assert_eq!(inner_root_count(&arena, &inner), Some(1), "raw pin still present");

		assert!(
			swap_raw_pin_for_tagged(&arena, tag.clone(), &inner),
			"already-tagged but leftover raw pin is dropped"
		);
		assert_eq!(inner_root_count(&arena, &inner), None);
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));
		assert!(!swap_raw_pin_for_tagged(&arena, tag, &inner), "fully swapped is a no-op");
	}

	#[test]
	fn late_tagging_does_not_flush() {
		let arena = new_arena();
		let mut inner = leaf(&arena, 7);
		inner.persist();
		arena.with_backend(|b| {
			b.flush_all_changes_to_db();
			assert_eq!(b.get_write_cache_len(), 0);
		});

		let tag = persist_tag_from_block_hash(&[9u8; 32]);
		assert!(swap_raw_pin_for_tagged(&arena, tag, &inner));
		arena.with_backend(|b| {
			assert!(
				b.get_write_cache_len() > 0,
				"swap must leave isolated deltas in the cache; flushing is the block-boundary job"
			);
		});
	}
}

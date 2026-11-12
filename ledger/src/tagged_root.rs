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

//! GC-root tagging as ordinary content-addressed data.
//!
//! A [`TaggedRoot`] pins a value together with an opaque tag (here: the parent
//! block hash of the block that produced the value) by wrapping it in a regular
//! DAG node and persisting the *wrapper* as the GC root. The inner value is kept
//! alive transitively and is no longer a raw root, so the reason it is retained
//! lives in the data and not in the storage schema.
//!
//! Both ledger versions share one storage crate (`ledger_storage_ledger_8`) and
//! hence one arena, so this single module serves `ledger_8` and `ledger_9`.
//!
//! The on-disk payload format is frozen: `b"tagged-root[v1]" ++ u32_le(tag_len) ++ tag`,
//! with the pinned value as the single child.
//!
//! # Matching
//!
//! [`tagged_roots`] recognises wrapper nodes by the payload magic prefix plus
//! their single-child shape, without deserializing arbitrary root types. A
//! non-wrapper root whose payload happens to start with the magic *and* has
//! exactly one child would be misidentified; roots are only ever created by
//! trusted in-process code, so this is practically impossible.

use ledger_storage_ledger_8::{
	Storable,
	arena::{Arena, ArenaHash, ArenaKey, Sp},
	backend::OnDiskObject,
	db::DB,
	storable::{Loader, WellBehavedHasher},
};
use midnight_serialize::{Deserializable, Serializable, Tagged};
use std::{
	collections::HashMap,
	io::{Error, ErrorKind, Read, Write},
	time::{Duration, Instant},
};

/// Payload prefix identifying wrapper nodes.
const MAGIC: [u8; 15] = *b"tagged-root[v1]";

/// Sanity cap on tag length when parsing candidate wrapper payloads.
const MAX_TAG_LEN: usize = 1 << 16;

/// A GC-root wrapper pinning `inner` together with an opaque `tag`.
///
/// See the [module docs](self).
pub struct TaggedRoot<T: Storable<D>, D: DB> {
	/// The opaque tag, e.g. a parent block hash.
	pub tag: Vec<u8>,
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
	fn children(&self) -> Vec<ArenaKey<D::Hasher>> {
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

fn read_tag<R: Read>(reader: &mut R) -> Result<Vec<u8>, Error> {
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
fn payload_bytes<H: WellBehavedHasher>(obj: &OnDiskObject<H>) -> Option<Vec<u8>> {
	let mut buf = Vec::new();
	obj.serialize(&mut buf).ok()?;
	Vec::<u8>::deserialize(&mut &buf[..], 0).ok()
}

/// Parse `(tag, inner hash)` out of a raw node, if the node is a wrapper.
pub(crate) fn parse_wrapper<H: WellBehavedHasher>(
	obj: &OnDiskObject<H>,
) -> Option<(Vec<u8>, ArenaHash<H>)> {
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
	tag: Vec<u8>,
	inner: &Sp<T, D>,
) -> Sp<TaggedRoot<T, D>, D> {
	let mut sp = arena.alloc(TaggedRoot { tag, inner: inner.clone() });
	sp.persist();
	sp
}

/// Hash of the wrapper that [`persist_tagged`] would root for `(tag, inner)`,
/// without persisting anything.
pub fn wrapper_key<T: Storable<D>, D: DB>(
	arena: &Arena<D>,
	tag: Vec<u8>,
	inner: &Sp<T, D>,
) -> ArenaHash<D::Hasher> {
	arena.alloc(TaggedRoot { tag, inner: inner.clone() }).hash()
}

/// Enumerate all tagged roots as `(wrapper hash, tag, persist count)`:
/// [`root_snapshot`] then [`classify_batch`] in batches of at most 1000 hashes
/// (the arena mutex is released between batches).
///
/// Convenience helper for tests and tooling; the GC worker uses the primitives.
pub fn tagged_roots<D: DB>(arena: &Arena<D>) -> Vec<(ArenaHash<D::Hasher>, Vec<u8>, u32)> {
	let (snapshot, _) = root_snapshot(arena);
	let hashes: Vec<_> = snapshot.keys().cloned().collect();
	let mut wrappers = Vec::new();
	for batch in hashes.chunks(1000) {
		for (hash, tag) in classify_batch(arena, batch) {
			if let Some(tag) = tag {
				let count = snapshot[&hash];
				wrappers.push((hash, tag, count));
			}
		}
	}
	wrappers
}

/// Every GC root with its persist count, in ONE backend hold doing only
/// `get_roots()` (a full column iteration; no point reads). Also returns how long
/// the hold took (including the wait for the arena mutex).
pub fn root_snapshot<D: DB>(arena: &Arena<D>) -> (HashMap<ArenaHash<D::Hasher>, u32>, Duration) {
	let start = Instant::now();
	let roots = arena.with_backend(|backend| backend.get_roots());
	(roots, start.elapsed())
}

/// One classified root: its hash and `Some(tag)` if it is a wrapper.
pub type Classification<H> = (ArenaHash<H>, Option<Vec<u8>>);

/// Classify roots in ONE backend hold: one `get` plus [`parse_wrapper`] per hash.
/// `Some(tag)` = tagged-root wrapper, `None` = anything else (legacy raw root,
/// or a node that cannot be read). Callers batch (at most ~1000 hashes per call)
/// so the hold stays short.
pub fn classify_batch<D: DB>(
	arena: &Arena<D>,
	hashes: &[ArenaHash<D::Hasher>],
) -> Vec<Classification<D::Hasher>> {
	arena.with_backend(|backend| {
		hashes
			.iter()
			.map(|hash| {
				let tag = backend.get(hash).and_then(parse_wrapper).map(|(tag, _)| tag);
				(hash.clone(), tag)
			})
			.collect()
	})
}

/// Sum of persist counts of wrappers whose inner child is `inner_hash`.
///
/// `None` if no such wrapper is currently a GC root. Test/diagnostic helper
/// (scans all roots).
#[cfg(any(test, feature = "test-utils"))]
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
	use super::*;
	use ledger_storage_ledger_8::{DefaultHasher, Storage, db::InMemoryDB};

	type D = InMemoryDB<DefaultHasher>;

	/// A leaf large enough to be a real (non-inlined) node.
	#[derive(Clone)]
	struct FatLeaf(u64);

	impl<DBImpl: DB> Storable<DBImpl> for FatLeaf {
		fn children(&self) -> Vec<ArenaKey<DBImpl::Hasher>> {
			Vec::new()
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

	fn raw_root_count(arena: &Arena<D>, inner: &Sp<FatLeaf, D>) -> Option<u32> {
		arena.with_backend(|b| b.get_roots().get(&inner.hash()).copied())
	}

	fn tag(n: u8) -> Vec<u8> {
		vec![n; 32]
	}

	#[test]
	fn persist_tagged_pins_wrapper_not_inner() {
		let arena = new_arena();
		let inner = leaf(&arena, 7);
		let wrapper = persist_tagged(&arena, tag(1), &inner);

		assert_eq!(raw_root_count(&arena, &inner), None, "inner is not a raw GC root");
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(1));
		assert_eq!(tagged_roots(&arena), vec![(wrapper.hash(), tag(1), 1)]);
	}

	#[test]
	fn same_tag_and_inner_share_key_and_count() {
		let arena = new_arena();
		let inner = leaf(&arena, 1);
		let w1 = persist_tagged(&arena, tag(1), &inner);
		let w2 = persist_tagged(&arena, tag(1), &inner);
		assert_eq!(w1.hash(), w2.hash());
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(2));
		assert_eq!(tagged_roots(&arena), vec![(w1.hash(), tag(1), 2)]);
	}

	#[test]
	fn different_tag_gives_different_key() {
		let arena = new_arena();
		let inner = leaf(&arena, 1);
		let w1 = persist_tagged(&arena, tag(1), &inner);
		let w2 = persist_tagged(&arena, tag(2), &inner);
		assert_ne!(w1.hash(), w2.hash());
		assert_eq!(tagged_pin_count(&arena, &inner.hash()), Some(2));
		assert_eq!(tagged_roots(&arena).len(), 2);
	}

	#[test]
	fn wrapper_key_matches_persisted_wrapper_without_rooting() {
		let arena = new_arena();
		let inner = leaf(&arena, 9);
		let key = wrapper_key(&arena, tag(3), &inner);
		assert!(tagged_roots(&arena).is_empty(), "wrapper_key must not persist");
		let w = persist_tagged(&arena, tag(3), &inner);
		assert_eq!(key, w.hash());
	}

	#[test]
	fn wrapper_payload_format_is_frozen() {
		let arena = new_arena();
		let inner = leaf(&arena, 5);
		let tag = tag(0xab);
		let wrapper = persist_tagged(&arena, tag.clone(), &inner);

		let mut expected = b"tagged-root[v1]".to_vec();
		expected.extend_from_slice(&32u32.to_le_bytes());
		expected.extend_from_slice(&tag);

		let payload = arena.with_backend(|backend| {
			let obj = backend.get(&wrapper.hash()).expect("wrapper node present");
			assert_eq!(obj.children.len(), 1);
			assert_eq!(obj.children[0].hash(), &inner.hash());
			payload_bytes(obj).expect("payload")
		});
		assert_eq!(payload, expected);
		assert_eq!(<TaggedRoot<FatLeaf, D> as Tagged>::tag(), "tagged-root[v1]");
		assert_eq!(<TaggedRoot<FatLeaf, D> as Tagged>::tag_unique_factor(), "tagged-root[v1]");
	}

	#[test]
	fn tagged_roots_reports_counts_and_skips_non_wrappers() {
		let arena = new_arena();
		let a = leaf(&arena, 1);
		let b = leaf(&arena, 2);
		let mut plain = leaf(&arena, 3);
		plain.persist();
		let wa = persist_tagged(&arena, tag(1), &a);
		let _ = persist_tagged(&arena, tag(1), &a);
		let wb = persist_tagged(&arena, tag(2), &b);

		let mut roots = tagged_roots(&arena);
		roots.sort_by_key(|(_, t, _)| t.clone());
		assert_eq!(roots, vec![(wa.hash(), tag(1), 2), (wb.hash(), tag(2), 1)]);
		assert_eq!(raw_root_count(&arena, &plain), Some(1));
	}

	#[test]
	fn root_snapshot_counts() {
		let arena = new_arena();
		let a = leaf(&arena, 1);
		let b = leaf(&arena, 2);
		let wa = persist_tagged(&arena, tag(1), &a);
		let _ = persist_tagged(&arena, tag(1), &a);
		let wb = persist_tagged(&arena, tag(2), &b);
		let (snap, _hold) = root_snapshot(&arena);
		assert_eq!(snap.len(), 2);
		assert_eq!(snap[&wa.hash()], 2);
		assert_eq!(snap[&wb.hash()], 1);
	}

	#[test]
	fn classify_batch_distinguishes_wrappers_from_legacy() {
		let arena = new_arena();
		let a = leaf(&arena, 1);
		let mut plain = leaf(&arena, 3);
		plain.persist();
		let w = persist_tagged(&arena, tag(7), &a);
		let mut got = classify_batch(&arena, &[w.hash(), plain.hash()]);
		got.sort_by_key(|(_, t)| t.is_some());
		assert_eq!(got, vec![(plain.hash(), None), (w.hash(), Some(tag(7)))]);
	}
}

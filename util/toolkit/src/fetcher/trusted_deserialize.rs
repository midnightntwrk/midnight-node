// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
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

//! Trusted cache deserializer for `LedgerState`.
//!
//! Bypasses the multi-pass security verification in `Arena::deserialize_sp` that is
//! designed for untrusted wire input. Since our wallet cache is self-generated, we
//! skip re-hashing for verification and the re-serialization round-trip check.
//!
//! # Usage
//!
//! ```ignore
//! let state: LedgerState<DefaultDB> = trusted_deserialize_tagged(&cached_bytes)?;
//! ```

use midnight_node_ledger_helpers::{
	DefaultDB, Sp, Storable,
	mn_ledger_serialize::{Deserializable, GLOBAL_TAG, Serializable, Tagged},
	mn_ledger_storage::{
		arena::{ArenaHash, ArenaKey, TopoSortedNodes},
		db::DB,
		storable::{Loader, SMALL_OBJECT_LIMIT},
		storage::default_storage,
	},
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io};

/// Reimplements `child_from` from `midnight-storage-core/storable.rs`.
///
/// Must stay in sync with upstream's `child_from` + `is_in_small_object_limit`.
fn child_from_reimpl(data: &[u8], children: &[ArenaKey<Sha256>]) -> ArenaKey<Sha256> {
	let mut size = 2 + data.len();
	// Max 16 children, so this is O(1)
	for child in children {
		size += child.serialized_size();
		if size > SMALL_OBJECT_LIMIT {
			return ArenaKey::Ref(compute_hash(data, children.iter().map(|k| k.hash())));
		}
	}
	if size > SMALL_OBJECT_LIMIT {
		return ArenaKey::Ref(compute_hash(data, children.iter().map(|k| k.hash())));
	}

	// `DirectChildNode::new` is `pub(crate)`, so we construct via its
	// `Deserializable` impl (which calls `new` internally).
	let mut buf = Vec::new();
	data.to_vec().serialize(&mut buf).expect("serialize data for DirectChildNode");
	children
		.to_vec()
		.serialize(&mut buf)
		.expect("serialize children for DirectChildNode");
	ArenaKey::Direct(
		Deserializable::deserialize(&mut buf.as_slice(), 0).expect("deserialize DirectChildNode"),
	)
}

/// Reimplements the `pub(crate) hash()` from `midnight-storage-core/arena.rs`.
///
/// **Must stay in sync with upstream.** The algorithm is:
/// `SHA256(data.len() as u32 LE || data || child_hash_0 || child_hash_1 || ...)`
///
/// If upstream changes this hash function, trusted deserialization will silently
/// produce wrong `ArenaKey::Ref` hashes, causing "hash not found" errors on restore.
fn compute_hash<'a>(
	data: &[u8],
	child_hashes: impl Iterator<Item = &'a ArenaHash<Sha256>>,
) -> ArenaHash<Sha256> {
	let mut hasher = Sha256::default();
	hasher.update((data.len() as u32).to_le_bytes());
	hasher.update(data);
	for c in child_hashes {
		hasher.update(&c.0);
	}
	ArenaHash(hasher.finalize())
}

/// A Loader that trusts the input data, skipping invariant checks.
///
/// Used for reconstructing arena objects from our own cache where the data
/// has already been validated at serialization time.
struct TrustedCacheLoader<'a> {
	node_map: &'a HashMap<ArenaHash<Sha256>, (Vec<u8>, Vec<ArenaKey<Sha256>>)>,
}

impl Loader<DefaultDB> for TrustedCacheLoader<'_> {
	const CHECK_INVARIANTS: bool = false;

	fn get<T: Storable<DefaultDB>>(
		&self,
		key: &ArenaKey<<DefaultDB as DB>::Hasher>,
	) -> Result<Sp<T, DefaultDB>, io::Error> {
		match key {
			ArenaKey::Direct(node) => {
				let child_loader = TrustedCacheLoader { node_map: self.node_map };
				let value = T::from_binary_repr(
					&mut &node.data[..],
					&mut node.children.iter().cloned(),
					&child_loader,
				)?;
				Ok(default_storage::<DefaultDB>().arena.alloc(value))
			},
			ArenaKey::Ref(hash) => {
				let (data, children) = self.node_map.get(hash).ok_or_else(|| {
					io::Error::new(io::ErrorKind::NotFound, "hash not found in trusted cache")
				})?;
				let child_loader = TrustedCacheLoader { node_map: self.node_map };
				let value = T::from_binary_repr(
					&mut data.as_slice(),
					&mut children.iter().cloned(),
					&child_loader,
				)?;
				Ok(default_storage::<DefaultDB>().arena.alloc(value))
			},
		}
	}

	fn alloc<T: Storable<DefaultDB>>(&self, obj: T) -> Sp<T, DefaultDB> {
		default_storage::<DefaultDB>().arena.alloc(obj)
	}

	fn get_recursion_depth(&self) -> u32 {
		0
	}
}

/// Deserialize a tagged `Storable` type from bytes, trusting the data integrity.
///
/// This is functionally equivalent to `midnight_node_ledger_helpers::deserialize` but
/// performs a single hash pass instead of two, and skips the re-serialization verification.
pub fn trusted_deserialize_tagged<T: Storable<DefaultDB> + Deserializable + Tagged>(
	bytes: &[u8],
) -> Result<T, io::Error> {
	let start = std::time::Instant::now();

	// Step 1: Strip tag prefix (format: "midnight:<tag>:")
	let tag_prefix = format!("{GLOBAL_TAG}{}:", T::tag());
	if bytes.len() < tag_prefix.len() || &bytes[..tag_prefix.len()] != tag_prefix.as_bytes() {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			format!(
				"tag mismatch: expected prefix '{}', got '{}'",
				tag_prefix,
				String::from_utf8_lossy(&bytes[..tag_prefix.len().min(bytes.len())])
			),
		));
	}
	let mut reader = &bytes[tag_prefix.len()..];

	// Step 2: Parse TopoSortedNodes (the serialized arena graph)
	let nodes: TopoSortedNodes = Deserializable::deserialize(&mut reader, 0)?;
	log::debug!("Trusted deserialize: parsed {} nodes in {:?}", nodes.nodes.len(), start.elapsed());

	// Step 3: Single-pass bottom-up ArenaKey computation + node_map construction.
	// TopoSortedNodes are ordered so children precede parents, meaning we can
	// resolve all keys in one forward pass (mirrors IrLoader's key_to_child_repr).
	//
	// TODO: Remove the workaround described below after we start using fixed ledger
	// (PR: https://github.com/midnightntwrk/midnight-ledger/pull/230)
	// Each child key uses the correct ArenaKey variant (Direct vs Ref) matching
	// what `child_from` in midnight-storage-core would produce. This is critical:
	// `serialize_to_node_list_bounded` uses ArenaKey (not ArenaHash) as a map key
	// for incoming_vertices. If the same logical node appears as both Ref(h) and
	// Direct(d) with d.hash == h, the topological sort panics.
	let mut node_repr: Vec<ArenaKey<Sha256>> = Vec::with_capacity(nodes.nodes.len());
	let mut node_map: HashMap<ArenaHash<Sha256>, (Vec<u8>, Vec<ArenaKey<Sha256>>)> =
		HashMap::with_capacity(nodes.nodes.len());

	for node in &nodes.nodes {
		let child_keys: Vec<ArenaKey<Sha256>> =
			node.child_indices.iter().map(|&i| node_repr[i as usize].clone()).collect();

		let repr = child_from_reimpl(&node.data, &child_keys);
		node_map.insert(repr.hash().clone(), (node.data.clone(), child_keys));
		node_repr.push(repr);
	}

	log::debug!("Trusted deserialize: hashed {} nodes in {:?}", node_repr.len(), start.elapsed());

	// Step 4: Reconstruct root using TrustedCacheLoader
	let root_hash = node_repr
		.last()
		.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty node list"))?
		.hash()
		.clone();
	let root_key = ArenaKey::Ref(root_hash);

	let loader = TrustedCacheLoader { node_map: &node_map };
	let sp: Sp<T, DefaultDB> = loader.get(&root_key)?;

	log::info!(
		"Trusted deserialize: complete in {:?} ({} nodes)",
		start.elapsed(),
		nodes.nodes.len()
	);

	Ok((*sp).clone())
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_ledger_helpers::{LedgerContext, LedgerState};

	fn load_genesis_context() -> LedgerContext<DefaultDB> {
		let genesis_path =
			format!("{}/test-data/genesis/genesis_block_undeployed.mn", env!("CARGO_MANIFEST_DIR"));
		let batches =
			crate::tx_generator::source::GetTxsFromFile::load_single_or_multiple(&genesis_path)
				.expect("failed to load genesis file");
		let source =
			crate::serde_def::SourceTransactions::from_batches(batches.batches, true, None);
		crate::tx_generator::builder::build_fork_aware_context(&source, &[])
			.expect("failed to build context")
	}

	fn assert_child_from_matches<T: Storable<DefaultDB>>(value: &T, label: &str) -> ArenaKey {
		let upstream = Storable::<DefaultDB>::as_child(value);

		let mut data = Vec::new();
		value.to_binary_repr(&mut data).unwrap();
		let children = value.children();
		let ours = child_from_reimpl(&data, &children);

		assert_eq!(upstream, ours, "{label}: child_from_reimpl diverges from upstream");
		ours
	}

	fn assert_child_from_matches_as_direct<T: Storable<DefaultDB>>(value: &T, label: &str) {
		let arena_key = assert_child_from_matches(value, label);
		assert!(matches!(arena_key, ArenaKey::Direct(_)));
	}

	#[test]
	fn trusted_deserialize_roundtrip() {
		let context = load_genesis_context();

		let ledger_state = context.ledger_state.lock().unwrap();
		let original_bytes =
			midnight_node_ledger_helpers::serialize(&*ledger_state).expect("serialize failed");
		drop(ledger_state);

		let restored: LedgerState<DefaultDB> =
			trusted_deserialize_tagged(&original_bytes).expect("trusted deserialize failed");

		let roundtrip_bytes =
			midnight_node_ledger_helpers::serialize(&restored).expect("re-serialize failed");

		assert_eq!(
			original_bytes,
			roundtrip_bytes,
			"roundtrip bytes differ: original {} bytes vs roundtrip {} bytes",
			original_bytes.len(),
			roundtrip_bytes.len()
		);
	}

	/// Verify trusted deserialization produces identical state to the standard
	/// (upstream) deserializer. If upstream changes their hash function or
	/// serialization format, this test fails immediately in CI.
	#[test]
	fn trusted_deser_matches_upstream() {
		let state = LedgerState::<DefaultDB>::new("test");
		let bytes = midnight_node_ledger_helpers::serialize(&state).expect("serialize failed");

		let standard: LedgerState<DefaultDB> =
			midnight_node_ledger_helpers::deserialize(&bytes[..])
				.expect("standard deserialize failed");
		let trusted: LedgerState<DefaultDB> =
			trusted_deserialize_tagged(&bytes).expect("trusted deserialize failed");

		let standard_bytes = midnight_node_ledger_helpers::serialize(&standard)
			.expect("re-serialize standard failed");
		let trusted_bytes =
			midnight_node_ledger_helpers::serialize(&trusted).expect("re-serialize trusted failed");

		assert_eq!(
			standard_bytes,
			trusted_bytes,
			"trusted and standard deserialization produce different state \
			 (standard {} bytes vs trusted {} bytes)",
			standard_bytes.len(),
			trusted_bytes.len()
		);
	}

	#[test]
	fn child_from_reimpl_fixed_size_types() {
		assert_child_from_matches_as_direct(&0u8, "u8(0)");
		assert_child_from_matches_as_direct(&255u8, "u8(255)");
		assert_child_from_matches_as_direct(&u32::MAX, "u32::MAX");
		assert_child_from_matches_as_direct(&u64::MAX, "u64::MAX");
	}

	// Sweep String lengths: serializes with a length prefix, so the
	// serialized data crosses SMALL_OBJECT_LIMIT somewhere in this range, so we should catch
	// if the formula changes.
	#[test]
	fn child_from_reimpl_boundary() {
		let start = 950;
		let end = 1130;
		for n in start..=end {
			let arena_key = assert_child_from_matches(&"a".repeat(n), &format!("String len={n}"));
			if n == start {
				assert!(matches!(arena_key, ArenaKey::Direct(_)))
			}
			if n == end {
				assert!(matches!(arena_key, ArenaKey::Ref(_)))
			}
		}
	}

	/// Test with tuple types that exercise the children path in child_from_reimpl.
	#[test]
	fn child_from_reimpl_with_children() {
		let arena = &default_storage::<DefaultDB>().arena;

		let small_sp = arena.alloc(42u8);
		assert_child_from_matches(&(small_sp,), "tuple(u8)");

		let large_sp = arena.alloc([0u8; SMALL_OBJECT_LIMIT]);
		assert_child_from_matches(&(large_sp,), "tuple([u8;1024])");

		let sp_a = arena.alloc(1u32);
		let sp_b = arena.alloc(2u32);
		assert_child_from_matches(&(sp_a, sp_b), "tuple(u32,u32)");
	}

	/// Test with LedgerState (always Ref, validates hash computation with
	/// real children from an actual ledger type).
	#[test]
	fn child_from_reimpl_ledger_state() {
		let state = LedgerState::<DefaultDB>::new("test");
		assert_child_from_matches(&state, "LedgerState::new");

		let context = load_genesis_context();
		let state = context.ledger_state.lock().unwrap();
		assert_child_from_matches(&*state, "genesis LedgerState");
	}
}

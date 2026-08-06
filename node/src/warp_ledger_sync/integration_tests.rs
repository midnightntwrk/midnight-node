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

//! In-process round-trip test for the warp ledger-sync core, with no networking:
//! init a real arena from genesis → serialize the `Ledger`-rooted snapshot (server) → compress +
//! page it through the transport chunker + reassembler → decompress → import + verify against the on-chain
//! `StateKey` (client/import). Also asserts the security property: a tampered blob is
//! rejected (`RootMismatch`), never imported.
//!
//! Run isolated (it touches the process-global `default_storage` singleton):
//! `cargo test -p midnight-node ledger_snapshot_roundtrip`.

use super::protocol::{ChunkAssembler, build_response, compress_snapshot, decompress_snapshot};

/// Compress + page `blob` end-to-end the way the client would, with a deliberately small chunk size
/// to force multiple ranges, and return the decompressed reassembled bytes.
fn compress_page_reassemble_decompress(blob: &[u8], chunk: u32) -> Vec<u8> {
	let compressed = compress_snapshot(blob).expect("compress snapshot");
	let raw_total_len = blob.len() as u64;
	let mut assembler = ChunkAssembler::new(compressed.len() as u64);
	loop {
		let response = build_response(&compressed, raw_total_len, assembler.next_offset(), chunk);
		assert_eq!(response.raw_total_len, raw_total_len);
		assert_eq!(response.compressed_total_len, compressed.len() as u64);
		if response.bytes.is_empty() {
			break;
		}
		assembler.accept(response.offset, &response.bytes).expect("contiguous chunk");
	}
	let reassembled = assembler.into_blob().expect("complete compressed blob");
	decompress_snapshot(&reassembled, raw_total_len).expect("decompress snapshot")
}

#[test]
fn ledger_snapshot_roundtrip_serialize_chunk_verify_import() {
	let dir = tempfile::tempdir().expect("tempdir");
	// Use a bundled v13 fixture to exercise the `ledger_8` dispatch path; the local undeployed
	// fixture is v18 (`ledger_9`).
	let genesis_state = include_bytes!("../../../res/genesis/genesis_state_preview.mn");

	// Initialize the arena from genesis in Separate mode. This sets the process-global
	// `default_storage`, persists the genesis ledger, and returns the on-chain `StateKey` bytes
	// (the tagged `TypedArenaKey<Ledger>`) — exactly what `pallet_midnight::StateKey` would hold.
	let state_key = midnight_node_ledger::ledger_8::storage::init_storage_paritydb_separate(
		dir.path(),
		genesis_state,
		1024,
	);
	assert!(!state_key.is_empty(), "genesis init must produce a StateKey");

	// Server side: serialize the `Ledger`-rooted snapshot at that StateKey.
	let blob = midnight_node_ledger::serialize_ledger_snapshot(false, &state_key)
		.expect("serialize ledger snapshot");
	assert!(blob.len() > state_key.len(), "snapshot blob should carry the arena, not just the key");

	// Transport: compress, page into 4 KiB ranges, reassemble, and decompress; must return the
	// canonical blob byte-identically.
	let reassembled = compress_page_reassemble_decompress(&blob, 4096);
	assert_eq!(reassembled, blob, "reassembled blob must be byte-identical to the server's");

	// Client/import: verify root == StateKey and persist as a wrapper tagged with the warp
	// target number (committed in the same flush). Idempotent against the already-initialized
	// arena (content-addressed; a leftover raw genesis pin is dropped).
	let warp_number = 42u32;
	let warp_tag = warp_number.to_le_bytes();
	midnight_node_ledger::import_verified_ledger_snapshot(
		false,
		&reassembled,
		&state_key,
		warp_number,
	)
	.expect("verified import of a faithful snapshot should succeed");
	assert!(
		midnight_node_ledger::gc::tagged_root_tags().iter().any(|t| t.as_slice() == warp_tag),
		"warp-point persist must be a tagged wrapper so GC can reclaim it"
	);

	// Security property: tamper a byte well past the tag prefix (in the node-data region). The
	// native multi-pass deserializer / root check must reject it — never a successful import.
	let mut tampered = blob.clone();
	let idx = tampered.len() / 2;
	tampered[idx] ^= 0xFF;
	let result = midnight_node_ledger::import_verified_ledger_snapshot(
		false,
		&tampered,
		&state_key,
		warp_number,
	);
	assert!(
		result.is_err(),
		"a tampered snapshot must fail verification and not be imported, got {result:?}"
	);
}

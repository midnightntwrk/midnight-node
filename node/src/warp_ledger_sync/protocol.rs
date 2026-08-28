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

//! Ledger-sync protocol message types, codec, naming, and the pure range-serving /
//! reassembly logic shared by the server and client.
//!
//! The transferred payload is the Snappy-compressed canonical, `Ledger`-rooted arena blob (derived
//! tag prefix ‖ `TopoSortedNodes` of the `Ledger` DAG). Transport pages the compressed stream by
//! **byte offset** (not by semantic node): the server streams contiguous byte ranges and the client
//! concatenates them in order, decompresses to the canonical blob, then deserialize + verifies. The
//! children-precede-parents property is intrinsic to the decompressed blob.

use parity_scale_codec::{Decode, Encode};

/// Protocol name suffix; the full name is `/{genesis_hash}[/{fork_id}]/midnight-ledger-sync/2`.
///
/// Version 2 range-serves a Snappy-compressed ledger snapshot. Version 1 served raw canonical
/// snapshot bytes.
pub const PROTOCOL_NAME_SUFFIX: &str = "midnight-ledger-sync/2";

/// Maximum number of bytes a single response chunk may carry. The server clamps a peer's
/// requested `max_len` to this; the network layer's `max_response_size` must be ≥ this plus codec
/// overhead. 1 MiB matches substrate's state-sync chunking.
pub const MAX_LEDGER_SYNC_CHUNK: u32 = 1024 * 1024;

/// Maximum decompressed ledger snapshot size accepted from a peer.
///
/// This is intentionally generous relative to current expected arena sizes, but it prevents a
/// malicious peer from advertising a tiny compressed payload that expands into an unreasonable
/// allocation before ledger-root verification can reject it.
pub const MAX_LEDGER_SYNC_RAW_BYTES: u64 = 1024 * 1024 * 1024;

/// Request a contiguous byte range of the `Ledger`-rooted arena blob at `target_hash`.
///
/// `target_hash` must be a finalized block whose state-sync target the server can serve (the
/// server rejects non-finalized / unknown blocks). `offset`/`max_len` page the blob; `max_len` is clamped
/// server-side to [`MAX_LEDGER_SYNC_CHUNK`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LedgerSyncRequest<Hash> {
	/// Finalized target block whose arena snapshot is requested.
	pub target_hash: Hash,
	/// Byte offset into the canonical blob to start from.
	pub offset: u64,
	/// Maximum number of bytes to return (clamped to [`MAX_LEDGER_SYNC_CHUNK`] by the server).
	pub max_len: u32,
}

/// A contiguous byte range of the Snappy-compressed canonical `Ledger`-rooted blob.
///
/// `compressed_total_len` is the full compressed stream size (lets the client learn the size up
/// front and drive parallel / resumable range fetches); `raw_total_len` is the expected
/// decompressed canonical blob size, used to bound and validate decompression; `offset`/`bytes` are
/// this compressed chunk.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LedgerSyncResponse {
	/// Total length of the full compressed blob at the target block.
	pub compressed_total_len: u64,
	/// Expected total length after Snappy decompression.
	pub raw_total_len: u64,
	/// Byte offset of this chunk within the compressed blob.
	pub offset: u64,
	/// The chunk bytes: `compressed_blob[offset .. offset + bytes.len()]`.
	pub bytes: Vec<u8>,
}

/// Build the full ledger-sync protocol name from a genesis hash and optional fork id, mirroring
/// substrate's `/{hex_genesis}[/{fork}]/state/2` convention.
pub fn ledger_sync_protocol_name<Hash: AsRef<[u8]>>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
) -> String {
	let genesis = hex::encode(genesis_hash.as_ref());
	match fork_id {
		Some(fork) => format!("/{genesis}/{fork}/{PROTOCOL_NAME_SUFFIX}"),
		None => format!("/{genesis}/{PROTOCOL_NAME_SUFFIX}"),
	}
}

/// Clamp a peer-requested `max_len` to the server limit.
pub fn clamp_max_len(requested: u32) -> u32 {
	requested.min(MAX_LEDGER_SYNC_CHUNK)
}

/// The chunk length the client requires at `offset` of a blob of `compressed_total_len` bytes,
/// given it always requests [`MAX_LEDGER_SYNC_CHUNK`]: an honest server ([`build_response`]) fills
/// every chunk to the end of the requested range, so anything shorter is a stalling/misbehaving
/// peer. Enforcing this caps the requests spent on one peer at
/// `ceil(compressed_total_len / MAX_LEDGER_SYNC_CHUNK)` — a peer serving valid-but-tiny chunks
/// can't tie the client up in an unbounded request loop.
pub fn required_chunk_len(compressed_total_len: u64, offset: u64) -> u64 {
	compressed_total_len.saturating_sub(offset).min(MAX_LEDGER_SYNC_CHUNK as u64)
}

/// Snappy-compress a canonical snapshot blob for transport.
pub fn compress_snapshot(blob: &[u8]) -> Result<Vec<u8>, snap::Error> {
	snap::raw::Encoder::new().compress_vec(blob)
}

/// Decompress a complete Snappy-compressed snapshot blob after range reassembly.
pub fn decompress_snapshot(
	compressed: &[u8],
	expected_raw_len: u64,
) -> Result<Vec<u8>, DecompressError> {
	validate_snapshot_lengths(compressed.len() as u64, expected_raw_len)?;
	let advertised = snap::raw::decompress_len(compressed).map_err(DecompressError::Snap)? as u64;
	if advertised != expected_raw_len {
		return Err(DecompressError::LengthMismatch {
			expected: expected_raw_len,
			actual: advertised,
		});
	}
	let bytes = snap::raw::Decoder::new()
		.decompress_vec(compressed)
		.map_err(DecompressError::Snap)?;
	if bytes.len() as u64 != expected_raw_len {
		return Err(DecompressError::LengthMismatch {
			expected: expected_raw_len,
			actual: bytes.len() as u64,
		});
	}
	Ok(bytes)
}

/// Validate advertised compressed/raw snapshot lengths before allocating or downloading the full
/// compressed stream.
pub fn validate_snapshot_lengths(
	compressed_total_len: u64,
	raw_total_len: u64,
) -> Result<(), DecompressError> {
	if raw_total_len > MAX_LEDGER_SYNC_RAW_BYTES {
		return Err(DecompressError::TooLarge { len: raw_total_len });
	}
	let max_compressed_len = snap::raw::max_compress_len(raw_total_len as usize) as u64;
	if compressed_total_len > max_compressed_len {
		return Err(DecompressError::CompressedTooLarge {
			len: compressed_total_len,
			max: max_compressed_len,
		});
	}
	Ok(())
}

/// Errors from decompressing a complete compressed snapshot.
#[derive(Debug, thiserror::Error)]
pub enum DecompressError {
	/// Peer advertised a decompressed length larger than this protocol accepts.
	#[error("decompressed snapshot length {len} exceeds limit {MAX_LEDGER_SYNC_RAW_BYTES}")]
	TooLarge {
		/// Advertised decompressed size.
		len: u64,
	},
	/// Peer advertised a compressed length larger than Snappy can emit for the raw length.
	#[error("compressed snapshot length {len} exceeds snappy limit {max}")]
	CompressedTooLarge {
		/// Advertised compressed size.
		len: u64,
		/// Maximum possible Snappy raw compressed size for the advertised raw size.
		max: u64,
	},
	/// The response metadata and Snappy stream header disagreed on decompressed length.
	#[error("decompressed snapshot length mismatch: expected {expected}, got {actual}")]
	LengthMismatch {
		/// Expected decompressed size from response metadata.
		expected: u64,
		/// Actual decompressed size from the Snappy header/result.
		actual: u64,
	},
	/// Snappy rejected the compressed stream.
	#[error("snappy decompression failed: {0}")]
	Snap(#[from] snap::Error),
}

/// Build a response chunk for `[offset, offset + clamp(max_len))` of `compressed_blob` (server side).
///
/// Clamps `max_len`, never reads past the end of the blob, and yields an empty chunk if `offset`
/// is at or past the end (which signals completion to the client).
pub fn build_response(
	compressed_blob: &[u8],
	raw_total_len: u64,
	offset: u64,
	max_len: u32,
) -> LedgerSyncResponse {
	let compressed_total_len = compressed_blob.len() as u64;
	let start = offset.min(compressed_total_len);
	let avail = compressed_total_len - start;
	let len = (clamp_max_len(max_len) as u64).min(avail);
	let start = start as usize;
	let end = start + len as usize;
	LedgerSyncResponse {
		compressed_total_len,
		raw_total_len,
		offset,
		bytes: compressed_blob[start..end].to_vec(),
	}
}

/// Errors from reassembling response chunks into the full blob (client side).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssembleError {
	/// A chunk did not start where the previous one ended. Chunks must be fed in order; parallel
	/// fetches must be reordered by `offset` before being accepted.
	#[error("non-contiguous chunk: expected offset {expected}, got {got}")]
	NonContiguous {
		/// The offset the assembler expected next (its current filled length).
		expected: u64,
		/// The offset the chunk actually carried.
		got: u64,
	},
	/// A chunk would extend the blob past the advertised compressed `total_len`.
	#[error("chunk overflows total_len {total}: offset {offset} + len {len}")]
	Overflow {
		/// Advertised total length of the blob.
		total: u64,
		/// Offset of the offending chunk.
		offset: u64,
		/// Length of the offending chunk.
		len: u64,
	},
	/// `into_blob` was called before all bytes were received.
	#[error("incomplete blob: have {have} of {total} bytes")]
	Incomplete {
		/// Bytes received so far.
		have: u64,
		/// Total bytes expected.
		total: u64,
	},
}

/// Reassembles ordered, contiguous response chunks into the full compressed blob.
///
/// In-order contiguous assembly is sufficient and simplest: a chunk is accepted only if its
/// `offset` equals the bytes received so far. Parallel / multi-peer fetches are allowed but the
/// client must reorder chunks by `offset` before feeding them here. The assembled
/// decompressed blob is verified against the on-chain `StateKey` by the client driver — this type
/// does no crypto, only transport-level reassembly.
#[derive(Debug)]
pub struct ChunkAssembler {
	total_len: u64,
	// Grown incrementally rather than pre-allocated to `total_len`, so a malicious peer
	// advertising a huge `total_len` cannot force a large up-front allocation.
	buf: Vec<u8>,
}

impl ChunkAssembler {
	/// Start assembling a blob of `total_len` bytes (learned from the first response).
	pub fn new(total_len: u64) -> Self {
		Self { total_len, buf: Vec::new() }
	}

	/// The offset the next chunk must start at (the number of bytes received so far). Use this to
	/// drive the next [`LedgerSyncRequest`] and to support resume after interruption.
	pub fn next_offset(&self) -> u64 {
		self.buf.len() as u64
	}

	/// Accept the next contiguous chunk. Returns an error (and leaves the assembler unchanged) if
	/// the chunk is out of order or would overflow `total_len`.
	pub fn accept(&mut self, offset: u64, bytes: &[u8]) -> Result<(), AssembleError> {
		let expected = self.buf.len() as u64;
		if offset != expected {
			return Err(AssembleError::NonContiguous { expected, got: offset });
		}
		if offset + bytes.len() as u64 > self.total_len {
			return Err(AssembleError::Overflow {
				total: self.total_len,
				offset,
				len: bytes.len() as u64,
			});
		}
		self.buf.extend_from_slice(bytes);
		Ok(())
	}

	/// Whether all `total_len` bytes have been received.
	pub fn is_complete(&self) -> bool {
		self.buf.len() as u64 == self.total_len
	}

	/// Consume the assembler and return the full blob, or [`AssembleError::Incomplete`] if bytes
	/// are still missing.
	pub fn into_blob(self) -> Result<Vec<u8>, AssembleError> {
		if !self.is_complete() {
			return Err(AssembleError::Incomplete {
				have: self.buf.len() as u64,
				total: self.total_len,
			});
		}
		Ok(self.buf)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;

	#[test]
	fn request_response_scale_roundtrip() {
		let req =
			LedgerSyncRequest { target_hash: H256::repeat_byte(0xab), offset: 4096, max_len: 1234 };
		let decoded = LedgerSyncRequest::<H256>::decode(&mut &req.encode()[..]).unwrap();
		assert_eq!(req, decoded);

		let resp = LedgerSyncResponse {
			compressed_total_len: 9_999,
			raw_total_len: 20_000,
			offset: 4096,
			bytes: vec![1, 2, 3, 4, 5],
		};
		let decoded = LedgerSyncResponse::decode(&mut &resp.encode()[..]).unwrap();
		assert_eq!(resp, decoded);
	}

	#[test]
	fn protocol_name_with_and_without_fork() {
		let genesis = H256::repeat_byte(0x01);
		let hex = hex::encode(genesis.as_ref());
		assert_eq!(
			ledger_sync_protocol_name(genesis, None),
			format!("/{hex}/midnight-ledger-sync/2")
		);
		assert_eq!(
			ledger_sync_protocol_name(genesis, Some("forkz")),
			format!("/{hex}/forkz/midnight-ledger-sync/2")
		);
	}

	#[test]
	fn clamp_respects_limit() {
		assert_eq!(clamp_max_len(10), 10);
		assert_eq!(clamp_max_len(MAX_LEDGER_SYNC_CHUNK + 1), MAX_LEDGER_SYNC_CHUNK);
		assert_eq!(clamp_max_len(u32::MAX), MAX_LEDGER_SYNC_CHUNK);
	}

	#[test]
	fn required_chunk_len_matches_honest_server() {
		let max = MAX_LEDGER_SYNC_CHUNK as u64;
		// Interior chunks must be full-size; the final chunk is the remainder.
		assert_eq!(required_chunk_len(3 * max + 100, 0), max);
		assert_eq!(required_chunk_len(3 * max + 100, 3 * max), 100);
		// At/past the end (and for an empty blob) nothing is required.
		assert_eq!(required_chunk_len(3 * max + 100, 3 * max + 100), 0);
		assert_eq!(required_chunk_len(100, 200), 0);
		assert_eq!(required_chunk_len(0, 0), 0);

		// What build_response produces is exactly what the client requires, at every offset.
		let blob: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
		for offset in [0u64, 1000, 4999, 5000] {
			let r = build_response(&blob, 10_000, offset, MAX_LEDGER_SYNC_CHUNK);
			assert_eq!(r.bytes.len() as u64, required_chunk_len(blob.len() as u64, offset));
		}
	}

	#[test]
	fn build_response_clamps_and_bounds() {
		let blob: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
		let raw_total_len = 10_000;

		// A normal interior range.
		let r = build_response(&blob, raw_total_len, 1000, 500);
		assert_eq!(r.compressed_total_len, 5000);
		assert_eq!(r.raw_total_len, raw_total_len);
		assert_eq!(r.offset, 1000);
		assert_eq!(r.bytes, &blob[1000..1500]);

		// max_len past the end is truncated to the tail.
		let r = build_response(&blob, raw_total_len, 4800, 1000);
		assert_eq!(r.bytes, &blob[4800..5000]);

		// offset at/past the end yields an empty chunk (completion signal).
		let r = build_response(&blob, raw_total_len, 5000, 100);
		assert!(r.bytes.is_empty());
		assert_eq!(r.compressed_total_len, 5000);
		let r = build_response(&blob, raw_total_len, 9999, 100);
		assert!(r.bytes.is_empty());
	}

	#[test]
	fn assembler_reassembles_byte_identical() {
		let blob: Vec<u8> = (0..=255u8).cycle().take(5000).collect();

		// Page the blob the way the client would: repeated build_response calls following
		// `next_offset`, fed into the assembler in order.
		let mut asm = ChunkAssembler::new(blob.len() as u64);
		loop {
			let chunk = build_response(&blob, 10_000, asm.next_offset(), 700);
			if chunk.bytes.is_empty() {
				break;
			}
			asm.accept(chunk.offset, &chunk.bytes).unwrap();
		}
		assert!(asm.is_complete());
		assert_eq!(asm.into_blob().unwrap(), blob);
	}

	#[test]
	fn assembler_rejects_non_contiguous() {
		let mut asm = ChunkAssembler::new(100);
		asm.accept(0, &[0u8; 10]).unwrap();
		// Gap: next expected offset is 10, not 20.
		assert_eq!(
			asm.accept(20, &[0u8; 10]),
			Err(AssembleError::NonContiguous { expected: 10, got: 20 })
		);
		// State unchanged after a rejected chunk.
		assert_eq!(asm.next_offset(), 10);
	}

	#[test]
	fn assembler_rejects_overflow_and_incomplete() {
		let mut asm = ChunkAssembler::new(16);
		assert_eq!(
			asm.accept(0, &[0u8; 32]),
			Err(AssembleError::Overflow { total: 16, offset: 0, len: 32 })
		);
		asm.accept(0, &[0u8; 8]).unwrap();
		assert_eq!(asm.into_blob(), Err(AssembleError::Incomplete { have: 8, total: 16 }));
	}

	#[test]
	fn snapshot_compression_roundtrip() {
		let raw: Vec<u8> = (0..=255u8).cycle().take(16_384).collect();
		let compressed = compress_snapshot(&raw).expect("compress");
		let decompressed = decompress_snapshot(&compressed, raw.len() as u64).expect("decompress");
		assert_eq!(decompressed, raw);
	}

	#[test]
	fn decompression_rejects_length_mismatch() {
		let raw = vec![42u8; 128];
		let compressed = compress_snapshot(&raw).expect("compress");
		let err = decompress_snapshot(&compressed, raw.len() as u64 + 1)
			.expect_err("wrong expected length must fail");
		assert!(matches!(err, DecompressError::LengthMismatch { .. }));
	}
}

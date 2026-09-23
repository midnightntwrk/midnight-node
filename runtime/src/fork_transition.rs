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

//! Runtime-side support for syncing a forked chain from true genesis.
//!
//! A fork produced by `mock-authorities convert` contains one block that a
//! stranger node cannot accept: its seal is signed by a key that is not in the
//! parent's on-chain authority set, and its state root reflects a delta no
//! runtime would have produced. Both are unavoidable -- an authority handover
//! has to be signed by the outgoing set, and the outgoing set is the real
//! network's validators.
//!
//! The escape hatch is Substrate's own: the chain spec's `codeSubstitutes` can
//! name a different runtime to use from a given block onward. A node syncing
//! the fork boots with a runtime built from this feature, which relaxes exactly
//! three things at exactly one block height:
//!
//! 1. [`aura_authorities_at`] reports the fork's mock authorities for the fork
//!    block's *parent*, so its seal verifies.
//! 2. `check_inherents` is skipped for the fork block, which carries a delta
//!    rather than the inherents a proposer would have produced.
//! 3. `execute_block` applies that delta instead of running `Executive`, which
//!    reproduces the header's state root exactly.
//!
//! Every other block, at every other height, executes unchanged -- it has to:
//! a `codeSubstitutes` entry stays active for all later blocks sharing the
//! on-chain `spec_version`, so any unconditional divergence here would desync
//! the node from the validators on the very next block.
//!
//! ## Configuration
//!
//! Both values are read at compile time, so a given fork runtime only ever
//! works for the fork it was built for:
//!
//! ```text
//! MIDNIGHT_FORK_HEIGHT=<fork block number>
//! MIDNIGHT_FORK_AURA_AUTHORITIES=0x<32-byte sr25519 key>,0x<...>
//! ```
//!
//! Both come from the `fork-bundle.json` that `mock-authorities convert`
//! writes.
//!
//! ## The failure mode to know about
//!
//! `codeSubstitutes` is keyed by `spec_version`: the substitute is looked up by
//! the *on-chain* spec version at that block, and matched against the substitute
//! runtime's own. Build this from a source revision whose `spec_version` differs
//! from the forked chain's - easily done, since a snapshot's node version and its
//! on-chain runtime version need not agree - and the substitute silently never
//! applies. Nothing reports a misconfiguration; instead `authorities()` falls
//! through to the real on-chain set and the fork block is rejected with
//! "Bad signature", which looks like a broken seal rather than a wrong build.
//! Check the chain's `state_getRuntimeVersion` before building. Built without them, every hook below is a no-op and the runtime
//! behaves exactly like the stock one.
//!
//! Nothing here touches the runtime's pallet set, call enum, or metadata. The
//! delta travels as an opaque block-body blob rather than a dispatchable, so a
//! runtime built without this feature is identical to the stock one rather
//! than merely equivalent -- which is the point, given what the feature does.

use alloc::vec::Vec;

use parity_scale_codec::DecodeAll;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;

use crate::BlockNumber;

/// Magic prefix marking a block-body blob as a fork delta.
///
/// Chosen so the blob cannot be mistaken for a transaction: SCALE-decoding it
/// as an `UncheckedExtrinsic` reads `b'M'` (0x4d) as a bare-preamble extrinsic
/// version, which no runtime supports, so every ordinary consumer rejects it.
/// Shared wire contract with `mock-authorities`' `fork::FORK_DELTA_MAGIC`.
pub const FORK_DELTA_MAGIC: &[u8; 8] = b"MNFORK1\0";

/// A batch of raw storage writes: `Some(value)` sets a key, `None` deletes it.
pub type StorageDelta = Vec<(Vec<u8>, Option<Vec<u8>>)>;

/// The fork block's own number, or `None` when this runtime was built without
/// `MIDNIGHT_FORK_HEIGHT`.
pub fn fork_height() -> Option<BlockNumber> {
	parse_u32(option_env!("MIDNIGHT_FORK_HEIGHT")?)
}

/// Whether `block` is the fork block.
pub fn is_fork_block(block: &<crate::Block as sp_runtime::traits::Block>::LazyBlock) -> bool {
	fork_height() == Some(block.header.number)
}

/// The AURA authorities to report for the block at `number`.
///
/// Returns the fork's mock set only when `number` is the fork block's parent:
/// that is the state the client reads when it checks the fork block's seal.
/// From the fork block onward the mocked authorities are in state, so every
/// later height falls through to the stock lookup.
pub fn aura_authorities_at(number: BlockNumber) -> Option<Vec<AuraId>> {
	let fork_height = fork_height()?;
	if number.checked_add(1)? != fork_height {
		return None;
	}

	let raw = option_env!("MIDNIGHT_FORK_AURA_AUTHORITIES")?;
	let authorities: Vec<AuraId> = raw
		.split(',')
		.filter(|entry| !entry.trim().is_empty())
		.map(|entry| {
			let bytes = parse_hex32(entry.trim())
				.expect("MIDNIGHT_FORK_AURA_AUTHORITIES holds 32-byte hex keys; qed");
			AuraId::from(sp_core::sr25519::Public::from_raw(bytes))
		})
		.collect();

	// An empty list would make `slot % len` panic in the client's slot-author
	// lookup, which is a far worse failure than refusing to special-case.
	if authorities.is_empty() { None } else { Some(authorities) }
}

/// Decode a fork delta out of one block-body blob, if that is what it is.
fn decode_delta(blob: &[u8]) -> Option<StorageDelta> {
	let rest = blob.strip_prefix(FORK_DELTA_MAGIC.as_slice())?;
	StorageDelta::decode_all(&mut &rest[..]).ok()
}

/// Apply the fork block's delta and nothing else.
///
/// `Executive` is deliberately bypassed. The delta was computed by the fork
/// tool as raw writes on top of the parent state, and the fork block's header
/// commits to the root of exactly those writes. Running `initialize_block` /
/// `finalize_block` around them would add frame-system bookkeeping the tool
/// never accounted for, and the resulting root would not match.
///
/// Bypassing `Executive` also bypasses its `final_checks`, so the header
/// commitments are re-established here and in the client:
///
/// - the extrinsics root is asserted below, so the body executed is exactly the
///   body the header commits to;
/// - the body must be exactly one well-formed delta blob, so a missing,
///   corrupted or extra entry panics instead of being skipped;
/// - the state root is checked by the client, which compares the executed root
///   against the header after `execute_block` returns and rejects the block on
///   mismatch.
pub fn execute_fork_block(block: &<crate::Block as sp_runtime::traits::Block>::LazyBlock) {
	let extrinsics_root = frame_system::extrinsics_root::<
		<crate::Runtime as frame_system::Config>::Hashing,
		_,
	>(&block.extrinsics, crate::VERSION.extrinsics_root_state_version());
	assert!(
		block.header.extrinsics_root == extrinsics_root,
		"Fork block extrinsics root must match its body.",
	);

	let [blob] = block.extrinsics.as_slice() else {
		panic!("Fork block body must be exactly one delta blob, got {}", block.extrinsics.len());
	};
	let delta = decode_delta(blob.inner()).expect("Fork block body must be a well-formed delta");

	for (key, value) in &delta {
		match value {
			Some(value) => frame_support::storage::unhashed::put_raw(key, value),
			None => frame_support::storage::unhashed::kill(key),
		}
	}

	log::info!(
		target: "runtime::fork-transition",
		"Applied fork delta at #{:?}: {} storage writes",
		block.header.number,
		delta.len(),
	);
}

/// Parse a decimal `u32` without `str::parse`, which is not available in the
/// const-adjacent context this is used from and pulls in formatting machinery.
fn parse_u32(raw: &str) -> Option<BlockNumber> {
	let mut value: BlockNumber = 0;
	for byte in raw.trim().as_bytes() {
		let digit = byte.checked_sub(b'0').filter(|d| *d <= 9)?;
		value = value.checked_mul(10)?.checked_add(digit as BlockNumber)?;
	}
	Some(value)
}

/// Parse `0x`-prefixed (or bare) 32-byte hex.
fn parse_hex32(raw: &str) -> Option<[u8; 32]> {
	let raw = raw.strip_prefix("0x").unwrap_or(raw).as_bytes();
	if raw.len() != 64 {
		return None;
	}

	let mut out = [0u8; 32];
	for (index, chunk) in raw.as_chunks::<2>().0.iter().enumerate() {
		out[index] = (nibble(chunk[0])? << 4) | nibble(chunk[1])?;
	}
	Some(out)
}

fn nibble(byte: u8) -> Option<u8> {
	match byte {
		b'0'..=b'9' => Some(byte - b'0'),
		b'a'..=b'f' => Some(byte - b'a' + 10),
		b'A'..=b'F' => Some(byte - b'A' + 10),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Decode;

	/// The safety property this feature rests on: built without the fork
	/// configuration, every hook is inert and the runtime behaves like stock.
	#[test]
	fn is_inert_without_configuration() {
		if option_env!("MIDNIGHT_FORK_HEIGHT").is_some() {
			// Configured build: this test cannot assert inertness, but the
			// height must at least parse, or the hooks would never fire.
			assert!(fork_height().is_some(), "MIDNIGHT_FORK_HEIGHT must parse");
			return;
		}

		assert_eq!(fork_height(), None);
		assert_eq!(aura_authorities_at(0), None);
		assert_eq!(aura_authorities_at(BlockNumber::MAX), None);
	}

	#[test]
	fn parses_decimal_heights() {
		assert_eq!(parse_u32("0"), Some(0));
		assert_eq!(parse_u32(" 1866778 "), Some(1_866_778));
		assert_eq!(parse_u32("12a"), None);
		assert_eq!(parse_u32(""), Some(0));
	}

	#[test]
	fn parses_prefixed_and_bare_hex() {
		let bare = "00".repeat(31) + "ff";
		let mut expected = [0u8; 32];
		expected[31] = 0xff;

		assert_eq!(parse_hex32(&bare), Some(expected));
		assert_eq!(parse_hex32(&alloc::format!("0x{bare}")), Some(expected));
	}

	/// Golden vector for the cross-repo wire contract, byte-identical to
	/// `mock-authorities`' `fork::GOLDEN_FORK_BLOB`. The tool encodes the fork
	/// delta and this runtime decodes it; nothing else checks that those two
	/// agree, and a silent disagreement would surface only as a state-root
	/// mismatch on a node that is already hours into a sync.
	#[test]
	fn decodes_the_tools_golden_blob() {
		const GOLDEN: &str = "544d4e464f524b3100080826aa010c010203083abb00";

		let encoded = decode_hex(GOLDEN);
		// Strip the compact length prefix the block body carries.
		let inner = Vec::<u8>::decode(&mut &encoded[..]).expect("compact-prefixed blob");

		let expected: StorageDelta = alloc::vec![
			(alloc::vec![0x26u8, 0xaa], Some(alloc::vec![1u8, 2, 3])),
			(alloc::vec![0x3au8, 0xbb], None),
		];
		assert_eq!(decode_delta(&inner), Some(expected));
	}

	fn decode_hex(raw: &str) -> Vec<u8> {
		raw.as_bytes()
			.as_chunks::<2>()
			.0
			.iter()
			.map(|c| (nibble(c[0]).unwrap() << 4) | nibble(c[1]).unwrap())
			.collect()
	}

	#[test]
	fn decodes_only_magic_prefixed_blobs() {
		use parity_scale_codec::Encode;

		let delta: StorageDelta = alloc::vec![(alloc::vec![1u8, 2], Some(alloc::vec![3u8]))];
		let mut blob = FORK_DELTA_MAGIC.to_vec();
		blob.extend_from_slice(&delta.encode());

		assert_eq!(decode_delta(&blob), Some(delta.clone()));
		// An ordinary extrinsic must never be mistaken for a delta.
		assert_eq!(decode_delta(&delta.encode()), None);
		assert_eq!(decode_delta(b"MNFORK1"), None);
		// Trailing garbage after a valid delta is corruption, not a shorter delta.
		blob.push(0);
		assert_eq!(decode_delta(&blob), None);
	}

	#[test]
	fn rejects_wrong_length_and_non_hex() {
		assert_eq!(parse_hex32("0x00"), None);
		assert_eq!(parse_hex32(&"zz".repeat(32)), None);
	}
}

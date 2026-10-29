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

//! Bind the signed next committee to the runtime MMR tip, including at rotation.
use crate::{
	decode_all, decode_justification, keccak, mmr_authority_set, payload, AuthoritySet, Hash,
	Result, Stakes,
};
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, Encode, Decode)]
pub struct CommitteeProof {
	pub leaf: Vec<u8>,
	pub leaf_index: u64,
	pub leaf_count: u64,
	/// Standard Substrate MMR proof: left peaks, then the tip's left siblings.
	pub items: Vec<Hash>,
}

fn merge(left: &Hash, right: &Hash) -> Hash {
	let mut bytes = [0; 64];
	bytes[..32].copy_from_slice(left);
	bytes[32..].copy_from_slice(right);
	keccak(&bytes)
}

impl CommitteeProof {
	pub fn root(&self) -> Result<Hash> {
		// Bounds also prevent ambiguous/truncated counts and oversized proof paths.
		if self.leaf_count == 0
			|| self.leaf_count > u32::MAX as u64
			|| self.leaf_index != self.leaf_count - 1
		{
			return Err("committee proof must be the MMR tip");
		}
		let siblings = self.leaf_count.trailing_zeros() as usize;
		let left_peaks = self.leaf_count.count_ones() as usize - 1;
		if self.items.len() != siblings + left_peaks {
			return Err("invalid committee MMR path length");
		}
		let mut root = keccak(&self.leaf);
		for sibling in &self.items[left_peaks..] {
			root = merge(sibling, &root);
		}
		// Substrate bags peaks from right to left using hash(right, left).
		for peak in self.items[..left_peaks].iter().rev() {
			root = merge(&root, peak);
		}
		Ok(root)
	}
}

/// Checks inclusion and runtime/payload agreement. Callers must also verify BEEFY signatures.
pub fn verify_committee(justification: &[u8], proof: &CommitteeProof) -> Result<()> {
	let signed = decode_justification(justification)?;
	let c = &signed.commitment;
	// Fixed standalone v1.0 leaf: version, parent number/hash, next set, message root.
	if proof.leaf.len() != 113 || proof.leaf[0] != 0x20 {
		return Err("unsupported committee MMR leaf");
	}
	let parent: u32 = decode_all(&proof.leaf[1..5])?;
	if parent.checked_add(1) != Some(c.block_number) || proof.leaf_count > c.block_number as u64 {
		return Err("committee leaf is not from the signed block");
	}
	if proof.root()? != payload::<Hash>(c, b"mh")? {
		return Err("committee MMR root mismatch");
	}
	// A historical leaf may still announce the old committee. Only the tip is accepted.
	// Never infer this state from a current, unauthenticated RPC authority list.
	let runtime_next: AuthoritySet = decode_all(&proof.leaf[37..81])?;
	let next_stakes: Stakes = payload(c, b"ns")?;
	let next: AuthoritySet = payload(c, b"nb")?;
	if runtime_next != mmr_authority_set(next.id, &next_stakes)? {
		return Err("runtime next committee disagrees with signed payload");
	}
	Ok(())
}

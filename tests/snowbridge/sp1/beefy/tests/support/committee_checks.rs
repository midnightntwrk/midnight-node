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

use super::*;
use polkadot_ckb_merkle_mountain_range::{util::MemStore, Merge, MMR};

struct RuntimeMerge;
impl Merge for RuntimeMerge {
	type Item = Hash;
	fn merge(left: &Hash, right: &Hash) -> polkadot_ckb_merkle_mountain_range::Result<Hash> {
		Ok(keccak(&[left.as_slice(), right.as_slice()].concat()))
	}
}

#[test]
fn tip_verifier_matches_substrate_mmr_across_peak_boundaries() {
	let store = MemStore::default();
	let mut mmr = MMR::<Hash, RuntimeMerge, _>::new(0, &store);
	for count in 1..=257u64 {
		let leaf = vec![count as u8; 113];
		let pos = mmr.push(keccak(&leaf)).unwrap();
		let path = mmr.gen_proof(vec![pos]).unwrap();
		let proof = CommitteeProof {
			leaf,
			leaf_index: count - 1,
			leaf_count: count,
			items: path.proof_items().to_vec(),
		};
		assert_eq!(proof.root().unwrap(), mmr.get_root().unwrap(), "count {count}");
	}
}

#[test]
fn rejects_signed_next_committee_that_disagrees_with_runtime() {
	let first = fixture(4, 0).unwrap();
	let mut batch = batch_with_witnesses(first.checkpoint, vec![first.justification]);
	let keys = signing_keys(batch.checkpoint.current.len as usize, 0);
	let replacement = stakes(&signing_keys(3, 30000));
	let mut c = decode_justification(&batch.justifications[0]).unwrap().commitment;
	for (id, bytes) in &mut c.payload {
		if id == b"ns" {
			*bytes = replacement.encode();
		}
		if id == b"nb" {
			*bytes = authority_set(8, &replacement).unwrap().encode();
		}
	}
	batch.justifications[0] = sign(c, &keys, &[0, 1, 2, 3]);
	// The old signature-only verifier accepts this fully signed, self-consistent payload.
	assert!(verify(&Input {
		checkpoint: batch.checkpoint.clone(),
		justification: batch.justifications[0].clone()
	})
	.is_ok());
	assert_eq!(verify_batch(&batch), Err("runtime next committee disagrees with signed payload"));
}

#[test]
fn rejects_missing_stale_tampered_and_non_tip_committee_witnesses() {
	let first = fixture(4, 0).unwrap();
	let valid = batch_with_witnesses(first.checkpoint, vec![first.justification]);
	assert!(verify_batch(&valid).is_ok());
	for case in 0..10 {
		let mut bad = valid.clone();
		let p = &mut bad.committee_proofs[0];
		match case {
			0 => p.leaf[49] ^= 1,   // runtime next committee root
			1 => p.leaf[37] ^= 1,   // runtime next set ID
			2 => p.leaf[45] ^= 1,   // runtime next set size
			3 => p.leaf[1] -= 1,    // historical committee state
			4 => p.leaf_index -= 1, // non-tip inclusion
			5 => p.items[0][0] ^= 1,
			6 => p.items.push([0; 32]),
			7 => p.leaf.push(0),
			8 => p.leaf[0] = 0x21,
			_ => {
				bad.committee_proofs.clear();
			},
		}
		assert!(verify_batch(&bad).is_err(), "case {case}");
	}
}

#[test]
fn cannot_omit_runtime_next_state_even_with_valid_current_signatures() {
	let first = fixture(4, 0).unwrap();
	let mut batch = batch_with_witnesses(first.checkpoint, vec![first.justification]);
	let keys = signing_keys(batch.checkpoint.current.len as usize, 0);
	let mut c = decode_justification(&batch.justifications[0]).unwrap().commitment;
	c.payload.retain(|p| p.0 != *b"ns" && p.0 != *b"nb");
	batch.justifications[0] = sign(c, &keys, &[0, 1, 2, 3]);
	assert_eq!(verify_batch(&batch), Err("missing required payload"));
}

#[test]
fn rotated_committee_must_match_pinned_next_and_old_committee_cannot_return() {
	let first = fixture(4, 0).unwrap();
	let initial = batch_with_witnesses(first.checkpoint.clone(), vec![first.justification.clone()]);
	let before = verify_batch(&initial).unwrap().new_state;
	let keys8 = signing_keys(before.current.len as usize, 10000);
	let keys9 = signing_keys(4, 20000);
	let rotated =
		sign(commitment(8, 101, &stakes(&keys8), Some(&stakes(&keys9)), 0), &keys8, &[0, 1, 2, 3]);
	let after = verify_batch(&batch_with_witnesses(before.clone(), vec![rotated.clone()]))
		.unwrap()
		.new_state;
	assert_eq!(Some(after.current.clone()), before.next);
	assert_eq!(after.next, Some(authority_set(9, &stakes(&keys9)).unwrap()));
	let old_keys = signing_keys(before.current.len as usize, 0);
	let stale = sign(
		commitment(7, 102, &stakes(&old_keys), Some(&stakes(&keys8)), 0),
		&old_keys,
		&[0, 1, 2, 3],
	);
	assert_eq!(
		verify_batch(&batch_with_witnesses(after, vec![stale])),
		Err("unannounced validator set")
	);
	let impostor_keys = signing_keys(before.current.len as usize, 30000);
	let impostor = sign(
		commitment(8, 101, &stakes(&impostor_keys), Some(&stakes(&keys9)), 0),
		&impostor_keys,
		&[0, 1, 2, 3],
	);
	assert_eq!(
		verify_batch(&batch_with_witnesses(before, vec![impostor])),
		Err("current authority commitment mismatch")
	);
}

#[test]
fn same_membership_rotation_still_checks_id_and_new_successor() {
	let keys = signing_keys(4, 0);
	let members = stakes(&keys);
	let initial = Checkpoint {
		genesis_hash: [1; 32],
		block_number: 0,
		current: authority_set(7, &members).unwrap(),
		next: Some(authority_set(8, &members).unwrap()),
	};
	let rotated = sign(commitment(8, 1, &members, Some(&members), 0), &keys, &[0, 1, 2]);
	let output = verify_batch(&batch_with_witnesses(initial, vec![rotated])).unwrap();
	assert_eq!(output.new_state.current.id, 8);
	assert_eq!(output.new_state.next.unwrap().id, 9);
}

#[test]
fn runtime_state_rejects_premature_and_late_rotation_even_with_valid_signatures() {
	let first = fixture(4, 0).unwrap();
	let n = first.checkpoint.current.len as usize;
	let old_keys = signing_keys(n, 0);
	let new_keys = signing_keys(n, 10000);
	let successor = stakes(&signing_keys(n, 20000));
	let mut checkpoint = first.checkpoint;
	checkpoint.next = Some(authority_set(8, &stakes(&new_keys)).unwrap());
	for premature in [true, false] {
		let (id, keys, next) = if premature {
			(8, &new_keys, successor.clone())
		} else {
			(7, &old_keys, stakes(&new_keys))
		};
		// Early promotion sees the runtime still announcing 8; a lingering old
		// committee sees the rotated runtime already announcing 9.
		let witness = if premature {
			committee_proof(100, 8, &stakes(&new_keys))
		} else {
			committee_proof(100, 9, &successor)
		};
		let mut c = commitment(id, 100, &stakes(keys), Some(&next), 0);
		c.payload.iter_mut().find(|p| p.0 == *b"mh").unwrap().1 = witness.root().unwrap().encode();
		let signed = sign(c, keys, &[0, 1, 2, 3]);
		assert!(verify(&Input { checkpoint: checkpoint.clone(), justification: signed.clone() })
			.is_ok());
		let batch = BatchInput {
			checkpoint: checkpoint.clone(),
			justifications: vec![signed],
			committee_proofs: vec![witness],
		};
		assert_eq!(
			verify_batch(&batch),
			Err("runtime next committee disagrees with signed payload")
		);
	}
}

#[test]
fn selection_defers_rotation_until_runtime_confirms_the_next_committee() {
	let first = fixture(4, 0).unwrap();
	let n = first.checkpoint.current.len as usize;
	let keys8 = signing_keys(n, 10000);
	let keys9 = signing_keys(n, 20000);
	let first_witness = committee_proof(100, 8, &stakes(&keys8));
	let boundary_witness = committee_proof(101, 8, &stakes(&keys8));
	let mut boundary = commitment(8, 101, &stakes(&keys8), Some(&stakes(&keys9)), 0);
	boundary.payload.iter_mut().find(|p| p.0 == *b"mh").unwrap().1 =
		boundary_witness.root().unwrap().encode();
	let boundary = sign(boundary, &keys8, &[0, 1, 2, 3]);
	let confirmed =
		sign(commitment(8, 102, &stakes(&keys8), Some(&stakes(&keys9)), 0), &keys8, &[0, 1, 2, 3]);
	let confirmed_witness = committee_proof(102, 9, &stakes(&keys9));
	let captures = vec![
		(100, first.justification, first_witness),
		(101, boundary, boundary_witness),
		(102, confirmed, confirmed_witness),
	];
	assert!(select::select(first.checkpoint.clone(), captures.clone(), 101).is_err());
	let batch = select::select(first.checkpoint, captures, 102).unwrap();
	assert_eq!(
		batch
			.justifications
			.iter()
			.map(|j| decode_justification(j).unwrap().commitment.block_number)
			.collect::<Vec<_>>(),
		vec![100, 102]
	);
	assert_eq!(verify_batch(&batch).unwrap().new_state.current.id, 8);
}

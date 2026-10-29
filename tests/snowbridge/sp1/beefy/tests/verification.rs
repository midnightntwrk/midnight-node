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

use k256::ecdsa::{SigningKey, VerifyingKey};
use midnight_beefy::*;
use parity_scale_codec::Encode;

fn mutate(input: &mut Input, change: impl FnOnce(&mut CompactSignedCommitment)) {
	let mut p = decode_justification(&input.justification).unwrap();
	change(&mut p);
	input.justification = [vec![1], p.encode()].concat();
}

#[test]
fn quorum_boundaries_and_matrix() {
	assert!(quorum(0).is_err());
	for (n, expected) in [(1, 1), (2, 2), (3, 3), (4, 3), (5, 4), (6, 5), (100, 67), (300, 201)] {
		assert_eq!(quorum(n).unwrap(), expected);
	}
	for s in [1, 2, 3, 4, 8, 16, 32] {
		let input = fixture(s, 37).unwrap();
		let out = verify(&input).unwrap();
		assert_eq!(out.signatures_verified, s as u32);
		assert_eq!(out.previous_state_hash, state_hash(&input.checkpoint));
		assert_eq!(out.new_state.next.as_ref().unwrap().id, 8);
	}
}

#[test]
fn rejects_exact_two_thirds_for_three_validators() {
	let keys = signing_keys(3, 0);
	let current = stakes(&keys);
	let input = Input {
		checkpoint: Checkpoint {
			genesis_hash: [1; 32],
			block_number: 0,
			current: authority_set(7, &current).unwrap(),
			next: None,
		},
		justification: sign(commitment(7, 1, &current, None, 0), &keys, &[0, 1]),
	};
	assert_eq!(verify(&input), Err("insufficient signatures"));
}

#[test]
fn sparse_indices_and_eight_validator_padding() {
	let keys = signing_keys(8, 0);
	let current = stakes(&keys);
	let input = Input {
		checkpoint: Checkpoint {
			genesis_hash: [1; 32],
			block_number: 0,
			current: authority_set(7, &current).unwrap(),
			next: None,
		},
		justification: sign(commitment(7, 1, &current, None, 0), &keys, &[0, 2, 3, 4, 6, 7]),
	};
	let p = decode_justification(&input.justification).unwrap();
	assert_eq!(p.signatures_from, [0b10111011, 0]);
	assert!(verify(&input).is_ok());
	let mut short = input.clone();
	mutate(&mut short, |p| {
		p.signatures_from.pop();
	});
	assert!(verify(&short).is_err());
}

#[test]
fn rejects_malformed_wire_and_bitmap() {
	let original = fixture(4, 0).unwrap();
	for change in 0..7 {
		let mut input = original.clone();
		match change {
			0 => input.justification[0] = 2,
			1 => input.justification.push(0),
			2 => {
				input.justification.pop();
			},
			3 => mutate(&mut input, |p| p.signatures_from[0] |= 1),
			4 => mutate(&mut input, |p| {
				p.signatures.pop();
			}),
			5 => mutate(&mut input, |p| p.validator_set_len = 0),
			_ => mutate(&mut input, |p| p.validator_set_len = 4097),
		}
		assert!(verify(&input).is_err(), "case {change}");
	}
}

#[test]
fn signatures_bind_every_commitment_byte() {
	let original = fixture(4, 20).unwrap();
	for change in 0..5 {
		let mut input = original.clone();
		mutate(&mut input, |p| match change {
			0 => p.commitment.block_number += 1,
			1 => p.commitment.payload.iter_mut().find(|p| p.0 == *b"mh").unwrap().1[0] ^= 1,
			2 => p.commitment.payload.iter_mut().find(|p| p.0 == *b"zz").unwrap().1[0] ^= 1,
			3 => p.signatures[0][0] ^= 1,
			_ => p.signatures[0][64] ^= 1,
		});
		assert!(verify(&input).is_err(), "case {change}");
	}
}

#[test]
fn replay_and_unknown_set_are_rejected() {
	let mut input = fixture(4, 0).unwrap();
	input.checkpoint.block_number = 100;
	assert_eq!(verify(&input), Err("stale commitment"));
	input.checkpoint.block_number = 0;
	mutate(&mut input, |p| p.commitment.validator_set_id = 9);
	assert_eq!(verify(&input), Err("unannounced validator set"));
}

#[test]
fn attacker_cannot_self_authenticate_an_authority_set() {
	let mut input = fixture(4, 0).unwrap();
	input.checkpoint.current.root = [0; 32];
	assert_eq!(verify(&input), Err("current authority commitment mismatch"));
	// Altering the list to invent extra slots cannot match the pinned root.
	let mut changed = fixture(4, 0).unwrap();
	mutate(&mut changed, |p| {
		let field = p.commitment.payload.iter_mut().find(|p| p.0 == *b"cs").unwrap();
		let mut members: Stakes = decode_all(&field.1).unwrap();
		members[1] = members[0];
		field.1 = members.encode();
	});
	assert_eq!(verify(&changed), Err("current authority commitment mismatch"));
}

#[test]
fn signed_but_invalid_payloads_are_rejected() {
	let original = fixture(4, 0).unwrap();
	let keys = signing_keys(original.checkpoint.current.len as usize, 0);
	for change in 0..7 {
		let mut input = original.clone();
		let mut c = decode_justification(&input.justification).unwrap().commitment;
		match change {
			0 => c.payload.push(c.payload[0].clone()),
			1 => c.payload.reverse(),
			2 => c.payload.retain(|p| p.0 != *b"mh"),
			3 => c.payload.iter_mut().find(|p| p.0 == *b"mh").unwrap().1.push(0),
			4 => c.payload.retain(|p| p.0 != *b"nb"),
			5 => c.payload.iter_mut().find(|p| p.0 == *b"cs").unwrap().1[34] ^= 1,
			_ => c.payload.iter_mut().find(|p| p.0 == *b"nb").unwrap().1[0] ^= 1,
		}
		input.justification = sign(c, &keys, &[0, 1, 2, 3]);
		assert!(verify(&input).is_err(), "case {change}");
	}
}

#[test]
fn rotation_uses_previously_authenticated_next_set() {
	let first = fixture(4, 0).unwrap();
	let out = verify(&first).unwrap();
	let keys = signing_keys(out.new_state.current.len as usize, 10000);
	let next = stakes(&keys);
	let rotated = Input {
		checkpoint: out.new_state.clone(),
		justification: sign(commitment(8, 101, &next, None, 0), &keys, &[0, 1, 2, 3]),
	};
	let result = verify(&rotated).unwrap();
	assert_eq!(result.new_state.current, out.new_state.next.unwrap());
	assert!(result.new_state.next.is_none());
	let mut unannounced = rotated;
	unannounced.checkpoint.next = None;
	assert_eq!(verify(&unannounced), Err("unannounced validator set"));
}

#[test]
fn next_set_announcement_cannot_be_replaced_or_forgotten() {
	let first = fixture(4, 0).unwrap();
	let state = verify(&first).unwrap().new_state;
	let keys = signing_keys(state.current.len as usize, 0);
	let current = stakes(&keys);
	let replacement = stakes(&signing_keys(3, 20000));
	let mut input = Input {
		checkpoint: state.clone(),
		justification: sign(
			commitment(7, 101, &current, Some(&replacement), 0),
			&keys,
			&[0, 1, 2, 3],
		),
	};
	// Even a valid current-set quorum cannot replace a pinned next set.
	assert_eq!(verify(&input), Err("conflicting next authority set"));
	input.justification = sign(commitment(7, 101, &current, None, 0), &keys, &[0, 1, 2, 3]);
	assert_eq!(verify(&input).unwrap().new_state.next, state.next);
}

#[test]
fn high_s_recovery_matches_low_s() {
	let input = fixture(1, 0).unwrap();
	let mut high = input.clone();
	mutate(&mut high, |p| {
		// secp256k1 order minus s (big-endian subtraction).
		let order = hex::decode("fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141")
			.unwrap();
		let mut borrow = 0i16;
		for i in (0..32).rev() {
			let value = order[i] as i16 - p.signatures[0][32 + i] as i16 - borrow;
			p.signatures[0][32 + i] = value as u8;
			borrow = i16::from(value < 0);
		}
		p.signatures[0][64] ^= 1;
	});
	assert_eq!(verify(&input).unwrap(), verify(&high).unwrap());
}

#[test]
fn upstream_scale_wire_layout_with_midnight_u32_block_number() {
	// Substrate commitment.rs signed_commitment_encode_decode_ecdsa golden vector.
	// Its TestCommitment uses u128; Midnight uses u32, so remove the extra 12 zero bytes.
	let signature_hex = concat!(
		"558455ad81279df0795cc985580e4fb75d72d948d1107b2ac80a09abed4da8480c746c",
		"c321f2319a5e99a830e314d10dd3cd68ce3dc0c33c86e99bcb7816f9ba012d6e1f8105c337a86cdd9aa",
		"acdc496577f3db8c55ef9e6fd48f2c5c05a2274707491635d8ba3df64f324575b7b2a34487bca2324b6",
		"a0046395a71681be3d0c2a00"
	);
	let wire = hex::decode(format!(
		"01046d68343048656c6c6f20576f726c642105000000000000000000000004300400000008{signature_hex}"
	))
	.unwrap();
	let p = decode_justification(&wire).unwrap();
	assert_eq!(p.commitment.block_number, 5);
	assert_eq!(p.commitment.validator_set_id, 0);
	assert_eq!(p.signatures_from, [0x30]);
	assert_eq!(p.signatures.len(), 2);
	assert_eq!(p.commitment.payload, vec![(*b"mh", "Hello World!".encode())]);
	assert_eq!([vec![1], p.encode()].concat(), wire);
}

#[test]
fn odd_merkle_leaf_is_promoted_not_duplicated() {
	let members = stakes(&signing_keys(3, 0));
	let leaves = members
		.iter()
		.map(|(key, stake)| {
			let pubkey = VerifyingKey::from_sec1_bytes(key).unwrap().to_encoded_point(false);
			let address = keccak(&pubkey.as_bytes()[1..]);
			keccak(&[&address[12..], &stake.to_le_bytes()].concat())
		})
		.collect::<Vec<_>>();
	let left = keccak(&[leaves[0].as_slice(), leaves[1].as_slice()].concat());
	let expected = keccak(&[left.as_slice(), leaves[2].as_slice()].concat());
	assert_eq!(authority_set(0, &members).unwrap().root, expected);
}

#[test]
fn authenticated_duplicate_slots_have_midnight_voting_weight() {
	let unique = signing_keys(2, 0);
	let keys = vec![unique[0].clone(), unique[0].clone(), unique[0].clone(), unique[1].clone()];
	let members = stakes(&keys);
	let c = commitment(7, 1, &members, None, 0);
	let mut input = Input {
		checkpoint: Checkpoint {
			genesis_hash: [1; 32],
			block_number: 0,
			current: authority_set(7, &members).unwrap(),
			next: None,
		},
		justification: sign(c.clone(), &keys, &[0, 1, 2]),
	};
	// Alice has 3 of 4 authenticated slots, satisfying the node's multiplicity quorum.
	assert_eq!(verify(&input).unwrap().signatures_verified, 3);
	// A signature for Bob's single slot is insufficient, regardless of its stake field.
	input.justification = sign(c, &keys, &[3]);
	assert_eq!(verify(&input), Err("insufficient signatures"));
}

#[test]
fn batch_preserves_original_checkpoint_and_authenticates_changed_membership() {
	let first = fixture(4, 0).unwrap();
	let keys = signing_keys(first.checkpoint.current.len as usize, 10000);
	let rotated = sign(
		commitment(8, 101, &stakes(&keys), Some(&stakes(&signing_keys(keys.len(), 20000))), 0),
		&keys,
		&[0, 1, 2, 3],
	);
	let batch = batch_with_witnesses(
		first.checkpoint.clone(),
		vec![first.justification.clone(), rotated.clone()],
	);
	let output = verify_batch(&batch).unwrap();
	assert_eq!(output.previous_state_hash, state_hash(&first.checkpoint));
	assert_eq!(output.new_state.current.id, 8);
	assert_eq!(output.encode().len(), 225);
	assert_eq!(verify(&first).unwrap().encode().len(), 225);
	let mut missing = batch.clone();
	missing.justifications.remove(0);
	missing.committee_proofs.remove(0);
	assert_eq!(verify_batch(&missing), Err("unannounced validator set"));
	let mut replay = batch.clone();
	replay.justifications.push(rotated);
	replay.committee_proofs.push(replay.committee_proofs.last().unwrap().clone());
	assert_eq!(verify_batch(&replay), Err("stale commitment"));
	assert!(verify_batch(&BatchInput {
		checkpoint: first.checkpoint,
		justifications: vec![],
		committee_proofs: vec![]
	})
	.is_err());
}

pub fn signing_keys(n: usize, offset: u32) -> Vec<SigningKey> {
	(0..n)
		.map(|i| {
			let mut bytes = [0u8; 32];
			bytes[28..].copy_from_slice(&(offset + i as u32 + 1).to_be_bytes());
			SigningKey::from_slice(&bytes).unwrap()
		})
		.collect()
}

pub fn stakes(keys: &[SigningKey]) -> Stakes {
	keys.iter()
		.enumerate()
		.map(|(i, key)| {
			(
				key.verifying_key().to_encoded_point(true).as_bytes().try_into().unwrap(),
				(i + 1) as u64,
			)
		})
		.collect()
}

pub fn sign(c: Commitment, keys: &[SigningKey], indices: &[usize]) -> Vec<u8> {
	let digest = keccak(&c.encode());
	let mut bitmap = vec![0u8; keys.len() / 8 + 1];
	let signatures = indices
		.iter()
		.map(|&i| {
			bitmap[i / 8] |= 0x80 >> (i % 8);
			let (sig, recovery) = keys[i].sign_prehash_recoverable(&digest).unwrap();
			let mut bytes = [0u8; 65];
			bytes[..64].copy_from_slice(&sig.to_bytes());
			bytes[64] = recovery.to_byte();
			bytes
		})
		.collect();
	let proof = CompactSignedCommitment {
		commitment: c,
		signatures_from: bitmap,
		validator_set_len: keys.len() as u32,
		signatures,
	};
	let mut wire = vec![1];
	wire.extend(proof.encode());
	wire
}

pub fn commitment(
	id: u64,
	block: u32,
	current: &Stakes,
	next: Option<&Stakes>,
	extra: usize,
) -> Commitment {
	let mut payload = vec![
		(
			*b"mh",
			next.map(|n| committee_proof(block, id + 1, n).root().unwrap())
				.unwrap_or([0; 32])
				.encode(),
		),
		(*b"cs", current.encode()),
		(*b"cb", authority_set(id, current).unwrap().encode()),
	];
	if let Some(next) = next {
		payload.push((*b"ns", next.encode()));
		payload.push((*b"nb", authority_set(id + 1, next).unwrap().encode()));
	}
	if extra > 0 {
		payload.push((*b"zz", vec![42; extra]));
	}
	payload.sort_by_key(|p| p.0);
	Commitment { payload, block_number: block, validator_set_id: id }
}

/// Choose a committee whose minimum quorum has exactly `signature_count` members.
pub fn fixture(signature_count: usize, extra: usize) -> Result<Input> {
	let n = (3 * signature_count - 1) / 2;
	let keys = signing_keys(n, 0);
	let current = stakes(&keys);
	let next = stakes(&signing_keys(n, 10000));
	let checkpoint = Checkpoint {
		genesis_hash: keccak(b"SYNTHETIC-MIDNIGHT-BENCHMARK"),
		block_number: 99,
		current: authority_set(7, &current).unwrap(),
		next: None,
	};
	let c = commitment(7, 100, &current, Some(&next), extra);
	let indices = (0..signature_count).collect::<Vec<_>>();
	Ok(Input { checkpoint, justification: sign(c, &keys, &indices) })
}

#[test]
fn invalid_surplus_signature_cannot_hide_behind_a_valid_quorum() {
	let keys = signing_keys(4, 0);
	let members = stakes(&keys);
	let mut input = Input {
		checkpoint: Checkpoint {
			genesis_hash: [1; 32],
			block_number: 0,
			current: authority_set(7, &members).unwrap(),
			next: None,
		},
		justification: sign(commitment(7, 1, &members, None, 0), &keys, &[0, 1, 2, 3]),
	};
	assert_eq!(verify(&input).unwrap().signatures_verified, 4);
	mutate(&mut input, |proof| proof.signatures[3][0] ^= 1);
	assert!(verify(&input).is_err());
}

#[path = "../../host/src/select.rs"]
mod select;

#[test]
fn relayer_retains_earlier_announcements_when_later_commitments_omit_them() {
	let first = fixture(4, 0).unwrap();
	let keys8 = signing_keys(first.checkpoint.current.len as usize, 10000);
	let keys9 = signing_keys(first.checkpoint.current.len as usize, 20000);
	let announce =
		sign(commitment(8, 101, &stakes(&keys8), Some(&stakes(&keys9)), 0), &keys8, &[0, 1, 2, 3]);
	let omit = sign(commitment(8, 102, &stakes(&keys8), None, 0), &keys8, &[0, 1, 2, 3]);
	let target = sign(
		commitment(9, 103, &stakes(&keys9), Some(&stakes(&signing_keys(keys9.len(), 30000))), 0),
		&keys9,
		&[0, 1, 2, 3],
	);
	let captures = vec![(100, first.justification), (101, announce), (102, omit), (103, target)];
	let captures = captures
		.into_iter()
		.map(|(block, signed)| {
			let c = decode_justification(&signed).unwrap().commitment;
			let witness = if let Some((_, encoded)) = c.payload.iter().find(|p| p.0 == *b"ns") {
				committee_proof(block, c.validator_set_id + 1, &decode_all(encoded).unwrap())
			} else {
				committee_proof(block, c.validator_set_id + 1, &stakes(&keys9))
			};
			(block, signed, witness)
		})
		.collect();
	let batch = select::select(first.checkpoint, captures, 103).unwrap();
	assert_eq!(batch.justifications.len(), 3);
	let output = verify_batch(&batch).unwrap();
	assert_eq!(output.new_state.current.id, 9);
	assert_eq!(output.new_state.block_number, 103);
}

// Synthetic committee witnesses use the standard runtime MMR layout and proof order.
fn committee_proof(block: u32, id: u64, members: &Stakes) -> CommitteeProof {
	let mut leaf = vec![0x20];
	leaf.extend((block - 1).encode());
	leaf.extend([42; 32]);
	leaf.extend(mmr_authority_set(id, members).unwrap().encode());
	leaf.extend([43; 32]);
	let count = block as u64;
	CommitteeProof {
		leaf,
		leaf_index: count - 1,
		leaf_count: count,
		items: (0..count.trailing_zeros() + count.count_ones() - 1)
			.map(|i| keccak(&i.to_le_bytes()))
			.collect(),
	}
}
fn batch_with_witnesses(checkpoint: Checkpoint, justifications: Vec<Vec<u8>>) -> BatchInput {
	let committee_proofs = justifications
		.iter()
		.map(|j| {
			let c = decode_justification(j).unwrap().commitment;
			let members: Stakes =
				decode_all(&c.payload.iter().find(|p| p.0 == *b"ns").unwrap().1).unwrap();
			committee_proof(c.block_number, c.validator_set_id + 1, &members)
		})
		.collect();
	BatchInput { checkpoint, justifications, committee_proofs }
}
#[path = "support/committee_checks.rs"]
mod committee_checks;

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

//! Midnight BEEFY light-client verification, shared by native and SP1 execution.
#![no_std]
extern crate alloc;
use alloc::vec::Vec;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use parity_scale_codec::{Decode, Encode};
use tiny_keccak::{Hasher, Keccak};

mod committee;
pub use committee::{verify_committee, CommitteeProof};

pub type Hash = [u8; 32];
pub type PublicKey = [u8; 33];
pub type EcdsaSignature = [u8; 65];
pub type Stakes = Vec<(PublicKey, u64)>;
pub type Result<T> = core::result::Result<T, &'static str>;
pub const MAX_VALIDATORS: usize = 4096;
pub const MAX_PROOF_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AuthoritySet {
	pub id: u64,
	pub len: u32,
	pub root: Hash,
}

/// This state must be anchored by the destination verifier, never chosen by the relayer.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Checkpoint {
	pub genesis_hash: Hash,
	pub block_number: u32,
	pub current: AuthoritySet,
	pub next: Option<AuthoritySet>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Commitment {
	pub payload: Vec<([u8; 2], Vec<u8>)>,
	pub block_number: u32,
	pub validator_set_id: u64,
}

/// Exact V1 compact wire layout after the version byte, including fixed-size signatures.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct CompactSignedCommitment {
	pub commitment: Commitment,
	pub signatures_from: Vec<u8>,
	pub validator_set_len: u32,
	pub signatures: Vec<EcdsaSignature>,
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct Input {
	pub checkpoint: Checkpoint,
	pub justification: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Output {
	pub previous_state_hash: Hash,
	pub new_state: Checkpoint,
	pub commitment_hash: Hash,
	pub mmr_root: Hash,
	pub signatures_verified: u32,
}

pub struct Prepared {
	pub digest: Hash,
	pub signatures: Vec<(PublicKey, EcdsaSignature)>,
	pub output: Output,
}

pub fn keccak(bytes: &[u8]) -> Hash {
	let mut hash = Keccak::v256();
	hash.update(bytes);
	let mut out = [0; 32];
	hash.finalize(&mut out);
	out
}

pub fn decode_all<T: Decode>(bytes: &[u8]) -> Result<T> {
	let mut remaining = bytes;
	let value = T::decode(&mut remaining).map_err(|_| "invalid SCALE")?;
	if !remaining.is_empty() {
		return Err("trailing SCALE bytes");
	}
	Ok(value)
}

pub fn state_hash(state: &Checkpoint) -> Hash {
	keccak(&(b"midnight-vector-state-v1", state).encode())
}

/// BEEFY uses a strict supermajority, including when N is divisible by three.
pub fn quorum(n: usize) -> Result<usize> {
	if n == 0 || n > MAX_VALIDATORS {
		return Err("invalid validator count");
	}
	Ok(n - (n - 1) / 3)
}

/// PoC profile: Ethereum address || little-endian u64 stake. Fixed by the guest key.
/// Substrate binary_merkle_tree promotes an unpaired node unchanged.
pub fn authority_set(id: u64, stakes: &Stakes) -> Result<AuthoritySet> {
	authority_root(id, stakes, true)
}

/// Runtime MMR leaves commit to Ethereum addresses without the payload's stake suffix.
pub fn mmr_authority_set(id: u64, stakes: &Stakes) -> Result<AuthoritySet> {
	authority_root(id, stakes, false)
}

fn authority_root(id: u64, stakes: &Stakes, include_stake: bool) -> Result<AuthoritySet> {
	quorum(stakes.len())?;
	let mut level = Vec::with_capacity(stakes.len());
	for (key, stake) in stakes {
		let public = VerifyingKey::from_sec1_bytes(key)
			.map_err(|_| "invalid authority key")?
			.to_encoded_point(false);
		let digest = keccak(&public.as_bytes()[1..]);
		let mut leaf = [0u8; 28];
		leaf[..20].copy_from_slice(&digest[12..]);
		leaf[20..].copy_from_slice(&stake.to_le_bytes());
		level.push(keccak(if include_stake { &leaf[..] } else { &leaf[..20] }));
	}
	while level.len() > 1 {
		level = level
			.chunks(2)
			.map(|pair| {
				if pair.len() == 1 {
					return pair[0];
				}
				let mut bytes = [0; 64];
				bytes[..32].copy_from_slice(&pair[0]);
				bytes[32..].copy_from_slice(&pair[1]);
				keccak(&bytes)
			})
			.collect();
	}
	Ok(AuthoritySet { id, len: stakes.len() as u32, root: level[0] })
}

pub fn decode_justification(bytes: &[u8]) -> Result<CompactSignedCommitment> {
	if bytes.len() > MAX_PROOF_BYTES {
		return Err("oversized justification");
	}
	if bytes.first() != Some(&1) {
		return Err("unsupported BEEFY proof version");
	}
	let proof: CompactSignedCommitment = decode_all(&bytes[1..])?;
	let n = proof.validator_set_len as usize;
	quorum(n)?;
	// Upstream pack() always adds padding, including a zero byte when N % 8 == 0.
	if proof.signatures_from.len() != n / 8 + 1 {
		return Err("invalid signature bitmap length");
	}
	for i in n..proof.signatures_from.len() * 8 {
		if bit(&proof.signatures_from, i) {
			return Err("nonzero signature bitmap padding");
		}
	}
	let count = (0..n).filter(|i| bit(&proof.signatures_from, *i)).count();
	if count != proof.signatures.len() {
		return Err("signature bitmap count mismatch");
	}
	Ok(proof)
}

fn bit(bitmap: &[u8], i: usize) -> bool {
	bitmap[i / 8] & (0x80 >> (i % 8)) != 0
}

fn payload<T: Decode>(commitment: &Commitment, id: &[u8; 2]) -> Result<T> {
	let bytes = commitment
		.payload
		.iter()
		.find(|(key, _)| key == id)
		.ok_or("missing required payload")?;
	decode_all(&bytes.1)
}

/// Parse, hash, authenticate authority membership, and validate all supported fields.
/// Does NOT authenticate the signatures; only `verify` returns a finality result.
pub fn prepare(input: &Input) -> Result<Prepared> {
	let proof = decode_justification(&input.justification)?;
	let c = &proof.commitment;
	let state = &input.checkpoint;
	if c.block_number <= state.block_number {
		return Err("stale commitment");
	}
	let active = if c.validator_set_id == state.current.id {
		&state.current
	} else {
		state
			.next
			.as_ref()
			.filter(|s| s.id == c.validator_set_id)
			.ok_or("unannounced validator set")?
	};
	if active.len != proof.validator_set_len {
		return Err("validator count mismatch");
	}
	if c.payload.is_empty() || c.payload.windows(2).any(|w| w[0].0 >= w[1].0) {
		return Err("payload IDs must be unique and sorted");
	}
	let mmr_root: Hash = payload(c, b"mh")?;
	let stakes: Stakes = payload(c, b"cs")?;
	let current: AuthoritySet = payload(c, b"cb")?;
	if current != *active || authority_set(c.validator_set_id, &stakes)? != current {
		return Err("current authority commitment mismatch");
	}
	let has_next_stakes = c.payload.iter().any(|(id, _)| id == b"ns");
	let has_next_set = c.payload.iter().any(|(id, _)| id == b"nb");
	if has_next_stakes != has_next_set {
		return Err("incomplete next authority payload");
	}
	let next = if has_next_set {
		let next_stakes: Stakes = payload(c, b"ns")?;
		let next: AuthoritySet = payload(c, b"nb")?;
		let next_id = current.id.checked_add(1).ok_or("set ID overflow")?;
		if next != authority_set(next_id, &next_stakes)? {
			return Err("next authority commitment mismatch");
		}
		if current.id == state.current.id && state.next.as_ref().is_some_and(|old| old != &next) {
			return Err("conflicting next authority set");
		}
		Some(next)
	} else if current.id == state.current.id {
		state.next.clone()
	} else {
		None
	};
	if proof.signatures.len() < quorum(stakes.len())? {
		return Err("insufficient signatures");
	}
	let digest = keccak(&c.encode());
	let mut signature_iter = proof.signatures.into_iter();
	let signatures = stakes
		.iter()
		.enumerate()
		.filter_map(|(i, (key, _))| {
			if bit(&proof.signatures_from, i) {
				Some((*key, signature_iter.next().unwrap()))
			} else {
				None
			}
		})
		.collect::<Vec<_>>();
	let output = Output {
		previous_state_hash: state_hash(state),
		new_state: Checkpoint {
			genesis_hash: state.genesis_hash,
			block_number: c.block_number,
			current,
			next,
		},
		commitment_hash: digest,
		mmr_root,
		signatures_verified: signatures.len() as u32,
	};
	Ok(Prepared { digest, signatures, output })
}

/// Recover and compare the SEC1 key, checking the recovery byte as Substrate does.
/// Normalize high-S signatures and flip the recovery parity to preserve recovery semantics.
pub fn verify_signatures(digest: &Hash, signatures: &[(PublicKey, EcdsaSignature)]) -> Result<()> {
	for (key, bytes) in signatures {
		let mut sig = Signature::from_slice(&bytes[..64]).map_err(|_| "invalid ECDSA signature")?;
		let mut recid = RecoveryId::from_byte(bytes[64]).ok_or("invalid ECDSA recovery ID")?;
		if let Some(normalized) = sig.normalize_s() {
			sig = normalized;
			recid = RecoveryId::new(!recid.is_y_odd(), recid.is_x_reduced());
		}
		let recovered = VerifyingKey::recover_from_prehash(digest, &sig, recid)
			.map_err(|_| "ECDSA recovery failed")?;
		if recovered.to_encoded_point(true).as_bytes() != key {
			return Err("ECDSA signer mismatch");
		}
	}
	Ok(())
}

pub fn verify(input: &Input) -> Result<Output> {
	let prepared = prepare(input)?;
	verify_signatures(&prepared.digest, &prepared.signatures)?;
	Ok(prepared.output)
}

/// Ordered proofs extend only the supplied destination checkpoint.
#[derive(Clone, Debug, Encode, Decode)]
pub struct BatchInput {
	pub checkpoint: Checkpoint,
	pub justifications: Vec<Vec<u8>>,
	pub committee_proofs: Vec<CommitteeProof>,
}

pub fn verify_batch(batch: &BatchInput) -> Result<Output> {
	if batch.justifications.is_empty() || batch.justifications.len() > 64 {
		return Err("invalid batch size");
	}
	if batch.committee_proofs.len() != batch.justifications.len() {
		return Err("missing committee proofs");
	}
	let mut state = batch.checkpoint.clone();
	let mut last = None;
	for (justification, committee) in batch.justifications.iter().zip(&batch.committee_proofs) {
		let output = verify(&Input { checkpoint: state, justification: justification.clone() })?;
		verify_committee(justification, committee)?;
		state = output.new_state.clone();
		last = Some(output);
	}
	let mut output = last.unwrap();
	output.previous_state_hash = state_hash(&batch.checkpoint);
	Ok(output)
}

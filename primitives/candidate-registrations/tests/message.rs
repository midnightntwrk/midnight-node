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

//! Golden tests over the registration message encoding.
//!
//! These bytes are what the off-chain registration tool signs and what the
//! runtime rebuilds to verify. Nothing else holds the two sides together, so a
//! change here that is not matched off-chain invalidates every registration
//! ever posted — which is exactly what a failing golden test is for.

use hex_literal::hex;
use midnight_primitives_candidate_registrations::{
	CandidateKey, CandidateKeys, CrossChainPublicKeyBytes, CrossChainSignatureBytes,
	PoolSignatureBytes, RegistrationDatum, RegistrationMessage, RegistrationStatus,
};
use sidechain_domain::StakePoolPublicKey;

fn key(id: &[u8; 4], byte: u8, proof_byte: u8) -> CandidateKey {
	CandidateKey {
		id: *id,
		bytes: vec![byte; 32].try_into().unwrap(),
		proof: vec![proof_byte; 64].try_into().unwrap(),
	}
}

fn datum(keys: Vec<CandidateKey>) -> RegistrationDatum {
	RegistrationDatum {
		status: RegistrationStatus::Active,
		cold_pub_key: StakePoolPublicKey([1u8; 32]),
		pool_signature: PoolSignatureBytes([2u8; 64]),
		cross_chain_pub_key: CrossChainPublicKeyBytes([3u8; 33]),
		cross_chain_signature: CrossChainSignatureBytes([4u8; 65]),
		keys: CandidateKeys::try_from(keys).unwrap(),
	}
}

#[test]
fn message_encoding_is_stable() {
	let message = RegistrationMessage::from_datum(&datum(vec![
		key(b"aura", 0xaa, 0xf0),
		key(b"gran", 0xbb, 0xf1),
	]));

	// cold_pub_key (32 raw bytes)
	// || cross_chain_pub_key (compact len 33 = 0x84, then 33 bytes)
	// || keys (compact len 2 = 0x08, then id || compact len || bytes, twice)
	let expected = hex!(
		"0101010101010101010101010101010101010101010101010101010101010101"
		"84030303030303030303030303030303030303030303030303030303030303030303"
		"08"
		"6175726180aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
		"6772616e80bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
	);

	assert_eq!(message.to_signed_bytes(), expected.to_vec());
}

#[test]
fn proofs_are_projected_out_of_the_message() {
	// The message is fully determined before any proof exists; that is what
	// keeps a key's proof from having to cover itself.
	let with_one_proof = datum(vec![key(b"aura", 0xaa, 0xf0)]);
	let with_another = datum(vec![key(b"aura", 0xaa, 0x0f)]);

	assert_eq!(
		RegistrationMessage::from_datum(&with_one_proof).to_signed_bytes(),
		RegistrationMessage::from_datum(&with_another).to_signed_bytes(),
	);
}

#[test]
fn message_is_rebuilt_from_canonical_order() {
	let ordered = datum(vec![key(b"aura", 0xaa, 0xf0), key(b"gran", 0xbb, 0xf1)]);
	let shuffled = datum(vec![key(b"gran", 0xbb, 0xf1), key(b"aura", 0xaa, 0xf0)]);

	assert_eq!(
		RegistrationMessage::from_datum(&ordered).to_signed_bytes(),
		RegistrationMessage::from_datum(&shuffled).to_signed_bytes(),
	);
}

#[test]
fn every_other_part_of_a_key_triple_is_covered() {
	let baseline = RegistrationMessage::from_datum(&datum(vec![key(b"aura", 0xaa, 0xf0)]));

	let other_bytes = RegistrationMessage::from_datum(&datum(vec![key(b"aura", 0xab, 0xf0)]));
	assert_ne!(baseline.to_signed_bytes(), other_bytes.to_signed_bytes());

	let other_id = RegistrationMessage::from_datum(&datum(vec![key(b"babe", 0xaa, 0xf0)]));
	assert_ne!(baseline.to_signed_bytes(), other_id.to_signed_bytes());
}

#[test]
fn message_binds_the_pool_and_the_registrant() {
	// R2: a proof lifted into another operator's registration must not verify.
	let mut other_pool = datum(vec![key(b"aura", 0xaa, 0xf0)]);
	other_pool.cold_pub_key = StakePoolPublicKey([9u8; 32]);

	let mut other_registrant = datum(vec![key(b"aura", 0xaa, 0xf0)]);
	other_registrant.cross_chain_pub_key = CrossChainPublicKeyBytes([9u8; 33]);

	let baseline = RegistrationMessage::from_datum(&datum(vec![key(b"aura", 0xaa, 0xf0)]));
	assert_ne!(
		baseline.to_signed_bytes(),
		RegistrationMessage::from_datum(&other_pool).to_signed_bytes()
	);
	assert_ne!(
		baseline.to_signed_bytes(),
		RegistrationMessage::from_datum(&other_registrant).to_signed_bytes()
	);
}

#[test]
fn status_is_not_covered_by_the_message() {
	// The flag is attributed by the Registration policy, not by anything
	// Midnight re-checks, so it is deliberately outside the signed bytes.
	let mut inactive = datum(vec![key(b"aura", 0xaa, 0xf0)]);
	inactive.status = RegistrationStatus::Inactive;

	assert_eq!(
		RegistrationMessage::from_datum(&datum(vec![key(b"aura", 0xaa, 0xf0)])).to_signed_bytes(),
		RegistrationMessage::from_datum(&inactive).to_signed_bytes(),
	);
}

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

//! Golden tests over the registration message encodings.
//!
//! These bytes are what the off-chain registration tool signs and what the
//! runtime rebuilds to verify. Nothing else holds the two sides together, so a
//! change here that is not matched off-chain invalidates every registration
//! ever posted — which is exactly what a failing golden test is for.

use hex_literal::hex;
use midnight_primitives_candidate_registrations::{
	CandidateKeys, CrossChainKeyMessage, CrossChainPublicKeyBytes, CrossChainSignatureBytes,
	RegistrationDatum, RewardAddressBytes, SessionKeyMessage,
};
use sidechain_domain::MainchainKeyHash;
use sp_core::{H256, proof_of_possession::statement_of_ownership};

const GENESIS_HASH: H256 = H256::repeat_byte(0x9e);
const CRCH: CrossChainPublicKeyBytes = CrossChainPublicKeyBytes([3u8; 33]);

fn datum() -> RegistrationDatum {
	RegistrationDatum {
		pool_id: MainchainKeyHash([1u8; 28]),
		cross_chain_pub_key: CRCH,
		cross_chain_signature: CrossChainSignatureBytes([4u8; 64]),
		keys: CandidateKeys::default(),
		reward_address: RewardAddressBytes([0x0f; 32]),
		fixed_cost: 340_000_000,
		attributes: Default::default(),
	}
}

#[test]
fn cross_chain_key_message_encoding_is_stable() {
	let message = CrossChainKeyMessage::from_datum(&datum(), GENESIS_HASH);

	// genesis_hash (32 raw bytes)
	// || pool_id (28 raw bytes)
	// || cross_chain_pub_key (compact len 33 = 0x84, then 33 bytes)
	let expected = hex!(
		"9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e9e"
		"01010101010101010101010101010101010101010101010101010101"
		"84030303030303030303030303030303030303030303030303030303030303030303"
	);

	assert_eq!(message.to_signed_bytes(), expected.to_vec());
}

#[test]
fn the_cross_chain_key_message_binds_the_network_the_pool_and_the_registrant() {
	let baseline = CrossChainKeyMessage::from_datum(&datum(), GENESIS_HASH).to_signed_bytes();

	let other_network =
		CrossChainKeyMessage::from_datum(&datum(), H256::repeat_byte(0x01)).to_signed_bytes();
	let mut other_pool = datum();
	other_pool.pool_id = MainchainKeyHash([9u8; 28]);
	let mut other_registrant = datum();
	other_registrant.cross_chain_pub_key = CrossChainPublicKeyBytes([9u8; 33]);

	assert_ne!(baseline, other_network);
	for other in [other_pool, other_registrant] {
		assert_ne!(
			baseline,
			CrossChainKeyMessage::from_datum(&other, GENESIS_HASH).to_signed_bytes()
		);
	}
}

#[test]
fn the_cross_chain_key_message_does_not_cover_the_keys_or_the_terms() {
	// Rule 21: `crch_signature` covers only fields an Update cannot change.
	let mut updated = datum();
	updated.reward_address = RewardAddressBytes([0xf0; 32]);
	updated.fixed_cost = 1;

	assert_eq!(
		CrossChainKeyMessage::from_datum(&datum(), GENESIS_HASH),
		CrossChainKeyMessage::from_datum(&updated, GENESIS_HASH)
	);
}

#[test]
fn session_key_message_encoding_is_stable() {
	let message = SessionKeyMessage::new(CRCH);

	// blake2_256(crch), computed outside this code.
	let expected = hex!("0a807365a29ddbed43660d53e8cd09ce19d5404400d1025c4d2b0d3eeeab9476");

	assert_eq!(message.to_signed_bytes(), expected.to_vec());
}

#[test]
fn a_session_key_signs_its_message_behind_the_pop_prefix() {
	// What a key's proof of possession is a signature over: `sp_core`'s
	// statement of ownership, `b"POP_"` then the message.
	let statement = statement_of_ownership(&SessionKeyMessage::new(CRCH).to_signed_bytes());

	let expected = hex!(
		"504f505f"
		"0a807365a29ddbed43660d53e8cd09ce19d5404400d1025c4d2b0d3eeeab9476"
	);

	assert_eq!(statement, expected.to_vec());
}

#[test]
fn the_session_key_message_binds_the_node() {
	assert_ne!(
		SessionKeyMessage::new(CRCH).to_signed_bytes(),
		SessionKeyMessage::new(CrossChainPublicKeyBytes([9u8; 33])).to_signed_bytes()
	);
}

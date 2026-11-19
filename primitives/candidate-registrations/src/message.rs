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

//! The messages a registration's signatures cover (MIP rule 21, Signature).
//!
//! `crch_signature` covers [`CrossChainKeyMessage`], and each session key's
//! proof of possession covers its own [`SessionKeyMessage`]. Both are rebuilt
//! by the runtime from the node it reads, never taken from the datum.

use alloc::vec::Vec;
use parity_scale_codec::Encode;
use sidechain_domain::MainchainKeyHash;
use sp_core::H256;

use crate::types::{CrossChainPublicKeyBytes, KeyTypeIdBytes, RegistrationDatum};

/// The message `crch_signature` covers: the network, the pool and the
/// cross-chain key.
///
/// Rule 21 has `crch_signature` cover only fields the node never changes,
/// because an `Update` may change the keys but not `crch_signature` (rule 10,
/// Changes), and name the network, so a registration copied to another
/// Midnight network does not verify there. The genesis block hash names it,
/// with no circularity: genesis does not depend on any registration on
/// Cardano, so the hash is known before anyone signs.
#[derive(Clone, Debug, PartialEq, Eq, Encode)]
pub struct CrossChainKeyMessage {
	/// The hash of the Midnight chain's genesis block.
	pub genesis_hash: H256,
	/// The pool's id: blake2b-224 of its cold verification key.
	pub pool_id: MainchainKeyHash,
	/// The registrant's compressed secp256k1 key.
	pub cross_chain_pub_key: Vec<u8>,
}

impl CrossChainKeyMessage {
	/// The message a datum's `crch_signature` is supposed to cover on the
	/// network of `genesis_hash`.
	pub fn from_datum(datum: &RegistrationDatum, genesis_hash: H256) -> Self {
		Self {
			genesis_hash,
			pool_id: datum.pool_id,
			cross_chain_pub_key: datum.cross_chain_pub_key.0.to_vec(),
		}
	}

	/// The bytes `crch_signature` signs.
	pub fn to_signed_bytes(&self) -> Vec<u8> {
		self.encode()
	}
}

/// The message one session key's proof of possession covers: the node's
/// `crch` and the key's id.
///
/// Each key has its own message (rule 21), so a pool that replaces one key
/// replaces that key's proof and the others stay valid. The `crch` binds the
/// key to this node: the set holds one node per `crch`, and `crch_signature`
/// binds the `crch` to the pool and the network, so a key and its proof copied
/// into another node fail there. The id keeps one key's proof from standing
/// for it under another id.
///
/// The key itself is not in the message: the proof is verified against it.
/// The proof is a `sp_core` proof of possession, a signature by the key over
/// `b"POP_" || to_signed_bytes()`; the prefix keeps it apart from any other
/// signature by the same key.
#[derive(Clone, Debug, PartialEq, Eq, Encode)]
pub struct SessionKeyMessage {
	/// The node's `crch`: the 33-byte compressed cross-chain key, unprefixed.
	pub cross_chain_pub_key: CrossChainPublicKeyBytes,
	/// The key's 4-byte id, as `keys` names it.
	pub key_id: KeyTypeIdBytes,
}

impl SessionKeyMessage {
	/// The message the key of `key_id` proves possession over, in the node
	/// of `cross_chain_pub_key`.
	pub fn new(cross_chain_pub_key: CrossChainPublicKeyBytes, key_id: KeyTypeIdBytes) -> Self {
		Self { cross_chain_pub_key, key_id }
	}

	/// The bytes handed to the proof of possession, which prefixes them with
	/// `b"POP_"` before signing: 33 bytes of `crch`, then 4 of `key_id`.
	pub fn to_signed_bytes(&self) -> Vec<u8> {
		self.encode()
	}
}

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

//! The one message every signature and proof in a registration covers.

use alloc::vec::Vec;
use parity_scale_codec::Encode;
use sidechain_domain::StakePoolPublicKey;

use crate::types::{KeyTypeIdBytes, RegistrationDatum};

/// The canonical registration message.
///
/// Every signature and proof in a registration — the pool's, the registrant's,
/// and one per session key — is over these bytes.
///
/// The message carries key *pairs*, not the `(key, proof)` triples of the
/// datum: `proof` is projected out. That is what keeps the construction
/// non-circular, because the message is fully determined before any proof
/// exists. Everything else in a triple is covered.
#[derive(Clone, Debug, PartialEq, Eq, Encode)]
pub struct RegistrationMessage {
	/// The pool's cold verification key.
	pub cold_pub_key: StakePoolPublicKey,
	/// The registrant's compressed secp256k1 key.
	pub cross_chain_pub_key: Vec<u8>,
	/// The registered keys, ascending by id, with proofs dropped.
	pub keys: Vec<(KeyTypeIdBytes, Vec<u8>)>,
}

impl RegistrationMessage {
	/// Rebuilds the message a datum's signatures are supposed to cover.
	///
	/// `keys` is re-sorted here rather than taken as the datum ordered it, so a
	/// verifier never depends on the datum being well-formed. The canonical
	/// ordering check is a separate, earlier admission step: a datum that is
	/// out of order is rejected on its own merits, not by producing a message
	/// its signatures happen not to match.
	pub fn from_datum(datum: &RegistrationDatum) -> Self {
		let mut keys: Vec<(KeyTypeIdBytes, Vec<u8>)> =
			datum.keys.iter().map(|key| (key.id, key.bytes.to_vec())).collect();
		keys.sort_by_key(|(id, _)| *id);

		Self {
			cold_pub_key: datum.cold_pub_key.clone(),
			cross_chain_pub_key: datum.cross_chain_pub_key.0.to_vec(),
			keys,
		}
	}

	/// The bytes that are signed and proved over.
	pub fn to_signed_bytes(&self) -> Vec<u8> {
		self.encode()
	}
}

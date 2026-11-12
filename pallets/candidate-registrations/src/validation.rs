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

//! The cryptographic part of admission: everything that can be decided from a
//! datum alone, without reading or writing state.
//!
//! The order the pallet runs these in is a security property, not a
//! presentational choice. The free structural checks come first; only a datum
//! that survives all of them is worth a signature verification.
//!
//! Which key types are session keys, and which scheme each uses, is the
//! runtime's knowledge — the per-scheme verifiers here are the pieces its
//! [`SessionKeyAuthority`](midnight_primitives_candidate_registrations::SessionKeyAuthority)
//! implementation assembles a `SessionKeys` out of.

use midnight_primitives_candidate_registrations::{
	CandidateKey, CandidateKeys, CrossChainPublicKeyBytes, CrossChainSignatureBytes,
	RegistrationInvalidReason,
};
use sp_core::{
	ByteArray, crypto::KeyTypeId, ecdsa, ed25519, proof_of_possession::ProofOfPossessionVerifier,
	sr25519,
};

/// Phase A. No crypto, no state read, no state change.
///
/// Canonical means ascending by `id` with no duplicate `id`. A non-canonical
/// datum is not re-sorted into shape — it is rejected, so there is exactly one
/// key list a given registration can mean.
pub fn check_canonical_keys(keys: &CandidateKeys) -> Result<(), RegistrationInvalidReason> {
	let mut previous: Option<[u8; 4]> = None;
	for key in keys.iter() {
		match previous {
			Some(previous_id) if key.id <= previous_id => {
				return Err(RegistrationInvalidReason::NonCanonicalKeys);
			},
			_ => previous = Some(key.id),
		}
	}
	Ok(())
}

/// Phase A. Free: every session key type is present.
///
/// Run before the unknown-key-type check so an operator who forgot a key is
/// told that, rather than being told about an unrelated extra one.
pub fn check_session_keys_present(
	keys: &CandidateKeys,
	session_key_types: &[KeyTypeId],
) -> Result<(), RegistrationInvalidReason> {
	for session_key_type in session_key_types {
		if !keys.iter().any(|key| key.id == session_key_type.0) {
			return Err(RegistrationInvalidReason::MissingKeyProof(session_key_type.0));
		}
	}
	Ok(())
}

/// Phase A. Free: the datum carries nothing but session keys.
///
/// A key the runtime has no use for is also one it cannot verify, so carrying
/// it is rejected rather than ignored — otherwise a registration could be
/// padded with unchecked material that later means something.
pub fn check_no_unknown_key_types(
	keys: &CandidateKeys,
	session_key_types: &[KeyTypeId],
) -> Result<(), RegistrationInvalidReason> {
	for key in keys.iter() {
		if !session_key_types.iter().any(|known| known.0 == key.id) {
			return Err(RegistrationInvalidReason::UnknownKeyType(key.id));
		}
	}
	Ok(())
}

/// Finds the key of a given type, for a `SessionKeys` field that needs it.
pub fn find_key(
	keys: &CandidateKeys,
	id: KeyTypeId,
) -> Result<&CandidateKey, RegistrationInvalidReason> {
	keys.iter()
		.find(|key| key.id == id.0)
		.ok_or(RegistrationInvalidReason::MissingKeyProof(id.0))
}

/// Verifies an sr25519 key's proof of possession and returns the key.
pub fn verify_sr25519(
	key: &CandidateKey,
	message: &[u8],
) -> Result<sr25519::Public, RegistrationInvalidReason> {
	verify_proof_of_possession::<sr25519::Pair>(key, message)
}

/// Verifies an ed25519 key's proof of possession and returns the key.
pub fn verify_ed25519(
	key: &CandidateKey,
	message: &[u8],
) -> Result<ed25519::Public, RegistrationInvalidReason> {
	verify_proof_of_possession::<ed25519::Pair>(key, message)
}

/// Verifies an ECDSA key's proof of possession and returns the key.
pub fn verify_ecdsa(
	key: &CandidateKey,
	message: &[u8],
) -> Result<ecdsa::Public, RegistrationInvalidReason> {
	verify_proof_of_possession::<ecdsa::Pair>(key, message)
}

fn verify_proof_of_possession<P>(
	key: &CandidateKey,
	message: &[u8],
) -> Result<P::Public, RegistrationInvalidReason>
where
	P: ProofOfPossessionVerifier,
	P::Public: ByteArray,
	P::ProofOfPossession: ByteArray,
{
	// A key or proof of the wrong length for its scheme is, for an operator's
	// purposes, indistinguishable from one that does not verify.
	let public = P::Public::from_slice(key.bytes.as_slice())
		.map_err(|_| RegistrationInvalidReason::InvalidKeyProof(key.id))?;
	let proof = P::ProofOfPossession::from_slice(key.proof.as_slice())
		.map_err(|_| RegistrationInvalidReason::InvalidKeyProof(key.id))?;

	if P::verify_proof_of_possession(message, &proof, &public) {
		Ok(public)
	} else {
		Err(RegistrationInvalidReason::InvalidKeyProof(key.id))
	}
}

/// Verifies the registrant's signature over the registration message.
///
/// The cross-chain key needs no session-key arm: this signature over the same
/// message already proves possession of it. It is a plain signature rather
/// than a proof of possession because the pool's signature — produced by
/// Cardano tooling over the same bytes — is one too, and both come out of the
/// same off-chain registration tool.
pub fn verify_cross_chain_signature(
	public_key: &CrossChainPublicKeyBytes,
	signature: &CrossChainSignatureBytes,
	message: &[u8],
) -> Result<(), RegistrationInvalidReason> {
	let public = ecdsa::Public::from_raw(public_key.0);
	let signature = ecdsa::Signature::from_raw(signature.0);

	if <ecdsa::Pair as sp_core::Pair>::verify(&signature, message, &public) {
		Ok(())
	} else {
		Err(RegistrationInvalidReason::InvalidCrossChainSignature)
	}
}

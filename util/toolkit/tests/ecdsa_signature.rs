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

//! Ledger-acceptance tests for ECDSA unshielded (NIGHT) signatures.
//!
//! These exercise `SignatureKind::signature_verify` — the exact primitive the ledger runs inside
//! `Transaction::well_formed` to accept an unshielded spend's signature (`S::signature_verify(&
//! data_to_sign, owner, sig)`). It dispatches on the wrapped `SignatureVerifyingKey`/`Signature`
//! scheme, so this proves the toolkit's `UnshieldedWallet` produces a wrapped ECDSA (verifying key,
//! signature) pair that the ledger accepts — without a running node. ECDSA is a ledger-9 feature,
//! so these use the crate-level (ledger-9) types. The rest of `well_formed` (balancing, proofs) is
//! scheme-independent and covered elsewhere; a full build+submit lives in the devnet-backed suite.

use midnight_node_ledger_helpers::{
	DefaultDB, Signature, SignatureKind, SignatureVerifyingKey, UnshieldedSignatureScheme,
	UnshieldedWallet, WalletSeed,
};
use rand::rngs::OsRng;

/// Run the ledger's unshielded-signature acceptance check.
fn ledger_accepts(msg: &[u8], vk: SignatureVerifyingKey, sig: &Signature) -> bool {
	<Signature as SignatureKind<DefaultDB>>::signature_verify::<()>(msg, vk, sig)
}

fn wallet(scheme: UnshieldedSignatureScheme) -> UnshieldedWallet {
	UnshieldedWallet::new(WalletSeed::Short([0x42; 16]), scheme)
}

/// The ledger accepts an ECDSA signature produced by the toolkit wallet, and rejects it against a
/// different message.
#[test]
fn ledger_accepts_ecdsa_unshielded_signature() {
	let w = wallet(UnshieldedSignatureScheme::Ecdsa);
	let msg = b"unshielded spend intent";
	let sig = w.sign(&mut OsRng, msg);

	assert!(
		ledger_accepts(msg, w.verifying_key(), &sig),
		"ledger must accept a valid ECDSA unshielded signature",
	);
	assert!(
		!ledger_accepts(b"tampered intent", w.verifying_key(), &sig),
		"ledger must reject an ECDSA signature over a different message",
	);
}

/// The dispatch rejects a scheme mismatch in either direction (an ECDSA signature against a Schnorr
/// verifying key, and vice versa).
#[test]
fn ledger_rejects_cross_scheme_signature() {
	let msg = b"unshielded spend intent";

	let ecdsa = wallet(UnshieldedSignatureScheme::Ecdsa);
	let ecdsa_sig = ecdsa.sign(&mut OsRng, msg);
	let schnorr = wallet(UnshieldedSignatureScheme::Schnorr);
	let schnorr_sig = schnorr.sign(&mut OsRng, msg);

	assert!(
		!ledger_accepts(msg, schnorr.verifying_key(), &ecdsa_sig),
		"a Schnorr key must not accept an ECDSA signature",
	);
	assert!(
		!ledger_accepts(msg, ecdsa.verifying_key(), &schnorr_sig),
		"an ECDSA key must not accept a Schnorr signature",
	);
}

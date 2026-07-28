// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! ECDSA `UnshieldedWallet` tests.
//!
//! ECDSA is a ledger-9+ feature (pre-9 the key types are `unimplemented!()` stubs). The shared
//! `versions/common` is compiled once per generation, so this module is included *solely* by the
//! `ledger_9` block (see `lib.rs`) — the tests exist and run only where ECDSA is real, avoiding a
//! misleading `ledger_7::…::ecdsa_… ok`. The whole `common::wallet` module (hence `UnshieldedWallet`
//! and every test here) is `can-panic`-gated, so the module gate on the `mod` declaration covers all
//! of them.

use super::UnshieldedWallet;

/// Fixed, arbitrary root seed — stable so derivations are reproducible.
fn seed() -> super::WalletSeed {
	super::WalletSeed::Short([0x42; 16])
}

/// An ECDSA `UnshieldedWallet` survives a tagged (`unshielded-wallet[v2]`) serialization
/// round-trip: the address and full keypair are preserved, and the restored signing key still
/// produces verifiable signatures.
#[test]
fn ecdsa_wallet_serialization_roundtrip() {
	use super::{UnshieldedSignatureScheme, deserialize, serialize};

	let wallet = UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Ecdsa);
	let bytes = serialize(&wallet).expect("serialize ECDSA wallet");
	let restored: UnshieldedWallet = deserialize(&bytes[..]).expect("deserialize ECDSA wallet");

	assert_eq!(restored.user_address, wallet.user_address);

	let (orig_vk, _) = wallet.ecdsa_keys().expect("original has ECDSA keys");
	let (vk, sk) = restored.ecdsa_keys().expect("restored keeps ECDSA keys");
	assert_eq!(vk, orig_vk, "verifying key must survive the round-trip");

	let msg = b"post-roundtrip signing";
	assert!(vk.verify(msg, &sk.sign(msg)), "restored signing key must still sign verifiably");
}

/// An ECDSA wallet's contract-maintenance verifying key is the ECDSA variant built from its
/// verifying key — proves `maintenance_verifying_key()` dispatches by scheme, which is what lets
/// a committee member authorize maintenance/deploy with ECDSA.
#[test]
fn ecdsa_maintenance_verifying_key_matches_scheme() {
	use super::{UnshieldedSignatureScheme, maintenance_verifying_key_ecdsa};

	let wallet = UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Ecdsa);
	let (vk, _) = wallet.ecdsa_keys().expect("ECDSA wallet has key material");
	assert_eq!(
		wallet.maintenance_verifying_key(),
		Some(maintenance_verifying_key_ecdsa(vk.clone())),
	);
}

/// Golden vector / regression anchor for ECDSA address derivation over the *full HD path*
/// (root seed → `m/44'/2400'/0'/4/0` leaf → key → address). This value is self-generated: the
/// published MIP-0003 vectors exercise the uniform-bytes→key→address steps (see
/// [`ecdsa_address_mip0003_conformance`]) but not the root-seed→leaf HD mapping, so there is no
/// official vector to pin the whole path against. `seed()` is arbitrary but stable.
#[test]
fn ecdsa_address_golden_vector() {
	use super::UnshieldedSignatureScheme;

	const EXPECTED_ECDSA_ADDRESS_HEX: &str =
		"953cab8c90974f2b9e6d03d6932be3488a27fa83c76790cb7116fa1980c81512";

	let actual = hex::encode(
		UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Ecdsa).user_address.0.0,
	);

	assert_eq!(actual, EXPECTED_ECDSA_ADDRESS_HEX);
}

/// MIP-0003 conformance for the ECDSA address *formula* — `SHA-256("midnight:ecdsa:" ‖
/// compressed-SEC1-vk)` applied to `UserAddress::from(ecdsa::VerifyingKey)`. The vectors come
/// from the official `midnight-wallet` `spec-reference` reference implementation and generator
/// (authored by the MIP author); each `uniform_bytes` is fed *directly* as the secp256k1 scalar
/// (i.e. it is the HD-path leaf output, NOT a root wallet seed) — which is exactly what
/// `UnshieldedWallet::from_bytes_ecdsa` consumes. Pinning these guarantees byte-for-byte interop
/// with the Wallet SDK's derivation.
#[test]
fn ecdsa_address_mip0003_conformance() {
	// (uniform_bytes, expected 32-byte unshielded address hex)
	let cases: [([u8; 32], &str); 3] = [
		([0x01; 32], "1139359859a68b29bec3120d85691f21a56593a27d4ee15c10aa059d0699eb3e"),
		([0x02; 32], "9dd08a454c354133504bddd366db239ea169db8454ebffb9b7718662b6a6e73d"),
		([0x04; 32], "7b62f3aeaf1e9df17474a4ab2dcd4b6ca4d832499d88b3b60fb2a35d69d02933"),
	];

	for (uniform_bytes, expected) in cases {
		let actual =
			hex::encode(UnshieldedWallet::from_bytes_ecdsa(uniform_bytes).user_address.0.0);
		assert_eq!(actual, expected, "uniform bytes {uniform_bytes:02x?}");
	}
}

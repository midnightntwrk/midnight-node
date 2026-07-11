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

use super::super::{
	ArenaKey, DB, DerivationPath, DeriveSeed, Deserializable, HRP_CONSTANT,
	HRP_CREDENTIAL_UNSHIELDED, HashOutput, IntentHash, IntoWalletAddress, Loader,
	MaintenanceVerifyingKey, Role, Serializable, Signature, SignatureVerifyingKey, SigningKeyEcdsa,
	SigningKeySchnorr, Storable, Tagged, TransactionSigningKey, UserAddress, VerifyingKeyEcdsa,
	VerifyingKeySchnorr, WalletAddress, WalletSeed, deserialize_untagged,
	maintenance_verifying_key, maintenance_verifying_key_ecdsa, serialize_untagged,
	signature_verifying_key, signature_verifying_key_ecdsa, transaction_signature,
	transaction_signature_ecdsa, transaction_signing_key, transaction_signing_key_ecdsa,
};
use hex::FromHexError;
use rand::{CryptoRng, Rng};
use std::num::ParseIntError;

#[derive(Copy, Clone, Debug)]
pub struct UtxoId {
	pub intent_hash: IntentHash,
	pub output_number: u32,
}

impl core::fmt::Display for UtxoId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{}#{}",
			hex::encode(serialize_untagged(&self.intent_hash).map_err(|_| std::fmt::Error)?),
			self.output_number
		)
	}
}

#[derive(Debug, thiserror::Error)]
pub enum UtxoIdParseError {
	#[error("wrong number of parts (!= 2)")]
	WrongNumberOfParts,
	#[error("hex decode error")]
	HexDecodeError(FromHexError),
	#[error("deserialization error")]
	DeserializationError(std::io::Error),
	#[error("parse int error")]
	ParseIntError(ParseIntError),
}

impl std::str::FromStr for UtxoId {
	type Err = UtxoIdParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (intent_hash_hex, output_number_str) =
			s.split_once('#').ok_or(UtxoIdParseError::WrongNumberOfParts)?;
		let intent_hash_bytes =
			hex::decode(intent_hash_hex).map_err(UtxoIdParseError::HexDecodeError)?;
		let intent_hash = deserialize_untagged(&mut intent_hash_bytes.as_slice())
			.map_err(UtxoIdParseError::DeserializationError)?;
		let output_number = output_number_str.parse().map_err(UtxoIdParseError::ParseIntError)?;

		Ok(Self { intent_hash, output_number })
	}
}

/// Signature scheme backing an unshielded (NIGHT) identity.
///
/// Schnorr is the historical default. ECDSA is only representable from ledger 9 on; selecting it
/// against an earlier generation panics deep in [`SigningKeyEcdsa`] — callers (the toolkit CLI)
/// guard against that and surface a clear error instead.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum UnshieldedSignatureScheme {
	#[default]
	Schnorr,
	Ecdsa,
}

/// An unshielded (NIGHT) wallet identity.
///
/// `keys` is `None` for address-only wallets (parsed from a bech32 address or a bare
/// [`UserAddress`]); those can name a recipient but cannot sign. The tag is `[v2]` because the
/// on-disk layout changed when the flat Schnorr fields became the scheme enum below — any tagged
/// (de)serialization, including the `fork_*` migrations, must reject the old layout.
#[derive(Clone, Storable, Serializable)]
#[tag = "unshielded-wallet[v2]"]
#[storable(base)]
pub struct UnshieldedWallet {
	pub user_address: UserAddress,
	keys: Option<UnshieldedWalletKeys>,
}

/// The per-scheme key material behind an [`UnshieldedWallet`]. The `signing_key` is `Option`
/// so a wallet can hold only the public half.
#[derive(Clone, Serializable)]
#[tag = "unshielded-wallet-keys[v1]"]
// For ledger 7/8, the ECDSA variant of this enum is size 1 - so we ignore the clippy warning here
#[allow(clippy::large_enum_variant)]
pub enum UnshieldedWalletKeys {
	Schnorr { verifying_key: VerifyingKeySchnorr, signing_key: Option<SigningKeySchnorr> },
	Ecdsa { verifying_key: VerifyingKeyEcdsa, signing_key: Option<SigningKeyEcdsa> },
}

impl std::fmt::Debug for UnshieldedWallet {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut debug_struct = f.debug_struct("UnshieldedWallet");
		debug_struct.field("user_address", &self.user_address);

		match &self.keys {
			Some(UnshieldedWalletKeys::Schnorr { verifying_key, .. }) => {
				debug_struct.field("verifying_key(schnorr)", verifying_key);
			},
			Some(UnshieldedWalletKeys::Ecdsa { verifying_key, .. }) => {
				debug_struct.field("verifying_key(ecdsa)", verifying_key);
			},
			None => {
				debug_struct.field("verifying_key", &Option::<()>::None);
			},
		}

		debug_struct.field("signing_key", &"REDACTED").finish()
	}
}

impl DeriveSeed for UnshieldedWallet {}

#[cfg(feature = "can-panic")]
impl IntoWalletAddress for UnshieldedWallet {
	fn address(&self, network_id: &str) -> WalletAddress {
		let hrp_string = format!(
			"{HRP_CONSTANT}_{HRP_CREDENTIAL_UNSHIELDED}{}",
			Self::network_suffix(network_id)
		);
		let hrp = bech32::Hrp::parse(&hrp_string)
			.unwrap_or_else(|err| panic!("Error while bech32 parsing: {err}"));

		let data = &self.user_address.0.0;

		WalletAddress::new(hrp, data.to_vec())
	}
}

impl UnshieldedWallet {
	fn from_bytes_schnorr(derived_seed: [u8; 32]) -> Self {
		let sk = SigningKeySchnorr::from_bytes(&derived_seed)
			.unwrap_or_else(|err| panic!("Error calculating the `SigningKey`: {err}"));
		let vk = sk.verifying_key();
		let user_address: UserAddress = vk.clone().into();

		Self {
			user_address,
			keys: Some(UnshieldedWalletKeys::Schnorr { verifying_key: vk, signing_key: Some(sk) }),
		}
	}

	fn from_bytes_ecdsa(derived_seed: [u8; 32]) -> Self {
		let sk = SigningKeyEcdsa::from_bytes(&derived_seed)
			.unwrap_or_else(|err| panic!("Error calculating the ECDSA `SigningKey`: {err}"));
		let vk = sk.verifying_key();
		let user_address: UserAddress = vk.clone().into();

		Self {
			user_address,
			keys: Some(UnshieldedWalletKeys::Ecdsa { verifying_key: vk, signing_key: Some(sk) }),
		}
	}

	/// Default (Schnorr) unshielded wallet derived at `m/44'/2400'/0'/0/0`.
	pub fn default(root_seed: WalletSeed) -> Self {
		let path = DerivationPath::default_for_role(Role::UnshieldedExternal);
		let derived_seed = Self::derive_seed(root_seed, &path);

		Self::from_bytes_schnorr(derived_seed)
	}

	/// Build an unshielded wallet for the given signature `scheme`. Schnorr derives at the
	/// external role (`.../0/0`); ECDSA derives at the dedicated ECDSA role (`.../4/0`).
	pub fn new(root_seed: WalletSeed, scheme: UnshieldedSignatureScheme) -> Self {
		let role = match scheme {
			UnshieldedSignatureScheme::Schnorr => Role::UnshieldedExternal,
			UnshieldedSignatureScheme::Ecdsa => Role::Ecdsa,
		};
		let path = DerivationPath::default_for_role(role);
		let derived_seed = Self::derive_seed(root_seed, &path);

		match scheme {
			UnshieldedSignatureScheme::Schnorr => Self::from_bytes_schnorr(derived_seed),
			UnshieldedSignatureScheme::Ecdsa => Self::from_bytes_ecdsa(derived_seed),
		}
	}

	/// The verifying key wrapped in this ledger generation's signature-verifying-key type
	/// (the concrete Schnorr key on ledger 7/8, the scheme enum on ledger 9).
	pub fn verifying_key(&self) -> SignatureVerifyingKey {
		match &self.keys {
			Some(UnshieldedWalletKeys::Schnorr { verifying_key, .. }) => {
				signature_verifying_key(verifying_key.clone())
			},
			Some(UnshieldedWalletKeys::Ecdsa { verifying_key, .. }) => {
				signature_verifying_key_ecdsa(verifying_key.clone())
			},
			None => panic!("Missing verifying key for the `UnshieldedWallet`"),
		}
	}

	/// The verifying key wrapped in this ledger generation's contract-maintenance-authority key
	/// type, dispatched by scheme. Used to build/deploy contract-maintenance committees.
	pub fn maintenance_verifying_key(&self) -> MaintenanceVerifyingKey {
		match &self.keys {
			Some(UnshieldedWalletKeys::Schnorr { verifying_key, .. }) => {
				maintenance_verifying_key(verifying_key.clone())
			},
			Some(UnshieldedWalletKeys::Ecdsa { verifying_key, .. }) => {
				maintenance_verifying_key_ecdsa(verifying_key.clone())
			},
			None => panic!("Missing verifying key for the `UnshieldedWallet`"),
		}
	}

	/// The signing key wrapped in this ledger generation's transaction-signing-key type.
	pub fn transaction_signing_key(&self) -> TransactionSigningKey {
		match &self.keys {
			Some(UnshieldedWalletKeys::Schnorr { signing_key: Some(sk), .. }) => {
				transaction_signing_key(sk)
			},
			Some(UnshieldedWalletKeys::Ecdsa { signing_key: Some(sk), .. }) => {
				transaction_signing_key_ecdsa(sk)
			},
			_ => panic!("Missing `SigningKey` for the `UnshieldedWallet`"),
		}
	}

	/// Sign `msg`, producing this ledger generation's signature type. Schnorr consumes `rng`;
	/// ECDSA signs deterministically (RFC 6979) and ignores it.
	pub fn sign(&self, rng: &mut (impl Rng + CryptoRng), msg: &[u8]) -> Signature {
		match &self.keys {
			Some(UnshieldedWalletKeys::Schnorr { signing_key: Some(sk), .. }) => {
				transaction_signature(sk.sign(rng, msg))
			},
			Some(UnshieldedWalletKeys::Ecdsa { signing_key: Some(sk), .. }) => {
				transaction_signature_ecdsa(sk.sign(msg))
			},
			_ => panic!("Missing `SigningKey` for the `UnshieldedWallet`"),
		}
	}

	/// The raw Schnorr signing key, for the Schnorr-only contract-maintenance committee and
	/// key-serialization paths. Panics for a non-Schnorr or address-only wallet.
	#[cfg(feature = "can-panic")]
	pub fn signing_key(&self) -> &SigningKeySchnorr {
		match &self.keys {
			Some(UnshieldedWalletKeys::Schnorr { signing_key: Some(sk), .. }) => sk,
			_ => panic!("Missing Schnorr `SigningKey` for the `UnshieldedWallet`"),
		}
	}

	/// Test-only access to the raw ECDSA key material, so tests can drive sign/verify against the
	/// wallet's actual keypair. `None` for non-ECDSA or address-only wallets.
	#[cfg(test)]
	pub(crate) fn ecdsa_keys(&self) -> Option<(&VerifyingKeyEcdsa, &SigningKeyEcdsa)> {
		match &self.keys {
			Some(UnshieldedWalletKeys::Ecdsa { verifying_key, signing_key: Some(sk) }) => {
				Some((verifying_key, sk))
			},
			_ => None,
		}
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum UnshieldedAddressParseError {
	DecodeError(bech32::DecodeError),
	InvalidHrpPrefix,
	InvalidHrpCredential,
	AddressNotUnshielded,
	InvalidDataLen(usize),
	Other,
}

impl TryFrom<&WalletAddress> for UnshieldedWallet {
	type Error = UnshieldedAddressParseError;

	fn try_from(address: &WalletAddress) -> Result<Self, Self::Error> {
		let hrp = address.human_readable_part();
		let prefix_parts = hrp.split('_').collect::<Vec<&str>>();

		prefix_parts
			.first()
			.filter(|c| *c == &HRP_CONSTANT)
			.ok_or(UnshieldedAddressParseError::InvalidHrpPrefix)?;

		let hrp_credential = prefix_parts
			.get(1)
			.ok_or(UnshieldedAddressParseError::InvalidHrpCredential)?
			.to_string();
		if hrp_credential != HRP_CREDENTIAL_UNSHIELDED {
			return Err(UnshieldedAddressParseError::AddressNotUnshielded);
		}

		let address_data: [u8; 32] = address
			.data()
			.as_slice()
			.try_into()
			.map_err(|_| UnshieldedAddressParseError::InvalidDataLen(address.data().len()))?;

		Ok(Self { user_address: UserAddress(HashOutput(address_data)), keys: None })
	}
}

impl From<UserAddress> for UnshieldedWallet {
	fn from(user_address: UserAddress) -> Self {
		Self { user_address, keys: None }
	}
}

// `derive_seed`/`DerivationPath` require the `can-panic` feature (see `hd.rs`).
#[cfg(all(test, feature = "can-panic"))]
mod tests {
	use super::super::super::WalletSeed;
	use super::{UnshieldedSignatureScheme, UnshieldedWallet};

	// `common` is compiled once per ledger generation. ECDSA is real only on ledger 9; on 7/8 the
	// key types are `unimplemented!()` stubs that panic when touched (see `ecdsa_unimpl.rs`), so
	// ECDSA bodies must compile everywhere but run only on 9. `LEDGER_VERSION` lives in the
	// enclosing version module (`lib.rs`), four modules up.
	const LEDGER_GENERATION: u32 = super::super::super::super::LEDGER_VERSION;

	/// Fixed, arbitrary root seed — stable so the golden vector is reproducible.
	fn seed() -> WalletSeed {
		WalletSeed::Short([0x42; 16])
	}

	/// `new(.., Schnorr)` must equal the historical `default(..)`. Runs on every generation.
	#[test]
	fn schnorr_new_matches_default() {
		assert_eq!(
			UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Schnorr).user_address,
			UnshieldedWallet::default(seed()).user_address,
		);
	}

	/// An ECDSA `UnshieldedWallet` survives a tagged (`unshielded-wallet[v2]`) serialization
	/// round-trip: the address and full keypair are preserved, and the restored signing key still
	/// produces verifiable signatures.
	#[test]
	fn ecdsa_wallet_serialization_roundtrip() {
		if LEDGER_GENERATION != 9 {
			return;
		}
		use super::super::super::{deserialize, serialize};

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
		if LEDGER_GENERATION != 9 {
			return;
		}
		use super::super::super::maintenance_verifying_key_ecdsa;

		let wallet = UnshieldedWallet::new(seed(), UnshieldedSignatureScheme::Ecdsa);
		let (vk, _) = wallet.ecdsa_keys().expect("ECDSA wallet has key material");
		assert!(wallet.maintenance_verifying_key() == maintenance_verifying_key_ecdsa(vk.clone()));
	}

	/// Golden vector / regression anchor for ECDSA address derivation over the *full HD path*
	/// (root seed → `m/44'/2400'/0'/4/0` leaf → key → address) on ledger 9. This value is
	/// self-generated: the published MIP-0003 vectors exercise the uniform-bytes→key→address steps
	/// (see [`ecdsa_address_mip0003_conformance`]) but not the root-seed→leaf HD mapping, so there is
	/// no official vector to pin the whole path against. `seed()` is arbitrary but stable.
	#[test]
	fn ecdsa_address_golden_vector() {
		if LEDGER_GENERATION != 9 {
			return;
		}
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
	/// [`UnshieldedWallet::from_bytes_ecdsa`] consumes. Pinning these guarantees byte-for-byte
	/// interop with the Wallet SDK's derivation.
	#[test]
	fn ecdsa_address_mip0003_conformance() {
		if LEDGER_GENERATION != 9 {
			return;
		}
		// (uniform_bytes, expected 32-byte unshielded address hex)
		let cases: [([u8; 32], &str); 3] = [
			(
				[0x01; 32],
				"1139359859a68b29bec3120d85691f21a56593a27d4ee15c10aa059d0699eb3e",
			),
			(
				[0x02; 32],
				"9dd08a454c354133504bddd366db239ea169db8454ebffb9b7718662b6a6e73d",
			),
			(
				[0x04; 32],
				"7b62f3aeaf1e9df17474a4ab2dcd4b6ca4d832499d88b3b60fb2a35d69d02933",
			),
		];

		for (uniform_bytes, expected) in cases {
			let actual =
				hex::encode(UnshieldedWallet::from_bytes_ecdsa(uniform_bytes).user_address.0.0);
			assert_eq!(actual, expected, "uniform bytes {uniform_bytes:02x?}");
		}
	}
}

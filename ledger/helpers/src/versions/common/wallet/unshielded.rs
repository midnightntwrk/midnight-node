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
	HRP_CREDENTIAL_UNSHIELDED, HashOutput, IntentHash, IntoWalletAddress, Loader, Role,
	Serializable, Signature, SignatureVerifyingKey, SigningKeyEcdsa, SigningKeySchnorr, Storable,
	Tagged, TransactionSigningKey, UserAddress, VerifyingKeyEcdsa, VerifyingKeySchnorr,
	WalletAddress, WalletSeed, deserialize_untagged, serialize_untagged, signature_verifying_key,
	signature_verifying_key_ecdsa, transaction_signature, transaction_signature_ecdsa,
	transaction_signing_key, transaction_signing_key_ecdsa,
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

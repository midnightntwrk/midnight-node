// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

#![cfg(feature = "can-panic")]

use super::super::{
	CoinPublicKey, DB, DerivationPath, DeriveSeed, Deserializable, EncryptionPublicKey,
	HRP_CONSTANT, HRP_CREDENTIAL_SHIELDED, HRP_CREDENTIAL_SHIELDED_ESK, HashOutput,
	IntoWalletAddress, Role, SecretKeys, Seed, Serializable, WalletAddress, WalletSeed,
	WalletState,
};
use bech32::{Bech32m, Hrp};
use derive_where::derive_where;

#[derive(Debug)]
#[derive_where(Clone)]
pub struct ShieldedWallet<D: DB + Clone> {
	pub state: WalletState<D>,
	pub coin_public_key: CoinPublicKey,
	pub enc_public_key: EncryptionPublicKey,
	secret_keys: Option<SecretKeys>,
}

impl<D: DB + Clone> DeriveSeed for ShieldedWallet<D> {}

impl<D: DB + Clone> IntoWalletAddress for ShieldedWallet<D> {
	fn address(&self, network_id: &str) -> WalletAddress {
		let hrp_string =
			format!("{HRP_CONSTANT}_{HRP_CREDENTIAL_SHIELDED}{}", Self::network_suffix(network_id));
		let hrp = bech32::Hrp::parse(&hrp_string)
			.unwrap_or_else(|err| panic!("Error while bech32 parsing: {err}"));
		let coin_pub_key = self.coin_public_key.0.0;
		let mut enc_pub_key = Vec::new();
		Serializable::serialize(&self.enc_public_key, &mut enc_pub_key)
			.unwrap_or_else(|err| panic!("Error Serializing `enc_public_key`: {err}"));
		let data = [&coin_pub_key[..], &[3, 0], &enc_pub_key[..]].concat();

		WalletAddress::new(hrp, data)
	}
}

impl<D: DB + Clone> ShieldedWallet<D> {
	fn from_seed(derived_seed: [u8; 32]) -> Self {
		let sks = SecretKeys::from(Into::<Seed>::into(derived_seed));
		let coin_public_key = sks.coin_public_key();
		let enc_public_key = sks.enc_public_key();
		let state = WalletState::new();

		Self { state, coin_public_key, enc_public_key, secret_keys: Some(sks) }
	}

	pub fn default(root_seed: WalletSeed) -> Self {
		let role = Role::Zswap;
		let path = DerivationPath::default_for_role(role);
		let derived_seed = Self::derive_seed(root_seed, &path);

		Self::from_seed(derived_seed)
	}

	pub fn from_path(root_seed: WalletSeed, path: &DerivationPath) -> Self {
		let derived_seed = Self::derive_seed(root_seed, path);
		Self::from_seed(derived_seed)
	}

	pub fn from_pub_keys(
		coin_public_key: CoinPublicKey,
		enc_public_key: EncryptionPublicKey,
	) -> Self {
		let state = WalletState::new();

		Self { state, coin_public_key, enc_public_key, secret_keys: None }
	}

	#[cfg(feature = "can-panic")]
	pub fn secret_keys(&self) -> &SecretKeys {
		self.secret_keys.as_ref().expect("Missing `SecretKeys` for the `ShieldedWallet")
	}

	/// Bech32m-encoded shielded encryption secret key, aka. "viewing key" sent from the wallet
	/// to the Indexer.
	pub fn viewing_key(&self, network_id: &str) -> String {
		let hrp = format!(
			"{HRP_CONSTANT}_{HRP_CREDENTIAL_SHIELDED_ESK}{}",
			Self::network_suffix(network_id)
		);
		let hrp = Hrp::parse(&hrp).expect("HRP for encryption secret key can be parsed");

		let mut data = Vec::with_capacity(64);
		self.secret_keys()
			.encryption_secret_key
			.serialize(&mut data)
			.expect("encryption secret key can be serialized");

		bech32::encode::<Bech32m>(hrp, &data).expect("viewing key can be bech32 encoded")
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum ShieldedAddressParseError {
	DecodeError(bech32::DecodeError),
	InvalidHrpPrefix,
	InvalidHrpCredential,
	AddressNotShielded,
	Other,
}

impl<D: DB + Clone> TryFrom<&WalletAddress> for ShieldedWallet<D> {
	type Error = ShieldedAddressParseError;

	fn try_from(address: &WalletAddress) -> Result<ShieldedWallet<D>, ShieldedAddressParseError> {
		let hrp = address.human_readable_part();
		let data = address.data();

		let prefix_parts = hrp.as_str().split('_').collect::<Vec<&str>>();

		prefix_parts
			.first()
			.filter(|c| *c == &HRP_CONSTANT)
			.ok_or(ShieldedAddressParseError::InvalidHrpPrefix)?;

		let hrp_credential = prefix_parts
			.get(1)
			.ok_or(ShieldedAddressParseError::InvalidHrpCredential)?
			.to_string();

		if hrp_credential != HRP_CREDENTIAL_SHIELDED {
			return Err(ShieldedAddressParseError::AddressNotShielded);
		}

		assert_eq!(data.len(), 66);
		let coin_bytes = &data[..32];
		assert_eq!(&data[32..34], &[3, 0]);
		let mut enc_bytes = &data[34..];

		let coin_public_key = CoinPublicKey(HashOutput(coin_bytes.try_into().unwrap()));
		let enc_public_key: EncryptionPublicKey = Deserializable::deserialize(&mut enc_bytes, 0)
			.unwrap_or_else(|err| panic!("Error deserializing `EncryptionPublicKey`: {err}"));

		Ok(Self::from_pub_keys(coin_public_key, enc_public_key))
	}
}

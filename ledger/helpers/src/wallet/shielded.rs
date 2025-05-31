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

use crate::{
	CoinPublicKey, DB, DerivationPath, DeriveSeed, Deserializable, EncryptionPublicKey,
	HRP_CONSTANT, HRP_CREDENTIAL_SHIELDED, HashOutput, IntoWalletAddress, NetworkId, Role,
	SecretKeys, Seed, Serializable, WalletAddress, WalletSeed, WalletState,
};
use bech32::Bech32m;
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
	fn address(self, network_id: NetworkId) -> WalletAddress {
		let hrp_string =
			format!("{}_{}{}", HRP_CONSTANT, HRP_CREDENTIAL_SHIELDED, Self::network(network_id));
		let hrp = bech32::Hrp::parse(&hrp_string)
			.unwrap_or_else(|err| panic!("Error while bech32 parsing: {}", err));

		let coin_pub_key = self.coin_public_key.0.0;
		let mut enc_pub_key = Vec::new();
		Serializable::serialize(&self.enc_public_key, &mut enc_pub_key)
			.unwrap_or_else(|err| panic!("Error Serializing `enc_public_key`: {}", err));
		let data = [&coin_pub_key[..], &enc_pub_key[..]].concat();

		let address = bech32::encode::<Bech32m>(hrp, &data)
			.unwrap_or_else(|err| panic!("Error bech32 encoding: {}", err));

		WalletAddress(address)
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
		let derived_seed = Self::derive_seed(root_seed, path);

		Self::from_seed(derived_seed)
	}

	pub fn from_path(root_seed: WalletSeed, path: String) -> Self {
		let path = DerivationPath::new(path);
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

	pub fn secret_keys(&self) -> &SecretKeys {
		self.secret_keys.as_ref().expect("Missing `SecretKeys` for the `ShieldedWallet")
	}
}

impl<D: DB + Clone> From<WalletAddress> for ShieldedWallet<D> {
	fn from(address: WalletAddress) -> Self {
		let (hrp, data) = bech32::decode(&address.0).unwrap_or_else(|err| {
			panic!("Error while bech32 decoding {:?} to `ShieldedWallet`: {}", address, err)
		});

		let prefix_parts = hrp.as_str().split('_').collect::<Vec<&str>>();

		prefix_parts
			.first()
			.filter(|c| *c == &HRP_CONSTANT)
			.expect("Error while parsing bech32 `hrp`");

		let hrp_credential = prefix_parts
			.get(1)
			.expect("Error while parsing bech32 `hrp_credential`")
			.to_string();
		assert!(
			hrp_credential == HRP_CREDENTIAL_SHIELDED,
			"Invalid address for a `ShieldedWallet`"
		);

		let coin_public_key = CoinPublicKey(HashOutput(data[..32].try_into().unwrap()));
		let enc_public_key: EncryptionPublicKey = Deserializable::deserialize(&mut &data[32..], 0)
			.unwrap_or_else(|err| panic!("Error deserializing `EncryptionPublicKey`: {}", err));

		Self::from_pub_keys(coin_public_key, enc_public_key)
	}
}

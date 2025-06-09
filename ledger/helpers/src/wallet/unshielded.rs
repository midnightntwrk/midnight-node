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
	DerivationPath, DeriveSeed, Deserializable, HRP_CONSTANT, HRP_CREDENTIAL_UNSHIELDED,
	IntoWalletAddress, NetworkId, Role, Serializable, SigningKey, VerifyingKey, WalletAddress,
	WalletSeed,
};
use bech32::Bech32m;

#[derive(Clone, Debug)]
pub struct UnshieldedWallet {
	pub verifying_key: VerifyingKey,
	signing_key: Option<SigningKey>,
}

impl DeriveSeed for UnshieldedWallet {}

impl IntoWalletAddress for UnshieldedWallet {
	fn address(self, network_id: NetworkId) -> WalletAddress {
		let hrp_string =
			format!("{}_{}{}", HRP_CONSTANT, HRP_CREDENTIAL_UNSHIELDED, Self::network(network_id));
		let hrp = bech32::Hrp::parse(&hrp_string)
			.unwrap_or_else(|err| panic!("Error while bech32 parsing: {}", err));

		let verifying_key = self.verifying_key;
		let mut x_only_bytes = Vec::new();
		Serializable::serialize(&verifying_key, &mut x_only_bytes)
			.unwrap_or_else(|err| panic!("Error Serializing `verifying_key`: {}", err));

		let data = &x_only_bytes;

		let address = bech32::encode::<Bech32m>(hrp, data)
			.unwrap_or_else(|err| panic!("Error bech32 encoding: {}", err));

		WalletAddress(address)
	}
}

impl UnshieldedWallet {
	fn from_seed(derived_seed: [u8; 32]) -> Self {
		let sk = SigningKey::from_bytes(&derived_seed)
			.unwrap_or_else(|err| panic!("Error calculating the `SigningKey`: {}", err));
		let verifying_key = sk.verifying_key();

		Self { verifying_key, signing_key: Some(sk) }
	}

	pub fn default(root_seed: WalletSeed) -> Self {
		let role = Role::UnshieldedExternal;
		let path = DerivationPath::default_for_role(role);
		let derived_seed = Self::derive_seed(root_seed, path);

		Self::from_seed(derived_seed)
	}

	pub fn from_path(root_seed: WalletSeed, path: String) -> Self {
		let path = DerivationPath::new(path);
		let derived_seed = Self::derive_seed(root_seed, path);

		Self::from_seed(derived_seed)
	}

	pub fn signing_key(&self) -> &SigningKey {
		self.signing_key
			.as_ref()
			.expect("Missing `SigningKey` for the `UnshieldedWallet")
	}

	pub fn from_pub_key(verifying_key: VerifyingKey) -> Self {
		Self { verifying_key, signing_key: None }
	}
}

impl From<WalletAddress> for UnshieldedWallet {
	fn from(address: WalletAddress) -> Self {
		let (hrp, data) = bech32::decode(&address.0).unwrap_or_else(|err| {
			panic!("Error while bech32 decoding to `UnshieldedWallet` {:?}: {}", address, err)
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
			hrp_credential == HRP_CREDENTIAL_UNSHIELDED,
			"Invalid address for a `UnshieldedWallet`"
		);

		let verifying_key: VerifyingKey = Deserializable::deserialize(&mut &data[..], 0)
			.unwrap_or_else(|err| panic!("Error deserializing `VerifyingKey`: {}", err));

		Self::from_pub_key(verifying_key)
	}
}

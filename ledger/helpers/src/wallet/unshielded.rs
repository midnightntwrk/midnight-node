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
	DerivationPath, DeriveSeed, HRP_CONSTANT, HRP_CREDENTIAL_UNSHIELDED, IntoWalletAddress,
	NetworkId, Role, SigningKey, UserAddress, VerifyingKey, WalletAddress, WalletSeed, network,
};
use base_crypto::hash::HashOutput;

#[derive(Clone, Debug)]
pub struct UnshieldedWallet {
	pub user_address: UserAddress,
	pub verifying_key: Option<VerifyingKey>,
	signing_key: Option<SigningKey>,
}

impl DeriveSeed for UnshieldedWallet {}

impl IntoWalletAddress for UnshieldedWallet {
	fn address(&self, network_id: NetworkId) -> WalletAddress {
		let hrp_string =
			format!("{HRP_CONSTANT}_{HRP_CREDENTIAL_UNSHIELDED}{}", network(network_id));
		let hrp = bech32::Hrp::parse(&hrp_string)
			.unwrap_or_else(|err| panic!("Error while bech32 parsing: {err}"));

		let data = &self.user_address.0.0;

		WalletAddress::new(hrp, data.to_vec())
	}
}

impl UnshieldedWallet {
	fn from_seed(derived_seed: [u8; 32]) -> Self {
		let sk = SigningKey::from_bytes(&derived_seed)
			.unwrap_or_else(|err| panic!("Error calculating the `SigningKey`: {err}"));
		let vk = sk.verifying_key();
		let user_address: UserAddress = vk.clone().into();

		Self { user_address, verifying_key: Some(vk), signing_key: Some(sk) }
	}

	pub fn default(root_seed: WalletSeed) -> Self {
		let role = Role::UnshieldedExternal;
		let path = DerivationPath::default_for_role(role);
		let derived_seed = Self::derive_seed(root_seed, &path);

		Self::from_seed(derived_seed)
	}

	pub fn from_path(root_seed: WalletSeed, path: &DerivationPath) -> Self {
		let derived_seed = Self::derive_seed(root_seed, path);

		Self::from_seed(derived_seed)
	}

	pub fn signing_key(&self) -> &SigningKey {
		self.signing_key
			.as_ref()
			.expect("Missing `SigningKey` for the `UnshieldedWallet")
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

		Ok(Self {
			user_address: UserAddress(HashOutput(address_data)),
			verifying_key: None,
			signing_key: None,
		})
	}
}

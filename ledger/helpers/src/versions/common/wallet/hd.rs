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

use super::super::{Deserializable, Serializable, Tagged, WalletSeed};
use bech32::{Bech32m, Hrp};
use bip32::{DerivationPath as Bip32DerivationPath, XPrv};
use std::str::FromStr;

pub const HRP_CONSTANT: &str = "mn";
pub const HRP_CREDENTIAL_UNSHIELDED: &str = "addr";
pub const HRP_CREDENTIAL_SHIELDED: &str = "shield-addr";
/// Encrypted Shielded Key
pub const HRP_CREDENTIAL_SHIELDED_ESK: &str = "shield-esk";
pub const HRP_CREDENTIAL_DUST: &str = "dust-addr";

#[derive(Debug, Clone)]
pub struct WalletAddress {
	hrp: Hrp,
	data: Vec<u8>,
}

impl WalletAddress {
	pub fn new(hrp: Hrp, data: Vec<u8>) -> Self {
		Self { hrp, data }
	}
	pub fn data(&self) -> &Vec<u8> {
		&self.data
	}
	pub fn human_readable_part(&self) -> String {
		self.hrp.as_str().to_string()
	}

	#[allow(clippy::unwrap_used)]
	pub fn to_bech32(&self) -> String {
		// We are OK to unwrap here because we only construct a `WalletAddress` from a valid bech32 address
		bech32::encode::<Bech32m>(self.hrp, &self.data)
			.unwrap_or_else(|err| panic!("Error bech32 encoding: {err}"))
	}
}

impl std::str::FromStr for WalletAddress {
	type Err = bech32::DecodeError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (hrp, data) = bech32::decode(s)?;
		Ok(WalletAddress { hrp, data })
	}
}

#[derive(Clone, Debug)]
pub enum Role {
	UnshieldedExternal,
	UnshieldedInternal,
	Dust,
	Zswap,
	Metadata,
}

pub struct DerivationPath {
	pub path: String,
	pub role: Role,
}

impl DerivationPath {
	pub fn new(path: String) -> Self {
		// Split the path by '/' and collect the elements
		let parts: Vec<&str> = path.split('/').collect();

		if parts.len() != 6 || parts[0] != "m" {
			panic!("Invalid derivation path format {path}");
		}

		// Parse the role component (4th index)
		let role_part = parts[4];

		// Attempt to parse role_part into an integer
		let role_index: u32 =
			role_part.parse().unwrap_or_else(|err| panic!("Invalid role value {err}"));

		let role = match role_index {
			0 => Role::UnshieldedExternal,
			1 => Role::UnshieldedInternal,
			2 => Role::Dust,
			3 => Role::Zswap,
			4 => Role::Metadata,
			_ => panic!("Unknown role value: {role_index}"),
		};

		Self { path, role }
	}

	pub fn default_for_role(role: Role) -> Self {
		let path = match role {
			Role::UnshieldedExternal => "m/44'/2400'/0'/0/0",
			Role::UnshieldedInternal => "m/44'/2400'/0'/1/0",
			Role::Dust => "m/44'/2400'/0'/2/0",
			Role::Zswap => "m/44'/2400'/0'/3/0",
			Role::Metadata => "m/44'/2400'/0'/4/0",
		};

		Self { path: path.to_string(), role }
	}
}

pub trait DeriveSeed {
	fn derive_seed(root_seed: WalletSeed, derivation_path: &DerivationPath) -> [u8; 32] {
		let derivation_path = Bip32DerivationPath::from_str(&derivation_path.path)
			.unwrap_or_else(|err| panic!("Error calculating the `DerivationPath`: {err}"));
		let derived = XPrv::derive_from_path(root_seed.as_bytes(), &derivation_path)
			.unwrap_or_else(|err| panic!("Error calculating the `ExtendedPrivateKey`: {err}"));

		derived.private_key().to_bytes().into()
	}
}

pub trait IntoWalletAddress {
	fn network_suffix(network_id: &str) -> String {
		if network_id == "mainnet" { String::new() } else { format!("_{network_id}") }
	}

	fn address(&self, network: &str) -> WalletAddress;
}

// in bech32-encoded addresses, we use the data's specific tag as a prefix, but not the global tag prefix
#[cfg(feature = "can-panic")]
pub(crate) fn short_tagged_serialize<T: Serializable + Tagged>(data: &T) -> Vec<u8> {
	let tag = T::tag();
	let mut buffer = vec![0; tag.len() + 1 + data.serialized_size()];
	buffer[..tag.len()].copy_from_slice(tag.as_bytes());
	buffer[tag.len()] = b':';
	let mut writer = &mut buffer[tag.len() + 1..];
	data.serialize(&mut writer).expect("infallible");
	buffer
}

pub(crate) fn short_tagged_deserialize<T: Deserializable + Tagged>(
	buffer: &[u8],
) -> Result<T, ShortTaggedDeserializeError> {
	let Some(tag_end) = buffer.iter().position(|b| *b == b':') else {
		return Err(ShortTaggedDeserializeError::MissingTag);
	};
	let expected_tag = T::tag();
	let actual_tag = &buffer[..tag_end];
	if expected_tag.as_bytes() != actual_tag {
		return Err(ShortTaggedDeserializeError::InvalidTag {
			expected: expected_tag,
			actual: actual_tag.to_vec(),
		});
	}
	let mut reader = &buffer[tag_end + 1..];
	T::deserialize(&mut reader, 0).map_err(ShortTaggedDeserializeError::DeserializeError)
}

#[derive(Debug)]
pub enum ShortTaggedDeserializeError {
	MissingTag,
	InvalidTag { expected: std::borrow::Cow<'static, str>, actual: Vec<u8> },
	DeserializeError(std::io::Error),
}

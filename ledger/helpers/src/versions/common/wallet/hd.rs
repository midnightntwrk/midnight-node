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

#![cfg(feature = "can-panic")]

use super::super::WalletSeed;
use bech32::{Bech32m, Hrp};
use bip32::{DerivationPath as Bip32DerivationPath, XPrv};
use std::str::FromStr;

pub const HRP_CONSTANT: &str = "mn";
pub const HRP_CREDENTIAL_UNSHIELDED: &str = "addr";
pub const HRP_CREDENTIAL_SHIELDED: &str = "shield-addr";
/// Encrypted Shielded Key
pub const HRP_CREDENTIAL_SHIELDED_ESK: &str = "shield-esk";
pub const HRP_CREDENTIAL_DUST: &str = "dust";

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
	UnshieldedExternal,
	UnshieldedInternal,
	Dust,
	Zswap,
	Ecdsa,
}

impl TryFrom<u32> for Role {
	type Error = DerivationPathError;

	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(Role::UnshieldedExternal),
			1 => Ok(Role::UnshieldedInternal),
			2 => Ok(Role::Dust),
			3 => Ok(Role::Zswap),
			4 => Ok(Role::Ecdsa),
			_ => Err(DerivationPathError::UnknownRole(value)),
		}
	}
}

#[derive(Debug, thiserror::Error)]
pub enum DerivationPathError {
	#[error("Invalid derivation path format: {0}")]
	InvalidFormat(String),
	#[error("Invalid role value: {0}")]
	InvalidRoleValue(String),
	#[error("Unknown role value: {0}")]
	UnknownRole(u32),
	#[error("Role mismatch: expected one of {expected:?}, got {actual:?}")]
	RoleMismatch { expected: Vec<Role>, actual: Role },
}

pub struct DerivationPath {
	pub path: String,
	pub role: Role,
}

impl DerivationPath {
	pub fn new(path: String) -> Result<Self, DerivationPathError> {
		// Split the path by '/' and collect the elements
		let parts: Vec<&str> = path.split('/').collect();

		if parts.len() != 6 || parts[0] != "m" {
			return Err(DerivationPathError::InvalidFormat(path));
		}

		// Parse the role component (4th index)
		let role_part = parts[4];

		// Attempt to parse role_part into an integer
		let role_index: u32 = role_part
			.parse()
			.map_err(|_| DerivationPathError::InvalidRoleValue(role_part.to_string()))?;

		let role = Role::try_from(role_index)?;

		Ok(Self { path, role })
	}

	pub fn validate_role(&self, expected: &[Role]) -> Result<(), DerivationPathError> {
		if !expected.contains(&self.role) {
			return Err(DerivationPathError::RoleMismatch {
				expected: expected.to_vec(),
				actual: self.role.clone(),
			});
		}
		Ok(())
	}

	pub fn default_for_role(role: Role) -> Self {
		let path = match role {
			Role::UnshieldedExternal => "m/44'/2400'/0'/0/0",
			Role::UnshieldedInternal => "m/44'/2400'/0'/1/0",
			Role::Dust => "m/44'/2400'/0'/2/0",
			Role::Zswap => "m/44'/2400'/0'/3/0",
			Role::Ecdsa => "m/44'/2400'/0'/4/0",
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_derivation_path_valid_all_roles() {
		for (role_index, expected_role) in [
			(0, Role::UnshieldedExternal),
			(1, Role::UnshieldedInternal),
			(2, Role::Dust),
			(3, Role::Zswap),
			(4, Role::Ecdsa),
		] {
			let path = format!("m/44'/2400'/0'/{role_index}/0");
			let dp = DerivationPath::new(path.clone()).expect("should be valid");
			assert_eq!(dp.role, expected_role);
			assert_eq!(dp.path, path);
		}
	}

	#[test]
	fn test_derivation_path_invalid_format_too_few_parts() {
		let result = DerivationPath::new("m/44'/2400'/0'".to_string());
		assert!(matches!(result, Err(DerivationPathError::InvalidFormat(_))));
	}

	#[test]
	fn test_derivation_path_invalid_format_wrong_prefix() {
		let result = DerivationPath::new("x/44'/2400'/0'/0/0".to_string());
		assert!(matches!(result, Err(DerivationPathError::InvalidFormat(_))));
	}

	#[test]
	fn test_derivation_path_invalid_role_value() {
		let result = DerivationPath::new("m/44'/2400'/0'/abc/0".to_string());
		assert!(matches!(result, Err(DerivationPathError::InvalidRoleValue(_))));
	}

	#[test]
	fn test_derivation_path_unknown_role() {
		let result = DerivationPath::new("m/44'/2400'/0'/5/0".to_string());
		assert!(matches!(result, Err(DerivationPathError::UnknownRole(5))));

		let result = DerivationPath::new("m/44'/2400'/0'/99/0".to_string());
		assert!(matches!(result, Err(DerivationPathError::UnknownRole(99))));
	}

	#[test]
	fn test_role_try_from_valid() {
		assert_eq!(Role::try_from(0).unwrap(), Role::UnshieldedExternal);
		assert_eq!(Role::try_from(1).unwrap(), Role::UnshieldedInternal);
		assert_eq!(Role::try_from(2).unwrap(), Role::Dust);
		assert_eq!(Role::try_from(3).unwrap(), Role::Zswap);
		assert_eq!(Role::try_from(4).unwrap(), Role::Ecdsa);
	}

	#[test]
	fn test_role_try_from_invalid() {
		assert!(matches!(Role::try_from(5), Err(DerivationPathError::UnknownRole(5))));
		assert!(matches!(Role::try_from(99), Err(DerivationPathError::UnknownRole(99))));
		assert!(matches!(
			Role::try_from(u32::MAX),
			Err(DerivationPathError::UnknownRole(u32::MAX))
		));
	}

	#[test]
	fn test_validate_role_single_match() {
		let dp = DerivationPath::default_for_role(Role::Zswap);
		assert!(dp.validate_role(&[Role::Zswap]).is_ok());
	}

	#[test]
	fn test_validate_role_single_mismatch() {
		let dp = DerivationPath::default_for_role(Role::Dust);
		let result = dp.validate_role(&[Role::Zswap]);
		assert!(matches!(result, Err(DerivationPathError::RoleMismatch { .. })));
	}

	#[test]
	fn test_validate_role_multiple_accepts_any_match() {
		let dp_ext = DerivationPath::default_for_role(Role::UnshieldedExternal);
		assert!(
			dp_ext
				.validate_role(&[Role::UnshieldedExternal, Role::UnshieldedInternal])
				.is_ok()
		);

		let dp_int = DerivationPath::default_for_role(Role::UnshieldedInternal);
		assert!(
			dp_int
				.validate_role(&[Role::UnshieldedExternal, Role::UnshieldedInternal])
				.is_ok()
		);
	}

	#[test]
	fn test_validate_role_multiple_rejects_non_match() {
		let dp = DerivationPath::default_for_role(Role::Dust);
		assert!(matches!(
			dp.validate_role(&[Role::UnshieldedExternal, Role::UnshieldedInternal]),
			Err(DerivationPathError::RoleMismatch { .. })
		));
	}
}

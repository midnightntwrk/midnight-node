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

use super::super::{ContractAddress, Transcript};
use bip39::Mnemonic;
use std::{collections::HashMap, str::FromStr};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WalletSeed {
	Short([u8; 16]),
	Medium([u8; 32]),
	Long([u8; 64]),
}

impl Default for WalletSeed {
	fn default() -> Self {
		Self::Medium([0; 32])
	}
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum WalletSeedError {
	#[error("{0}")]
	InvalidHex(#[from] hex::FromHexError),
	#[error("expected 16, 32, or 64 bytes; got {0}")]
	InvalidLength(usize),
	#[error("{0}")]
	InvalidMnemonic(#[from] bip39::Error),
}

impl WalletSeed {
	#[cfg(feature = "can-panic")]
	pub fn try_from_hex_str(value: &str) -> Result<Self, WalletSeedError> {
		let bytes = hex::decode(value)?;
		match bytes.len() {
			16 => Ok(Self::Short(bytes.try_into().unwrap())),
			32 => Ok(Self::Medium(bytes.try_into().unwrap())),
			64 => Ok(Self::Long(bytes.try_into().unwrap())),
			len => Err(WalletSeedError::InvalidLength(len)),
		}
	}

	pub fn try_from_mnemonic(value: &str) -> Result<Self, WalletSeedError> {
		let mnemonic = Mnemonic::parse(value)?;
		Ok(Self::Long(mnemonic.to_seed("")))
	}

	pub fn as_bytes(&self) -> &[u8] {
		match self {
			Self::Short(bytes) => bytes,
			Self::Medium(bytes) => bytes,
			Self::Long(bytes) => bytes,
		}
	}
}

impl From<[u8; 32]> for WalletSeed {
	fn from(value: [u8; 32]) -> Self {
		Self::Medium(value)
	}
}

impl FromStr for WalletSeed {
	type Err = WalletSeedError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let s = s.trim();
		if let Ok(seed) = Self::try_from_hex_str(s) { Ok(seed) } else { Self::try_from_mnemonic(s) }
	}
}

pub type MaintenanceCounter = u32;

#[derive(Default, Clone)]
pub struct MaintenanceUpdateBuilder {
	pub num_contract_replace_auth: u32,
	pub num_contract_key_remove: u32,
	pub num_contract_key_insert: u32,
	pub addresses_map: HashMap<ContractAddress, MaintenanceCounter>,
	pub addresses_vec: Vec<ContractAddress>,
}

impl MaintenanceUpdateBuilder {
	pub fn new(
		num_contract_replace_auth: u32,
		num_contract_key_remove: u32,
		num_contract_key_insert: u32,
	) -> Self {
		MaintenanceUpdateBuilder {
			num_contract_replace_auth,
			num_contract_key_remove,
			num_contract_key_insert,
			..Default::default()
		}
	}

	pub fn add_address(&mut self, addr: &ContractAddress, counter: MaintenanceCounter) {
		self.addresses_map.insert(*addr, counter);
		self.addresses_vec.push(*addr);
	}

	pub fn add_addresses(&mut self, addrs: &[ContractAddress], counters: Vec<MaintenanceCounter>) {
		(0..addrs.len()).for_each(|i| self.add_address(&addrs[i], counters[i]));
	}

	pub fn increase_counter(&mut self, addr: ContractAddress) {
		if let Some(counter) = self.addresses_map.get_mut(&addr) {
			*counter = counter.saturating_add(1);
		}
	}
}

#[derive(Debug, Clone)]
pub enum WalletUpdate {
	Yes,
	No,
}

#[derive(Debug, Clone)]
pub enum ContractType {
	MerkleTree,
	// MicroDao,
}

#[derive(Debug, Clone)]
pub struct ZswapContractAddresses {
	pub outputs: Option<Vec<ContractAddress>>,
	pub transients: Option<Vec<ContractAddress>>,
}

pub enum WalletKind {
	Legacy,
	NoLegacy,
}

pub type Transcripts<D> = (Option<Transcript<D>>, Option<Transcript<D>>);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Segment {
	Guaranteed = 0,
	Fallible = 1,
}

impl From<Segment> for u16 {
	fn from(val: Segment) -> Self {
		match val {
			Segment::Guaranteed => 0,
			Segment::Fallible => 1,
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::WalletSeed;

	#[test]
	fn should_decode_wallet_seeds_in_different_formats() {
		let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon diesel";
		let mnemonic_seed: WalletSeed = mnemonic.parse().unwrap();
		let hex = "a51c86de32d0791f7cffc3bdff1abd9bb54987f0ed5effc30c936dddbb9afd9d530c8db445e4f2d3ea42a321b260e022aadf05987c9a67ec7b6b6ca1d0593ec9";
		let hex_seed: WalletSeed = hex.parse().unwrap();
		assert_eq!(mnemonic_seed, hex_seed);
	}
}

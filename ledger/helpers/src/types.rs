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

use coin_structure::contract::ContractAddress;
use onchain_runtime::transcript::Transcript;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct WalletSeed(pub [u8; 32]);

impl From<[u8; 32]> for WalletSeed {
	fn from(value: [u8; 32]) -> Self {
		Self(value)
	}
}

impl From<WalletSeed> for [u8; 32] {
	fn from(value: WalletSeed) -> Self {
		value.0
	}
}

impl From<&str> for WalletSeed {
	fn from(value: &str) -> Self {
		let bytes: Vec<u8> = hex::decode(value).unwrap();
		let bytes: [u8; 32] = bytes.try_into().unwrap();

		Self(bytes)
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
	FeePayments = 2,
}

impl From<Segment> for u16 {
	fn from(val: Segment) -> Self {
		match val {
			Segment::Guaranteed => 0,
			Segment::Fallible => 1,
			Segment::FeePayments => 2,
		}
	}
}

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

//! Test mock for system-parameters pallet

use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use sp_core::H256;
use sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, IdentityLookup},
};

use crate as pallet_system_parameters;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system = 0,
		SystemParameters: pallet_system_parameters = 1,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

impl pallet_system_parameters::Config for Test {
	type SystemOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type WeightInfo = ();
}

/// Build genesis storage for testing
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	t.into()
}

/// Build genesis storage with initial values
pub fn new_test_ext_with_genesis(
	terms_hash: Option<H256>,
	terms_url: Option<String>,
	d_param: Option<sidechain_domain::DParameter>,
) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let (num_permissioned, num_registered) = match d_param {
		Some(d) => (Some(d.num_permissioned_candidates), Some(d.num_registered_candidates)),
		None => (None, None),
	};

	pallet_system_parameters::GenesisConfig::<Test> {
		terms_and_conditions: pallet_system_parameters::TermsAndConditionsGenesisConfig {
			hash: terms_hash,
			url: terms_url,
		},
		d_parameter: pallet_system_parameters::DParameterGenesisConfig {
			num_permissioned_candidates: num_permissioned,
			num_registered_candidates: num_registered,
		},
		_marker: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}

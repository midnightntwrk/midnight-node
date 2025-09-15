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

use crate::mock::sp_runtime::testing::H256;
use frame_support::sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, Get, IdentityLookup},
};
use frame_support::traits::{ConstU16, ConstU32, ConstU64};
use frame_support::*;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use sidechain_domain::*;
use sp_io::TestExternalities;

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::pallet]
pub mod mock_pallet {
	use frame_support::pallet_prelude::*;
	use sidechain_domain::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	pub type LastTokenTransfer<T: Config> = StorageValue<_, NativeTokenAmount, OptionQuery>;
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Timestamp: pallet_timestamp,
		NativeTokenObservation: crate::pallet,
		Midnight: pallet_midnight,
		MidnightSystem: pallet_midnight_system,
		Mock: mock_pallet,
	}
);

pub const SLOT_DURATION: u64 = 6 * 1000;

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<3000>;
	type WeightInfo = ();
}

pub type BeneficiaryId = midnight_node_ledger::types::Hash;
pub type BlockRewardPoints = u128;
pub type BlockReward = (BlockRewardPoints, Option<BeneficiaryId>);
pub struct LedgerBlockReward;
impl Get<BlockReward> for LedgerBlockReward {
	fn get() -> BlockReward {
		(0, None)
	}
}

impl pallet_midnight::Config for Test {
	type WeightInfo = pallet_midnight::weights::SubstrateWeight<Test>;
	type BlockReward = LedgerBlockReward;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

impl pallet_midnight_system::Config for Test {
	type LedgerStateProviderMut = Midnight;
	type LedgerBlockContextProvider = Midnight;
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type Nonce = u64;
	type Block = Block;
	type RuntimeTask = RuntimeTask;
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type ExtensionsWeightInfo = ();
}

parameter_types! {
	pub const MaxRegistrationsPerCardanoAddress: u8 = 100;
}

impl crate::pallet::Config for Test {
	type MaxRegistrationsPerCardanoAddress = MaxRegistrationsPerCardanoAddress;
	type MidnightSystemTransactionExecutor = MidnightSystem;
}

impl mock_pallet::Config for Test {}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t: TestExternalities = RuntimeGenesisConfig {
		system: Default::default(),
		native_token_observation: Default::default(),
		midnight: MidnightConfig {
			_config: Default::default(),
			network_id: UndeployedNetwork.network_id(),
			genesis_state_key: midnight_node_ledger::get_root(UndeployedNetwork.genesis_state()),
		},
	}
	.build_storage()
	.unwrap()
	.into();

	t.execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
	});

	t
}

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

// grcov-excl-start
use crate as pallet_midnight;
use frame_support::traits::{ConstU16, ConstU64};
//#[cfg(feature = "experimental")]
//use sp_block_rewards::GetBlockRewardPoints;
use sp_core::H256;
use sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, Get, IdentityLookup},
};

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system = 0,
		Timestamp: pallet_timestamp = 1,
		Midnight: pallet_midnight = 5,
		//#[cfg(feature = "experimental")]
		//BlockRewards: pallet_block_rewards = 9,
	}
);

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Block = Block;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
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
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type RuntimeTask = ();
	type SingleBlockMigrations = (); // replaces the `Executive` now for configuring migrations.
	type MultiBlockMigrator = (); // the `pallet-migrations` would be set here, if deployed.
	type PreInherents = (); // a hook that runs before any inherent.
	type PostInherents = (); // a hook to run between inherents and `poll`/MBM logic.
	type PostTransactions = (); // a hook to run after all transactions but before `on_idle`.
	type ExtensionsWeightInfo = ();
}

pub const SLOT_DURATION: u64 = 6 * 1000;

impl pallet_timestamp::Config for Test {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

pub type BeneficiaryId = midnight_node_ledger::types::Hash;
pub type BlockRewardPoints = u128;
pub type BlockReward = (BlockRewardPoints, Option<BeneficiaryId>);
pub struct LedgerBlockReward;
impl Get<BlockReward> for LedgerBlockReward {
	#[cfg(feature = "experimental")]
	fn get() -> BlockReward {
		/*
		(
			<Test as pallet_block_rewards::Config>::GetBlockRewardPoints::get_block_reward(),
			pallet_block_rewards::CurrentBlockBeneficiary::<Test>::get(),
		)
		*/
		(0, None)
	}
	#[cfg(not(feature = "experimental"))]
	fn get() -> BlockReward {
		(0, None)
	}
}

impl pallet_midnight::Config for Test {
	type WeightInfo = ();
	type BlockReward = LedgerBlockReward;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

/*
#[cfg(feature = "experimental")]
pub const BLOCK_REWARD_POINTS: u128 = 500_000;
#[cfg(feature = "experimental")]
pub struct LedgerBlockRewardPoints;
#[cfg(feature = "experimental")]
impl GetBlockRewardPoints<BlockRewardPoints> for LedgerBlockRewardPoints {
	fn get_block_reward() -> BlockRewardPoints {
		BLOCK_REWARD_POINTS
	}
}
*/

/*
#[cfg(feature = "experimental")]
impl pallet_block_rewards::Config for Test {
	type BeneficiaryId = BeneficiaryId;
	type BlockRewardPoints = BlockRewardPoints;
	type GetBlockRewardPoints = LedgerBlockRewardPoints;
}
	 */

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}

pub fn midnight_events() -> Vec<super::Event> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Midnight(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

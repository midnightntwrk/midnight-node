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

use frame_support::sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, Get, IdentityLookup},
};
use frame_support::traits::{ConstU16, ConstU32, ConstU64};
use frame_support::*;
use midnight_primitives::MidnightSystemTransactionExecutor;
use sidechain_domain::*;
use sp_io::TestExternalities;
use sp_runtime::testing::H256;
use std::sync::{LazyLock, Mutex};

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

#[frame_support::runtime]
mod runtime {
	use frame_system::pallet;

	use crate::mock;

	use super::*;

	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;
	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp::Pallet<Test>;
	#[runtime::pallet_index(2)]
	pub type CNightObservation = pallet_cnight_observation::Pallet<Test>;
	#[runtime::pallet_index(3)]
	pub type Mock = mock_pallet::Pallet<Test>;
}

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

pub struct MidnightSystemTx {}

static CAPTURED_SYSTEM_TXS: LazyLock<Mutex<Vec<Vec<u8>>>> = LazyLock::new(|| Mutex::new(vec![]));

impl MidnightSystemTx {
	pub fn pop_captured_system_txs() -> Vec<Vec<u8>> {
		CAPTURED_SYSTEM_TXS.lock().unwrap().drain(..).collect()
	}
}

impl MidnightSystemTransactionExecutor for MidnightSystemTx {
	fn execute_system_transaction(
		serialized_system_transaction: Vec<u8>,
	) -> Result<midnight_node_ledger::types::Hash, __private::DispatchError> {
		CAPTURED_SYSTEM_TXS.lock().unwrap().push(serialized_system_transaction);
		Ok(midnight_node_ledger::types::Hash::default())
	}
}

impl pallet_cnight_observation::Config for Test {
	type MidnightSystemTransactionExecutor = MidnightSystemTx;
}

impl mock_pallet::Config for Test {}

#[cfg(feature = "std")]
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t: TestExternalities = RuntimeGenesisConfig {
		system: Default::default(),
		c_night_observation: Default::default(),
	}
	.build_storage()
	.unwrap()
	.into();

	t.execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
	});

	t
}

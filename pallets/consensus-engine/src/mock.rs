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

//! Test mock for the consensus-engine pallet.

use frame_support::{
	derive_impl,
	traits::{ConstBool, ConstU32, ConstU64},
};
use frame_system::EnsureRoot;
use parity_scale_codec::Encode;
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_babe::BABE_ENGINE_ID;
use sp_consensus_babe::digests::{PreDigest as BabePreDigest, SecondaryPlainPreDigest};
use sp_consensus_slots::Slot;
use sp_core::H256;
use sp_runtime::{
	BuildStorage, Digest, DigestItem,
	traits::{BlakeTwo256, IdentityLookup},
};

use crate as pallet_consensus_engine;

/// Epoch length shared by pallet-babe and the consensus-engine pallet in the mock.
const EPOCH_DURATION: u64 = 300;
/// Slot duration in milliseconds used by the AURA/BABE/timestamp mock configs.
const SLOT_DURATION: u64 = 6000;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system = 0,
		ConsensusEngine: pallet_consensus_engine = 1,
		Timestamp: pallet_timestamp = 2,
		Aura: pallet_aura = 3,
		Babe: pallet_babe = 4,
	}
);

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
	type BlockHashCount = ();
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_aura::Config for Test {
	type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<32>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

impl pallet_babe::Config for Test {
	type EpochDuration = ConstU64<EPOCH_DURATION>;
	type ExpectedBlockTime = ConstU64<SLOT_DURATION>;
	type EpochChangeTrigger = pallet_babe::ExternalTrigger;
	type DisabledValidators = ();
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<5>;
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_consensus_engine::Config for Test {
	// Only root drives state transitions in the mock, mirroring the runtime's governance origin.
	type GovernanceOrigin = EnsureRoot<u64>;
	type EpochDuration = ConstU64<EPOCH_DURATION>;
	type WeightInfo = ();
}

/// Start a new block whose header carries exactly the given digest `logs`.
pub fn start_block_with_logs(logs: Vec<DigestItem>) {
	let number = System::block_number() + 1;
	System::initialize(&number, &Default::default(), &Digest { logs });
}

/// An AURA pre-runtime digest item for `slot`.
pub fn aura_pre_digest(slot: u64) -> DigestItem {
	DigestItem::PreRuntime(AURA_ENGINE_ID, Slot::from(slot).encode())
}

/// A BABE (secondary plain) pre-runtime digest item for `slot`.
pub fn babe_pre_digest(slot: u64) -> DigestItem {
	DigestItem::PreRuntime(
		BABE_ENGINE_ID,
		BabePreDigest::SecondaryPlain(SecondaryPlainPreDigest {
			authority_index: 0,
			slot: Slot::from(slot),
		})
		.encode(),
	)
}

/// A pre-runtime digest item for an unrelated engine, which the pallet must ignore.
pub fn unrelated_pre_digest() -> DigestItem {
	DigestItem::PreRuntime(*b"test", Vec::new())
}

/// Start a new block whose header carries an AURA pre-runtime digest for `slot`,
/// mirroring how the pallet reads the current slot in `on_initialize`.
pub fn start_block_at_slot(slot: u64) {
	start_block_with_logs(vec![aura_pre_digest(slot)]);
}

/// Start a new block whose header carries a BABE pre-runtime digest (alongside
/// the AURA one, at the same slot), simulating a node that emits BABE digests too early.
pub fn start_block_with_babe_pre_digest(slot: u64) {
	start_block_with_logs(vec![aura_pre_digest(slot), babe_pre_digest(slot)]);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	// Block 0 does not record events; move to block 1 so `assert_last_event` works.
	ext.execute_with(|| System::set_block_number(1));
	ext
}

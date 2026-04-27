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

use crate as pallet_collective_proposer_cancel;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, Everything, NeverEnsureOrigin},
};
use frame_system::{EnsureNone, EnsureRoot};
use runtime_common::governance::RecordProposer;
use sp_core::H256;
use sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, IdentityLookup},
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system = 0,
		Council: pallet_collective::<Instance1> = 1,
		CouncilMembership: pallet_membership::<Instance1> = 2,
		ProposerCancel: pallet_collective_proposer_cancel::<Instance1> = 3,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
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

pub const MOTION_DURATION: u64 = 5 * 24 * 60 * 60 / 6; // 5 days in 6-second blocks
pub const MAX_PROPOSALS: u32 = 100;
pub const MAX_MEMBERS: u32 = 10;

parameter_types! {
	pub const MotionDurationParam: u64 = MOTION_DURATION;
	pub MaxProposalWeight: frame_support::weights::Weight =
		frame_support::weights::Weight::from_parts(u64::MAX, u64::MAX);
}

pub type CouncilCollective = pallet_collective::Instance1;
impl pallet_collective::Config<CouncilCollective> for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDurationParam;
	type MaxProposals = ConstU32<MAX_PROPOSALS>;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type DefaultVote = pallet_collective::MoreThanMajorityThenPrimeDefaultVote;
	type SetMembersOrigin = NeverEnsureOrigin<()>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<u64>;
	type KillOrigin = EnsureRoot<u64>;
	// Recording proposer is what makes proposer-cancel work; with the unit
	// `()` Consideration `CostOf` is left empty and `cancel_proposal` always
	// fails with `ProposerNotRecorded`.
	type Consideration = RecordProposer;
	type WeightInfo = ();
}

impl pallet_membership::Config<pallet_membership::Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = NeverEnsureOrigin<()>;
	type RemoveOrigin = NeverEnsureOrigin<()>;
	type SwapOrigin = NeverEnsureOrigin<()>;
	type ResetOrigin = EnsureNone<Self::AccountId>;
	type PrimeOrigin = NeverEnsureOrigin<()>;
	type MembershipInitialized = Council;
	type MembershipChanged = Council;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type WeightInfo = ();
}

impl pallet_collective_proposer_cancel::Config<CouncilCollective> for Test {}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_membership::GenesisConfig::<Test, pallet_membership::Instance1> {
		members: vec![1, 2, 3].try_into().unwrap(),
		phantom: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn proposer_cancel_events()
-> Vec<pallet_collective_proposer_cancel::Event<Test, CouncilCollective>> {
	System::events()
		.into_iter()
		.filter_map(|r| if let RuntimeEvent::ProposerCancel(e) = r.event { Some(e) } else { None })
		.collect()
}

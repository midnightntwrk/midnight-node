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

use crate::{
	self as pallet_federated_authority, AuthId,
	types::{
		AuthorityBody, FederatedAuthorityEnsureProportionAtLeast, FederatedAuthorityOriginManager,
	},
};
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, Everything, Hooks, NeverEnsureOrigin},
};
use frame_system::{EnsureNone, EnsureRoot};
use sp_core::H256;
use sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, IdentityLookup},
};

type Block = frame_system::mocking::MockBlock<Test>;

pub(crate) const COUNCIL_PALLET_ID: AuthId = 40;
pub(crate) const TECHNICAL_COMMITTEE_PALLET_ID: AuthId = 42;

frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system = 0,
		// Governance - matching runtime structure
		Council: pallet_collective::<Instance1> = 40,
		CouncilMembership: pallet_membership::<Instance1> = 41,
		TechnicalCommittee: pallet_collective::<Instance2> = 42,
		TechnicalCommitteeMembership: pallet_membership::<Instance2> = 43,
		FederatedAuthority: pallet_federated_authority = 44,
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

// Parameters matching runtime
pub const MOTION_DURATION: u64 = 5 * 24 * 60 * 60 / 6; // 5 days in 6-second blocks
pub const MAX_PROPOSALS: u32 = 100;
pub const MAX_MEMBERS: u32 = 10;

parameter_types! {
	pub const MotionDurationParam: u64 = MOTION_DURATION;
	pub MaxProposalWeight: frame_support::weights::Weight = frame_support::weights::Weight::from_parts(u64::MAX, u64::MAX);
}

// Council configuration
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
	type Consideration = ();
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

// Technical Committee configuration
pub type TechnicalCommitteeCollective = pallet_collective::Instance2;
impl pallet_collective::Config<TechnicalCommitteeCollective> for Test {
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
	type Consideration = ();
	type WeightInfo = ();
}

impl pallet_membership::Config<pallet_membership::Instance2> for Test {
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = NeverEnsureOrigin<()>;
	type RemoveOrigin = NeverEnsureOrigin<()>;
	type SwapOrigin = NeverEnsureOrigin<()>;
	type ResetOrigin = EnsureNone<Self::AccountId>;
	type PrimeOrigin = NeverEnsureOrigin<()>;
	type MembershipInitialized = TechnicalCommittee;
	type MembershipChanged = TechnicalCommittee;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type WeightInfo = ();
}

// Federated Authority configuration
pub const MAX_NUM_BODIES: u32 = 30; // Bigger number to properly measure `a` impact in the benchmarks

type CouncilApproval = AuthorityBody<
	Council,
	pallet_collective::EnsureProportionAtLeast<u64, CouncilCollective, 2, 3>,
>;
type TechnicalCommitteeApproval = AuthorityBody<
	TechnicalCommittee,
	pallet_collective::EnsureProportionAtLeast<u64, TechnicalCommitteeCollective, 2, 3>,
>;

type CouncilRevoke = AuthorityBody<
	Council,
	pallet_collective::EnsureProportionAtLeast<u64, CouncilCollective, 2, 3>,
>;
type TechnicalCommitteeRevoke = AuthorityBody<
	TechnicalCommittee,
	pallet_collective::EnsureProportionAtLeast<u64, TechnicalCommitteeCollective, 2, 3>,
>;

impl crate::Config for Test {
	type MotionCall = RuntimeCall;
	type MaxAuthorityBodies = ConstU32<MAX_NUM_BODIES>;
	type MotionDuration = MotionDurationParam;
	type MotionApprovalProportion = FederatedAuthorityEnsureProportionAtLeast<2, MAX_NUM_BODIES>; // Council +  TechnicalCommittee approvals should be enough
	type MotionApprovalOrigin =
		FederatedAuthorityOriginManager<(CouncilApproval, TechnicalCommitteeApproval)>;
	type MotionRevokeOrigin =
		FederatedAuthorityOriginManager<(CouncilRevoke, TechnicalCommitteeRevoke)>;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	// Initialize Council members
	pallet_membership::GenesisConfig::<Test, pallet_membership::Instance1> {
		members: vec![1, 2, 3].try_into().unwrap(),
		phantom: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	// Initialize Technical Committee members
	pallet_membership::GenesisConfig::<Test, pallet_membership::Instance2> {
		members: vec![4, 5, 6].try_into().unwrap(),
		phantom: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		<System as Hooks<u64>>::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		<System as Hooks<u64>>::on_initialize(System::block_number());
	}
}

pub fn last_event() -> RuntimeEvent {
	System::events().pop().expect("Event expected").event
}

pub fn federated_authority_events() -> Vec<crate::Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(
			|r| {
				if let RuntimeEvent::FederatedAuthority(e) = r.event { Some(e) } else { None }
			},
		)
		.collect::<Vec<_>>()
}

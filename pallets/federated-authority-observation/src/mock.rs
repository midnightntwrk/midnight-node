// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0

use crate as pallet_federated_authority_observation;
use frame_support::{derive_impl, parameter_types, traits::NeverEnsureOrigin};
use runtime_common::governance::{AlwaysNo, MembershipHandler};
use sp_runtime::{BuildStorage, traits::IdentityLookup};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		// Governance bodies
		Council: pallet_collective::<Instance1>,
		CouncilMembership: pallet_membership::<Instance1>,
		TechnicalCommittee: pallet_collective::<Instance2>,
		TechnicalCommitteeMembership: pallet_membership::<Instance2>,
		FederatedAuthorityObservation: pallet_federated_authority_observation,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
	pub const CouncilMaxMembers: u32 = 10;
	pub const TechnicalCommitteeMaxMembers: u32 = 10;
	pub const MotionDuration: u64 = 100;
	pub const MaxProposals: u32 = 100;
}

/// Council
type CouncilCollective = pallet_collective::Instance1;
impl pallet_collective::Config<CouncilCollective> for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = MaxProposals;
	type MaxMembers = CouncilMaxMembers;
	type DefaultVote = AlwaysNo;
	type SetMembersOrigin = NeverEnsureOrigin<()>;
	type MaxProposalWeight = ();
	type DisapproveOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type KillOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = ();
}

impl pallet_membership::Config<pallet_membership::Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = NeverEnsureOrigin<()>;
	type RemoveOrigin = NeverEnsureOrigin<()>;
	type SwapOrigin = NeverEnsureOrigin<()>;
	type ResetOrigin = NeverEnsureOrigin<()>;
	type PrimeOrigin = NeverEnsureOrigin<()>;
	type MembershipInitialized = MembershipHandler<Test, Council>;
	type MembershipChanged = ();
	type MaxMembers = CouncilMaxMembers;
	type WeightInfo = ();
}

/// Technical Committee
type TechnicalCommitteeCollective = pallet_collective::Instance2;
impl pallet_collective::Config<TechnicalCommitteeCollective> for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = MaxProposals;
	type MaxMembers = TechnicalCommitteeMaxMembers;
	type DefaultVote = AlwaysNo;
	type SetMembersOrigin = NeverEnsureOrigin<()>;
	type MaxProposalWeight = ();
	type DisapproveOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type KillOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = ();
}

impl pallet_membership::Config<pallet_membership::Instance2> for Test {
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = NeverEnsureOrigin<()>;
	type RemoveOrigin = NeverEnsureOrigin<()>;
	type SwapOrigin = NeverEnsureOrigin<()>;
	type ResetOrigin = NeverEnsureOrigin<()>;
	type PrimeOrigin = NeverEnsureOrigin<()>;
	type MembershipInitialized = MembershipHandler<Test, TechnicalCommittee>;
	type MembershipChanged = ();
	type MaxMembers = TechnicalCommitteeMaxMembers;
	type WeightInfo = ();
}

impl pallet_federated_authority_observation::Config for Test {
	type CouncilMaxMembers = CouncilMaxMembers;
	type TechnicalCommitteeMaxMembers = TechnicalCommitteeMaxMembers;
	type CouncilMembershipChanged = MembershipHandler<Test, Council>;
	type TechnicalCommitteeMembershipChanged = MembershipHandler<Test, TechnicalCommittee>;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

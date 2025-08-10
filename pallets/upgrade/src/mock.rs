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

use super::*;
use crate as pallet_upgrade;

use frame_support::{
	BoundedVec, PalletId, derive_impl,
	pallet_prelude::*,
	parameter_types,
	traits::EqualPrivilegeOnly,
	traits::{Hooks, OneSessionHandler},
	weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};

use frame_system::EnsureRoot;
// use pallet_partner_chains_session::{SessionManager, ShouldEndSession};
use pallet_partner_chains_session::{SessionManager, ShouldEndSession};
use sp_core::{Get, MaxEncodedLen};
use sp_io::TestExternalities;
use sp_runtime::testing::UintAuthorityId;
use sp_runtime::{BuildStorage, Perbill, impl_opaque_keys};
use sp_staking::SessionIndex;
use sp_version::RuntimeVersion;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

use sp_consensus_aura::sr25519::AuthorityId as AuraId;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Upgrade: pallet_upgrade,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,

		Session: pallet_partner_chains_session
	}
);

pub struct MockSessionManager;
impl ShouldEndSession<u64> for MockSessionManager {
	fn should_end_session(_now: u64) -> bool {
		true
	}
}

impl SessionManager<u64, TestSessionKeys> for MockSessionManager {
	// Session member alternation targeting a majority of members
	fn new_session(session_idx: u32) -> Option<Vec<(u64, TestSessionKeys)>> {
		let mut session_members = vec![];
		for i in 0..10 {
			let tsk = TestSessionKeys { other: UintAuthorityId(i) };
			session_members.push((i, tsk));
		}
		if session_idx & 2 == 0 {
			Some(session_members[0..7].to_vec())
		} else {
			Some(session_members[3..10].to_vec())
		}
	}
	fn end_session(end_index: u32) {
		pallet_upgrade::Pallet::<Test>::on_session_end(end_index);
	}
	fn start_session(_: u32) {}
}

pub struct OtherSessionHandler;
impl OneSessionHandler<AccountId> for OtherSessionHandler {
	type Key = UintAuthorityId;

	fn on_genesis_session<'a, I: Iterator<Item = (&'a AccountId, Self::Key)> + 'a>(_: I) {}

	fn on_new_session<'a, I: Iterator<Item = (&'a AccountId, Self::Key)> + 'a>(
		_: bool,
		_: I,
		_: I,
	) {
	}

	fn on_disabled(_validator_index: u32) {}
}

impl sp_runtime::BoundToRuntimeAppPublic for OtherSessionHandler {
	type Public = UintAuthorityId;
}

impl_opaque_keys! {
	#[derive(MaxEncodedLen, PartialOrd, Ord)]
	pub struct TestSessionKeys {
		pub other: OtherSessionHandler,
	}
}

impl pallet_partner_chains_session::Config for Test {
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ShouldEndSession = MockSessionManager;
	type NextSessionRotation = ();
	type SessionManager = MockSessionManager;
	type SessionHandler = (OtherSessionHandler,);
	type Keys = TestSessionKeys;
}

impl pallet_preimage::Config for Test {
	type WeightInfo = pallet_preimage::weights::SubstrateWeight<Test>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = ();
	type ManagerOrigin = EnsureRoot<AccountId>;
	type Consideration = ();
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub const MaxAuthorities: u32 = 50;
	pub const MaxPreimageSize: u32 = pallet_upgrade::MAX_PREIMAGE_SIZE;
	pub const MaxVoteTargets: u8 = 5;
	pub const UpgradeDelay: u32 = 5;
	pub const PalletUpgradeId: PalletId = PalletId(*b"hardfork");
	pub const UpgradeVoteThreshold: sp_arithmetic::Percent = sp_arithmetic::Percent::from_percent(70);
	pub const SessionsPerVotingPeriod: u32 = 4;

}

pub struct MockValidatorSet;
impl sp_runtime::traits::Get<BoundedVec<AuraId, MaxAuthorities>> for MockValidatorSet {
	fn get() -> BoundedVec<AuraId, MaxAuthorities> {
		let account_uints: Vec<u64> = vec![1, 2, 3, 4, 5, 6, 7];
		account_uints
			.into_iter()
			.map(|a| UintAuthorityId(a).to_public_key())
			.collect::<Vec<_>>()
			.try_into()
			.unwrap()
	}
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const BlockHashCount: u32 = 2400;
	pub BlockWeights: frame_system::limits::BlockWeights =
	frame_system::limits::BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub BlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;

	pub MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
		BlockWeights::get().max_block;
}

impl pallet_scheduler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaximumSchedulerWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	#[cfg(feature = "runtime-benchmarks")]
	type MaxScheduledPerBlock = ConstU32<512>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MaxScheduledPerBlock = ConstU32<50>;
	type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Test>;
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = frame_system::Pallet<Test>;
}

impl Config for Test {
	type WeightInfo = ();
	type AuthorityId = AuraId;
	type PalletId = PalletUpgradeId;
	type PalletsOrigin = OriginCaller;
	type MaxValidators = MaxAuthorities;
	type Scheduler = Scheduler;
	type UpgradeDelay = UpgradeDelay;
	type UpgradeVoteThreshold = UpgradeVoteThreshold;
	type ValidatorSet = MockValidatorSet;
	type MaxVoteTargets = MaxVoteTargets;
	type SessionsPerVotingPeriod = SessionsPerVotingPeriod;

	fn spec_version() -> RuntimeVersion {
		System::runtime_version()
	}

	fn current_authority() -> Option<AuraId> {
		let current_validator_index = System::block_number() % MockValidatorSet::get().len() as u64;
		let current_authority = &MockValidatorSet::get()[current_validator_index as usize];
		Some(current_authority.clone())
	}

	type Preimage = Preimage;
	type SetCode = ();
}

pub fn start_session(session_index: SessionIndex) {
	for i in Session::current_index()..session_index {
		System::on_finalize(System::block_number());
		Session::on_finalize(System::block_number());

		let parent_hash = if System::block_number() > 1 {
			let hdr = System::finalize();
			hdr.hash()
		} else {
			System::parent_hash()
		};

		System::reset_events();
		System::initialize(&(i as u64 + 1), &parent_hash, &Default::default());
		System::set_block_number((i + 1).into());

		<System as Hooks<u64>>::on_initialize(System::block_number());
		<Session as Hooks<u64>>::on_initialize(System::block_number());
	}
}

pub(crate) fn new_test_ext() -> TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	TestExternalities::new(t)
}

pub fn upgrade_events() -> Vec<super::Event<Test>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Upgrade(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>()
}

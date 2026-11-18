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

use crate::{
	self as pallet_block_rewards, ActivationPrerequisites, LockOwner, PayoutFailure, RewardLedger,
	RewardPayee, RewardPayout, SnapshotPayee, SnapshotPool, StakeCredential, StakeSnapshot,
};
use core::cell::Cell;
use core::cell::RefCell;
use core::ops::Not;
use frame_support::derive_impl;
use frame_support::weights::Weight;
use midnight_primitives_block_production::{
	Attribution, BlockProducer, EpochLabel, OperatorId, PoolId, Producer,
};
use midnight_primitives_block_rewards::{BlockUsage, CardanoEpoch, UsageDimension, Utilization};
use midnight_primitives_block_rewards::{Ratio, RewardParametersV1};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system = 0,
		BlockRewards: pallet_block_rewards = 1,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

/// Mock ledger state kept in storage, so a rolled-back settlement rolls it back too.
pub struct Stored<T>(&'static [u8], core::marker::PhantomData<T>);

impl<T: Encode + Decode + Default> Stored<T> {
	const fn new(key: &'static [u8]) -> Self {
		Self(key, core::marker::PhantomData)
	}
	pub fn get(&self) -> T {
		frame_support::storage::unhashed::get_or_default(self.0)
	}
	pub fn set(&self, value: T) {
		frame_support::storage::unhashed::put(self.0, &value);
	}
	pub fn with_borrow<R>(&self, f: impl FnOnce(&T) -> R) -> R {
		f(&self.get())
	}
	pub fn with_borrow_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
		let mut value = self.get();
		let result = f(&mut value);
		self.set(value);
		result
	}
}

pub static LEDGER_POOL: Stored<u128> = Stored::new(b":mock:ledger_pool");
pub static TREASURY_PAYMENTS: Stored<Vec<u128>> = Stored::new(b":mock:treasury_payments");
pub static DISTRIBUTIONS: Stored<Vec<Vec<RewardPayout>>> = Stored::new(b":mock:distributions");
/// What the block has used of the mock ledger's single limited dimension.
pub static BLOCK_USED: Stored<u64> = Stored::new(b":mock:block_used");

/// The mock ledger's limit on its one limited dimension.
pub const BLOCK_LIMIT: u64 = 1_000;

std::thread_local! {
	pub static RESERVE_ANCHOR_MS: Cell<Option<u64>> = const { Cell::new(Some(0)) };
	pub static NOW_MS: Cell<u64> = const { Cell::new(0) };
	pub static CARDANO_EPOCH_SOURCE_LIVE: Cell<bool> = const { Cell::new(true) };
	pub static CARDANO_EPOCH: Cell<CardanoEpoch> = const { Cell::new(CardanoEpoch(0)) };
	pub static ATTRIBUTION: Cell<Attribution> =
		const { Cell::new(Attribution { epoch: EpochLabel(0), producer: None }) };
	pub static UTILIZATION: Cell<Utilization> = const { Cell::new(Utilization { used: 0, max: 1 }) };
	pub static RELEASED: Cell<u128> = const { Cell::new(0) };
	pub static PAYOUT_FAILURE: Cell<Option<PayoutFailure>> = const { Cell::new(None) };
	pub static LEDGER_POOL_UNREADABLE: Cell<bool> = const { Cell::new(false) };
	/// `None`: the ledger's `reserve_pool` is `initial_reward_reserve_star − L`.
	pub static RESERVE_POOL: Cell<Option<u128>> = const { Cell::new(None) };
	pub static SNAPSHOT: RefCell<SnapshotPools> = const { RefCell::new(SnapshotPools::new()) };
	/// Every operator not recorded in `SNAPSHOT` is a pool whose owners hold all its stake.
	pub static EVERY_OPERATOR_A_POOL: Cell<bool> = const { Cell::new(true) };
	pub static LOCK_OWNERS: RefCell<LockOwners> = const { RefCell::new(LockOwners::new()) };
	/// What one Treasury payment or output uses of the block limit.
	pub static ENTRY_COST: Cell<u64> = const { Cell::new(1) };
	/// What the block's extrinsics use before `on_idle`.
	pub static EXTRINSICS_USE: Cell<u64> = const { Cell::new(0) };
	pub static USAGE_UNREADABLE: Cell<bool> = const { Cell::new(false) };
	pub static DISTRIBUTION_FAILURE: Cell<Option<PayoutFailure>> = const { Cell::new(None) };
	/// Bumped to make the provider serve a different record for an epoch.
	pub static SNAPSHOT_REVISION: Cell<u8> = const { Cell::new(0) };
}

pub fn operator(byte: u8) -> OperatorId {
	OperatorId::AdaSpo(PoolId::decode(&mut &[byte; 28][..]).unwrap())
}

/// A block of Midnight epoch `epoch` whose committee comes from Cardano epoch `epoch`.
pub fn produced_by(epoch: u64, producer: Option<Producer>) {
	produced_in(epoch, epoch as u32, producer);
}

pub fn produced_in(midnight_epoch: u64, cardano_epoch: u32, producer: Option<Producer>) {
	ATTRIBUTION.set(Attribution { epoch: EpochLabel(midnight_epoch), producer });
	CARDANO_EPOCH.set(CardanoEpoch(cardano_epoch));
}

pub struct MockCardanoEpoch;
impl frame_support::traits::Get<CardanoEpoch> for MockCardanoEpoch {
	fn get() -> CardanoEpoch {
		CARDANO_EPOCH.get()
	}
}

pub type LockOwners = alloc::collections::BTreeMap<(CardanoEpoch, StakeCredential), LockOwner>;

/// Pools of each epoch's snapshot, with their payees in order.
pub type SnapshotPools =
	alloc::collections::BTreeMap<(CardanoEpoch, OperatorId), (SnapshotPool, Vec<SnapshotPayee>)>;

/// A pool whose owners hold all its stake, so its whole total goes to the operator.
pub fn sole_pool() -> SnapshotPool {
	SnapshotPool {
		terms: midnight_primitives_block_rewards::PoolTerms {
			fixed_cost: 0,
			margin: midnight_primitives_block_rewards::Ratio::new(0, 1),
		},
		reward_address: midnight_primitives_block_rewards::RewardAddress([0xee; 32]),
		operator_stake: 1,
		total_stake: 1,
	}
}

pub struct MockSnapshot;
impl StakeSnapshot for MockSnapshot {
	/// Complete once a pool of the epoch is recorded, or always while `EVERY_OPERATOR_A_POOL`
	/// is set.
	fn snapshot_id(cardano_epoch: CardanoEpoch) -> Option<[u8; 32]> {
		let recorded =
			SNAPSHOT.with_borrow(|pools| pools.keys().any(|(epoch, _)| *epoch == cardano_epoch));
		let mut id = [cardano_epoch.0 as u8; 32];
		id[0] ^= SNAPSHOT_REVISION.get();
		(recorded || EVERY_OPERATOR_A_POOL.get()).then_some(id)
	}
	fn pool(cardano_epoch: CardanoEpoch, operator: &OperatorId) -> Option<SnapshotPool> {
		SNAPSHOT
			.with_borrow(|pools| pools.get(&(cardano_epoch, *operator)).map(|(pool, _)| *pool))
			.or_else(|| EVERY_OPERATOR_A_POOL.get().then_some(sole_pool()))
	}
	fn lock_owners(
		cardano_epoch: CardanoEpoch,
		after: Option<StakeCredential>,
		limit: u32,
	) -> Vec<LockOwner> {
		LOCK_OWNERS.with_borrow(|owners| {
			owners
				.iter()
				.filter(|((epoch, credential), _)| {
					*epoch == cardano_epoch && after.is_none_or(|after| *credential > after)
				})
				.take(limit as usize)
				.map(|(_, owner)| *owner)
				.collect()
		})
	}
	fn payees(
		cardano_epoch: CardanoEpoch,
		operator: &OperatorId,
		after: Option<RewardPayee>,
		limit: u32,
	) -> Vec<SnapshotPayee> {
		SNAPSHOT.with_borrow(|pools| {
			pools.get(&(cardano_epoch, *operator)).map_or_else(Vec::new, |(_, payees)| {
				payees
					.iter()
					.filter(|payee| after.is_none_or(|after| payee.payee > after))
					.take(limit as usize)
					.copied()
					.collect()
			})
		})
	}
}

pub struct MockBlockProducer;
impl BlockProducer for MockBlockProducer {
	fn current() -> Attribution {
		ATTRIBUTION.get()
	}
}

pub struct MockReleased;
impl frame_support::traits::Get<u128> for MockReleased {
	fn get() -> u128 {
		RELEASED.get()
	}
}

pub struct MockLedger;
impl RewardLedger for MockLedger {
	fn block_reward_pool() -> Option<u128> {
		LEDGER_POOL_UNREADABLE.get().not().then(|| LEDGER_POOL.get())
	}
	fn reserve_pool() -> Option<u128> {
		LEDGER_POOL_UNREADABLE.get().not().then(|| {
			RESERVE_POOL
				.get()
				.unwrap_or(parameters(1).initial_reward_reserve_star - RELEASED.get())
		})
	}
	fn pay_block_rewards_to_treasury(amount: u128) -> Result<(), PayoutFailure> {
		if let Some(failure) = PAYOUT_FAILURE.get() {
			return Err(failure);
		}
		LEDGER_POOL.set(LEDGER_POOL.get().checked_sub(amount).ok_or(PayoutFailure::Rejected)?);
		use_block(1)?;
		TREASURY_PAYMENTS.with_borrow_mut(|payments| payments.push(amount));
		Ok(())
	}
	fn treasury_payment_weight() -> Weight {
		Weight::from_parts(1_000_000, 0)
	}
	fn distribute_night(outputs: Vec<RewardPayout>) -> Result<(), PayoutFailure> {
		if let Some(failure) = DISTRIBUTION_FAILURE.get() {
			return Err(failure);
		}
		let total: u128 = outputs.iter().map(|output| output.amount).sum();
		LEDGER_POOL.set(LEDGER_POOL.get().checked_sub(total).ok_or(PayoutFailure::Rejected)?);
		use_block(outputs.len())?;
		DISTRIBUTIONS.with_borrow_mut(|distributions| distributions.push(outputs));
		Ok(())
	}
	fn distribution_weight(outputs: u32) -> Weight {
		Weight::from_parts(100_000, 0).saturating_mul(outputs.into())
	}
	fn block_usage() -> Option<BlockUsage> {
		USAGE_UNREADABLE.get().not().then(|| usage_of_block(BLOCK_USED.get()))
	}
}

/// Adds `entries` ledger entries to the block, or refuses them as the ledger would.
fn use_block(entries: usize) -> Result<(), PayoutFailure> {
	let used = BLOCK_USED.get().saturating_add(ENTRY_COST.get().saturating_mul(entries as u64));
	if used > BLOCK_LIMIT {
		return Err(PayoutFailure::BlockFull);
	}
	BLOCK_USED.set(used);
	Ok(())
}

/// A block that used `used` of the mock ledger's limited dimension.
pub fn usage_of_block(used: u64) -> BlockUsage {
	usage_of(Utilization { used: used.into(), max: BLOCK_LIMIT.into() })
}

/// A block that used `utilization` of one dimension of the ledger's limit and nothing else.
pub fn usage_of(utilization: Utilization) -> BlockUsage {
	let mut usage = BlockUsage::default();
	usage.0[0] = UsageDimension { used: utilization.used as u64, limit: utilization.max as u64 };
	usage
}

pub struct MockPrerequisites;
impl ActivationPrerequisites for MockPrerequisites {
	fn reserve_anchor_ms() -> Option<u64> {
		RESERVE_ANCHOR_MS.get()
	}
	fn cardano_epoch_source_live() -> bool {
		CARDANO_EPOCH_SOURCE_LIVE.get()
	}
}

pub struct MockTime;
impl frame_support::traits::UnixTime for MockTime {
	fn now() -> core::time::Duration {
		core::time::Duration::from_millis(NOW_MS.get())
	}
}

impl pallet_block_rewards::Config for Test {
	type GovernanceOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type Ledger = MockLedger;
	type Prerequisites = MockPrerequisites;
	type BlockProducer = MockBlockProducer;
	type Time = MockTime;
	type Released = MockReleased;
	type SlotDurationMs = frame_support::traits::ConstU64<6_000>;
	type CardanoEpoch = MockCardanoEpoch;
	type Snapshot = MockSnapshot;
	type WeightInfo = ();
}

/// π = 3/100, B = 1/4, T = 1/20, six-second blocks.
pub fn parameters(version: u32) -> RewardParametersV1 {
	RewardParametersV1 {
		version,
		initial_reward_reserve_star: 6_000_000_000_873_988,
		initial_expansion: Ratio::new(3, 100),
		reserve_allocation: Ratio::new(1, 4),
		treasury_allocation: Ratio::new(1, 20),
		expected_blocks_per_year: 5_256_000,
		distribution_rate: Ratio::new(7, 438_000_000),
		fixed_subsidy: Ratio::new(95, 100),
		recipient_split_policy: [0; 32],
		recipient_policy_hash: [0; 32],
		publication_hash: [0; 32],
		governance_action_hash: [0; 32],
		max_updates_per_block: 100,
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	RESERVE_ANCHOR_MS.set(Some(0));
	NOW_MS.set(0);
	SNAPSHOT_REVISION.set(0);
	CARDANO_EPOCH_SOURCE_LIVE.set(true);
	produced_by(0, None);
	UTILIZATION.set(Utilization { used: 0, max: 1 });
	RELEASED.set(0);
	PAYOUT_FAILURE.set(None);
	SNAPSHOT.with_borrow_mut(SnapshotPools::clear);
	EVERY_OPERATOR_A_POOL.set(true);
	LOCK_OWNERS.with_borrow_mut(LockOwners::clear);
	DISTRIBUTION_FAILURE.set(None);
	LEDGER_POOL_UNREADABLE.set(false);
	RESERVE_POOL.set(None);
	ENTRY_COST.set(1);
	EXTRINSICS_USE.set(0);
	USAGE_UNREADABLE.set(false);
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use super::*;
use frame_support::{assert_ok, derive_impl, parameter_types, traits::Hooks};
use pallet_authorship::EventHandler;
use parity_scale_codec::Encode;
use sp_core::Pair;
use sp_runtime::{
	BuildStorage, generic,
	traits::{BlakeTwo256, IdentityLookup},
};

type Extrinsic =
	generic::UncheckedExtrinsic<AccountId32, RuntimeCall, sp_runtime::MultiSignature, ()>;
type Block = generic::Block<generic::Header<u64, BlakeTwo256>, Extrinsic>;
frame_support::construct_runtime!(pub enum Test { System: frame_system = 0, BlockRewards: crate = 12 });
parameter_types! {
	pub Weights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights::builder()
		.base_block(Weight::zero())
		.for_class(DispatchClass::all(), |w| w.base_extrinsic = Weight::zero())
		.for_class(DispatchClass::Normal, |w| w.max_total = Some(Weight::from_parts(1000, 1000)))
		.for_class(DispatchClass::Operational, |w| {w.max_total = Some(Weight::from_parts(2000,2000)); w.reserved = Some(Weight::from_parts(1000,1000));})
		.build_or_panic();
}
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<AccountId32>;
	type BlockWeights = Weights;
}
pub struct TestAuthorPool;
impl AuthorPool for TestAuthorPool {
	fn pool_for_author(author: &AccountId32) -> Option<PoolId> {
		(*author == AccountId32::new([4; 32])).then_some([4; 28])
	}
}
impl Config for Test {
	type AuthorPool = TestAuthorPool;
}
fn ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	GenesisConfig::<Test> {
		config: midnight_primitives_block_rewards::BlockRewardsConfig {
			reserve: 438_000_000_000,
			..Default::default()
		},
		_config: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.unwrap();
	storage.into()
}
fn account(n: u8) -> RewardAccount {
	RewardAccount {
		skh: [n; 28],
		deposit: 3_000_000,
		committed: false,
		registration: Some(Registration { operator_keys: Vec::new(), payout_threshold: Some(0) }),
	}
}
#[test]
fn per_block_rounds_each_component_down() {
	ext().execute_with(|| {
		let author = AccountId32::new([3; 32]);
		System::register_extra_weight_unchecked(Weight::from_parts(337, 0), DispatchClass::Normal);
		BlockRewards::note_author(author.clone());
		BlockRewards::on_finalize(1);
		// Nb=7000, Nf=6650, Nv=floor(350*337/1000)=117.
		assert_eq!(AuthorAccrued::<Test>::get(author), 6767);
		assert_eq!(EpochTreasury::<Test>::get(), 233);
		assert_eq!(Reserve::<Test>::get(), 438_000_000_000 - 7000);
	});
}
#[test]
fn operator_requires_valid_proof_of_possession() {
	let pair = sp_core::ecdsa::Pair::from_seed(&[7; 32]);
	let mut a = account(1);
	assert_eq!(paired_author(&a), None);
	let mut message = b"midnight:rewards-operator".to_vec();
	message.extend_from_slice(&a.skh);
	let sig = pair.sign(&message);
	a.registration.as_mut().unwrap().operator_keys = alloc::vec![
		(b"sidechain".to_vec(), pair.public().0.to_vec()),
		(b"sidechain_sig".to_vec(), sig.0.to_vec())
	];
	assert_eq!(
		paired_author(&a),
		Some(AccountId32::new(sp_io::hashing::blake2_256(&pair.public().0)))
	);
	a.registration.as_mut().unwrap().operator_keys[0].1 =
		sp_core::ecdsa::Pair::from_seed(&[8; 32]).public().0.to_vec();
	assert_eq!(paired_author(&a), None);
	a.registration.as_mut().unwrap().operator_keys.pop();
	assert_eq!(paired_author(&a), None);
}
#[test]
fn selection_uses_eligibility_cap_and_persistent_rotation() {
	ext().execute_with(|| {
		MaxLeaves::<Test>::put(2);
		let mut accounts: Vec<_> = (1..=7).map(account).collect();
		accounts[0].registration = None;
		accounts[1].committed = true;
		accounts[2].deposit -= 1;
		accounts[3].registration.as_mut().unwrap().payout_threshold = Some(10);
		for n in 1..=7 {
			Balances::<Test>::insert([n; 28], 10);
		}
		BlockRewards::close_epoch(4);
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			5,
			accounts.clone(),
			vec![]
		));
		assert_eq!(
			EpochLeaves::<Test>::get(4),
			alloc::vec![reward_leaf([5; 28], 10), reward_leaf([6; 28], 10)]
		);
		assert_eq!(RotationCursor::<Test>::get(), [7; 28]);
		assert_eq!(Balances::<Test>::get([4; 28]), 10);
		assert_eq!(Balances::<Test>::get([5; 28]), 0);
		BlockRewards::close_epoch(5);
		assert_ok!(BlockRewards::note_reward_accounts(RuntimeOrigin::none(), 6, accounts, vec![]));
		assert_eq!(EpochLeaves::<Test>::get(5), alloc::vec![reward_leaf([7; 28], 10)]);
		assert_eq!(RotationCursor::<Test>::get(), [7; 28]);
	});
}
#[test]
fn zero_balance_qualifies_when_fee_and_threshold_are_zero() {
	ext().execute_with(|| {
		DistFee::<Test>::put(0);
		BlockRewards::close_epoch(0);
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			1,
			alloc::vec![account(1)],
			vec![]
		));
		assert_eq!(EpochLeaves::<Test>::get(0), alloc::vec![reward_leaf([1; 28], 0)]);
	});
}
#[test]
fn bare_digest_has_contract_wire_layout() {
	let call = RuntimeCall::BlockRewards(Call::submit_rewards_digest {
		epoch: 7,
		leaf_count: 3,
		root: [1; 32],
		min_key: [2; 28],
		max_key: [3; 28],
		treasury_total: 17,
	});
	let encoded = Extrinsic::new_bare(call).encode();
	assert_eq!(encoded.len(), 125);
	assert_eq!(&encoded[..5], &[0xed, 0x01, 0x05, 0x0c, 0x00]);
	assert_eq!(&encoded[5..13], &7u64.to_le_bytes());
	assert_eq!(&encoded[13..21], &3u64.to_le_bytes());
	assert_eq!(&encoded[109..], &17u128.to_le_bytes());
}
#[test]
fn merkle_root_promotes_odd_node_like_aiken_builder() {
	let leaves = [reward_leaf([1; 28], 10), reward_leaf([2; 28], 20), reward_leaf([3; 28], 30)];
	let left = sp_io::hashing::keccak_256(
		&[sp_io::hashing::keccak_256(&leaves[0]), sp_io::hashing::keccak_256(&leaves[1])].concat(),
	);
	let expected =
		sp_io::hashing::keccak_256(&[left, sp_io::hashing::keccak_256(&leaves[2])].concat());
	let actual: [u8; 32] = binary_merkle_tree::merkle_root::<Keccak256, _>(leaves).into();
	assert_eq!(actual, expected);
	assert_eq!(
		actual,
		[
			0xa0, 0x9d, 0x65, 0x7, 0x13, 0xcc, 0xa2, 0x3, 0x26, 0x92, 0x25, 0xab, 0xf9, 0x8c, 0x7e,
			0x9e, 0xbe, 0x12, 0x26, 0xfb, 0x90, 0x3a, 0x88, 0x1, 0xb6, 0xd2, 0x5, 0x4d, 0x59, 0x7e,
			0x5c, 0x3d
		]
	);
}
#[test]
fn empty_epoch_uses_zero_root_and_transfers_treasury() {
	ext().execute_with(|| {
		EpochTreasury::<Test>::put(29);
		BlockRewards::close_epoch(8);
		BlockRewards::on_finalize(1);
		assert_eq!(
			PendingDigest::<Test>::get(),
			Some(RewardsDigest {
				epoch: 8,
				leaf_count: 0,
				root: [0; 32],
				min_key: [0; 28],
				max_key: [0; 28],
				treasury_total: 29
			})
		);
		assert_eq!(EpochTreasury::<Test>::get(), 0);
	});
}

#[test]
fn fresh_registration_moves_author_accrual_before_selection() {
	ext().execute_with(|| {
		let pair = sp_core::ecdsa::Pair::from_seed(&[7; 32]);
		let mut account = account(9);
		let mut message = b"midnight:rewards-operator".to_vec();
		message.extend_from_slice(&account.skh);
		let signature = pair.sign(&message);
		account.registration.as_mut().unwrap().operator_keys = alloc::vec![
			(b"sidechain".to_vec(), pair.public().0.to_vec()),
			(b"sidechain_sig".to_vec(), signature.0.to_vec()),
		];
		let author = paired_author(&account).unwrap();
		AuthorAccrued::<Test>::insert(&author, 1);
		VirtualAccountPolicy::<Test>::put([1; 28]);
		BlockRewards::close_epoch(5);
		assert!(PendingDigest::<Test>::get().is_none());
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			6,
			alloc::vec![account.clone()],
			vec![]
		));
		assert_eq!(EpochLeaves::<Test>::get(5), alloc::vec![reward_leaf([9; 28], 1)]);
		assert_eq!(AuthorAccrued::<Test>::get(author), 0);
		BlockRewards::close_epoch(6);
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			7,
			alloc::vec![account],
			vec![]
		));
		assert!(EpochLeaves::<Test>::get(6).is_empty());
	});
}

#[test]
fn unbounded_threshold_and_overflowing_fee_sum_are_ineligible() {
	ext().execute_with(|| {
		let mut accounts = alloc::vec![account(1), account(2)];
		accounts[0].registration.as_mut().unwrap().payout_threshold = None;
		accounts[1].registration.as_mut().unwrap().payout_threshold = Some(u128::MAX);
		Balances::<Test>::insert([1; 28], u128::MAX);
		Balances::<Test>::insert([2; 28], u128::MAX);
		BlockRewards::close_epoch(0);
		assert_ok!(BlockRewards::note_reward_accounts(RuntimeOrigin::none(), 1, accounts, vec![]));
		assert!(EpochLeaves::<Test>::get(0).is_empty());
	});
}

#[test]
fn typescript_noble_signature_pairs_operator() {
	let key = [
		0x2, 0x98, 0x9c, 0xb, 0x76, 0xcb, 0x56, 0x39, 0x71, 0xfd, 0xc9, 0xbe, 0xf3, 0x1e, 0xc0,
		0x6c, 0x35, 0x60, 0xf3, 0x24, 0x9d, 0x6e, 0xe9, 0xe5, 0xd8, 0x3c, 0x57, 0x62, 0x55, 0x96,
		0xe0, 0x5f, 0x6f,
	];
	let signature = [
		0xdf, 0x41, 0x9a, 0xc, 0x94, 0x83, 0x6f, 0x8c, 0x7d, 0x54, 0xf6, 0x1f, 0xeb, 0x8a, 0x8b,
		0xf3, 0x2f, 0x78, 0x2d, 0xf4, 0xd0, 0xb0, 0x65, 0x11, 0x33, 0xfa, 0x20, 0xfe, 0x8c, 0x8d,
		0x1f, 0xbe, 0x77, 0x14, 0x75, 0xa8, 0xad, 0x30, 0xf0, 0xaa, 0x44, 0xbf, 0x6a, 0x87, 0xea,
		0x5a, 0x67, 0x5b, 0x80, 0x8c, 0x17, 0xfd, 0x93, 0x71, 0xc3, 0xba, 0x14, 0x86, 0xe4, 0x28,
		0xe9, 0xcf, 0xba, 0x7e, 0x0,
	];
	let mut account = account(1);
	account.registration.as_mut().unwrap().operator_keys = alloc::vec![
		(b"sidechain".to_vec(), key.to_vec()),
		(b"sidechain_sig".to_vec(), signature.to_vec())
	];
	assert_eq!(paired_author(&account), Some(AccountId32::new(sp_io::hashing::blake2_256(&key))));
}

#[test]
fn registered_author_accrues_to_pool() {
	ext().execute_with(|| {
		let author = AccountId32::new([4; 32]);
		BlockRewards::note_author(author.clone());
		BlockRewards::on_finalize(1);
		assert_eq!(PoolAccrued::<Test>::get([4; 28]), 6650);
		assert_eq!(AuthorAccrued::<Test>::get(author), 0);
	});
}

fn pool_snapshot() -> PoolSnapshot {
	PoolSnapshot {
		pool_id: [4; 28],
		margin: 100_000_000,
		reward_account: [9; 28],
		owners: vec![[1; 28]],
		delegators: vec![([1; 28], 20), ([2; 28], 30), ([3; 28], 50)],
	}
}

#[test]
fn split_excludes_owners_and_retains_margin_owner_share_and_rounding() {
	ext().execute_with(|| {
		PoolAccrued::<Test>::insert([4; 28], 101);
		BlockRewards::close_epoch(3);
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			4,
			vec![],
			vec![pool_snapshot()]
		));
		assert_eq!(Balances::<Test>::get([1; 28]), 0);
		assert_eq!(Balances::<Test>::get([2; 28]), 27);
		assert_eq!(Balances::<Test>::get([3; 28]), 45);
		assert_eq!(Balances::<Test>::get([9; 28]), 29);
		assert!(!PoolAccrued::<Test>::contains_key([4; 28]));
	});
}

#[test]
fn zero_stake_pays_operator_and_missing_snapshot_retains_accrual() {
	ext().execute_with(|| {
		PoolAccrued::<Test>::insert([4; 28], 101);
		PoolAccrued::<Test>::insert([5; 28], 99);
		let mut pool = pool_snapshot();
		pool.delegators.clear();
		BlockRewards::close_epoch(3);
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			4,
			vec![],
			vec![pool]
		));
		assert_eq!(Balances::<Test>::get([9; 28]), 101);
		assert_eq!(PoolAccrued::<Test>::get([5; 28]), 99);
	});
}

#[test]
fn split_uses_full_width_multiplication() {
	ext().execute_with(|| {
		PoolAccrued::<Test>::insert([4; 28], u128::MAX);
		let mut pool = pool_snapshot();
		pool.margin = 0;
		pool.delegators = vec![([2; 28], u64::MAX.into())];
		BlockRewards::close_epoch(3);
		assert_ok!(BlockRewards::note_reward_accounts(
			RuntimeOrigin::none(),
			4,
			vec![],
			vec![pool]
		));
		assert_eq!(Balances::<Test>::get([2; 28]), u128::MAX);
		assert_eq!(Balances::<Test>::get([9; 28]), 0);
	});
}

#[test]
fn inherent_rejects_changed_pool_snapshot() {
	ext().execute_with(|| {
		let mut data = sp_inherents::InherentData::new();
		data.put_data(
			ACCOUNTS_INHERENT_IDENTIFIER,
			&RewardAccountsData {
				epoch: 4,
				accounts: vec![],
				pool_snapshots: vec![pool_snapshot()],
			},
		)
		.unwrap();
		let call = BlockRewards::create_inherent(&data).unwrap();
		assert!(BlockRewards::check_inherent(&call, &data).is_ok());
		let mut changed = pool_snapshot();
		changed.margin += 1;
		let changed_call = Call::note_reward_accounts {
			epoch: 4,
			accounts: vec![],
			pool_snapshots: vec![changed],
		};
		assert!(matches!(
			BlockRewards::check_inherent(&changed_call, &data),
			Err(InherentError::AccountsMismatch)
		));
	});
}

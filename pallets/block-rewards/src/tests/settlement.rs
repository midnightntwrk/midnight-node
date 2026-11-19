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

use super::*;
use frame_support::{Hashable, traits::Get, weights::RuntimeDbWeight};
use midnight_primitives_block_production::Producer;
use midnight_primitives_block_rewards::{PoolTerms, Utilization, split_pool};

const E5: CardanoEpoch = CardanoEpoch(5);

const TERMS_1: PoolTerms = PoolTerms { fixed_cost: 0, margin: Ratio { n: 0, d: 1 } };
const TERMS_2: PoolTerms = PoolTerms { fixed_cost: 1_000, margin: Ratio { n: 1, d: 10 } };

fn record_pool(byte: u8, pool: SnapshotPool, payees: Vec<SnapshotPayee>) {
	SNAPSHOT.with_borrow_mut(|pools| {
		pools.insert((E5, operator(byte)), (pool, payees));
	});
}

/// Pool 1 pays its operator at 0x90 and a lock at 0x01. Pool 2 pays its operator at 0x20, a
/// delegator and a lock both at 0x05, and a delegator with no address, whose share goes to
/// escrow.
fn record_both_pools() {
	record_pool(
		1,
		SnapshotPool {
			terms: TERMS_1,
			reward_address: address(0x90),
			operator_stake: 500,
			total_stake: 1_000,
		},
		vec![lock(7, 500, Some(0x01))],
	);
	record_pool(
		2,
		SnapshotPool {
			terms: TERMS_2,
			reward_address: address(0x20),
			operator_stake: 100,
			total_stake: 1_000,
		},
		vec![delegator(3, 300, Some(0x05)), delegator(4, 200, None), lock(6, 400, Some(0x05))],
	);
}

/// Operators 1 and 2 produced epoch 5 (blocks 2 to 4), closed in block 5. `L` covers
/// everything drawn and the ledger pool holds it.
fn epoch_5_closed() -> sp_io::TestExternalities {
	let mut ext = installed();
	ext.execute_with(|| {
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		produced_by(5, Some(Producer::Operator(operator(2))));
		finalize(2);
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(3);
		finalize(4);
		produced_by(6, Some(Producer::Permissioned));
		finalize(5);
		RELEASED.set(Drawn::<Test>::get());
		LEDGER_POOL.set(Drawn::<Test>::get());
		System::reset_events();
	});
	ext
}

/// Epoch 5 settled in block 6, delegator 4's share held in escrow.
pub(super) fn epoch_5_settled() -> sp_io::TestExternalities {
	let mut ext = epoch_5_closed();
	ext.execute_with(|| {
		record_both_pools();
		initialize(6);
	});
	ext
}

fn bucket(byte: u8) -> u128 {
	Buckets::<Test>::get(E5, operator(byte))
}

fn treasury() -> u128 {
	ClosedEpochs::<Test>::get(E5).unwrap().treasury
}

/// What each destination should receive for epoch 5.
fn expected() -> Vec<(Destination, u128)> {
	let (operator_1, shares_1) = split_pool(bucket(1), TERMS_1, 500, &[500]).unwrap();
	let (operator_2, shares_2) = split_pool(bucket(2), TERMS_2, 100, &[300, 200, 400]).unwrap();
	vec![
		(Destination::Address(address(0x01)), shares_1[0]),
		(Destination::Address(address(0x05)), shares_2[0] + shares_2[2]),
		(Destination::Address(address(0x20)), operator_2),
		(Destination::Address(address(0x90)), operator_1),
		(Destination::Escrow(credential(4)), shares_2[1]),
	]
}

fn distributed() -> Vec<(RewardAddress, u128)> {
	DISTRIBUTIONS.with_borrow(|calls| {
		calls.iter().flatten().map(|output| (output.address, output.amount)).collect()
	})
}

fn expected_outputs() -> Vec<(RewardAddress, u128)> {
	expected()
		.into_iter()
		.filter_map(|(destination, amount)| match destination {
			Destination::Address(address) => Some((address, amount)),
			Destination::Escrow(_) => None,
		})
		.collect()
}

fn set_budget(updates: u32) {
	ParameterSets::<Test>::mutate(1, |parameters| {
		let Some(RewardParameters::V1(parameters)) = parameters else { unreachable!() };
		parameters.max_updates_per_block = updates
	});
}

#[test]
fn one_block_settles_the_epoch_in_address_order_one_output_per_address() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let (pools, treasury) = (bucket(1) + bucket(2), treasury());
		let (outputs, escrowed) = (expected_outputs(), expected().last().unwrap().1);
		initialize(6);

		assert_eq!(DISTRIBUTIONS.with_borrow(Vec::len), 1);
		assert_eq!(distributed(), outputs);
		assert_eq!(Escrow::<Test>::get(credential(4)), escrowed);
		assert_eq!(EscrowTotal::<Test>::get(), escrowed);
		assert_eq!(BlockRewards::state().escrow_total, escrowed);
		assert_eq!(Settled::<Test>::get(), treasury + pools);
		assert_eq!(Settlement::<Test>::get(), None);
		assert_eq!(Buckets::<Test>::iter_prefix(E5).count(), 0);
		assert_eq!(Payouts::<Test>::iter_prefix(E5).count(), 0);
		assert_eq!(last_event(), Event::EpochSettled { cardano_epoch: E5 });
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn a_small_budget_settles_over_several_blocks_still_in_address_order() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		set_budget(2);
		let outputs = expected_outputs();
		let mut block = 6;
		while Settlement::<Test>::get().is_some() {
			initialize(block);
			assert_eq!(BlockRewards::do_try_state(), Ok(()), "block {block}");
			block += 1;
			assert!(block < 40);
		}
		assert!(DISTRIBUTIONS.with_borrow(Vec::len) > 1);
		assert_eq!(distributed(), outputs);
		assert_eq!(SettlementHalted::<Test>::get(), None);
	});
}

#[test]
fn a_payout_waits_whole_until_the_release_covers_it() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let outputs = expected_outputs();
		let first = outputs[0].1;
		RELEASED.set(treasury() + first + 1);
		initialize(6);
		assert_eq!(distributed(), [(address(0x01), first)]);
		assert_eq!(Settlement::<Test>::get().unwrap().entry, SettlementEntry::Payout);
		assert_eq!(SettlementHalted::<Test>::get(), None);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));

		RELEASED.set(Drawn::<Test>::get());
		initialize(7);
		assert_eq!(distributed(), outputs);
		assert_eq!(Settlement::<Test>::get(), None);
	});
}

#[test]
fn a_snapshot_record_gone_before_the_split_halts() {
	epoch_5_closed().execute_with(|| {
		EVERY_OPERATOR_A_POOL.set(false);
		initialize(6);
		assert_eq!(payments(), [treasury()]);
		assert!(matches!(
			Settlement::<Test>::get().unwrap().entry,
			SettlementEntry::Split { operator: o, .. } if o == operator(1)
		));
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn a_pool_missing_from_a_complete_snapshot_halts() {
	epoch_5_closed().execute_with(|| {
		EVERY_OPERATOR_A_POOL.set(false);
		record_pool(
			1,
			SnapshotPool {
				terms: TERMS_1,
				reward_address: address(0x90),
				operator_stake: 1,
				total_stake: 1,
			},
			Vec::new(),
		);
		initialize(6);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(payments(), [treasury()]);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn a_lock_without_an_address_halts() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		SNAPSHOT.with_borrow_mut(|pools| {
			pools.get_mut(&(E5, operator(1))).unwrap().1 = vec![lock(7, 500, None)];
		});
		initialize(6);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
	});
}

#[test]
fn a_rejected_distribution_halts_and_rolls_the_block_back() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		DISTRIBUTION_FAILURE.set(Some(PayoutFailure::Rejected));
		initialize(6);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::DistributionFailed));
		assert_eq!(Settled::<Test>::get(), 0);
		assert_eq!(
			Settlement::<Test>::get(),
			Some(SettlementCursor { cardano_epoch: E5, entry: SettlementEntry::Treasury })
		);
		assert_eq!(Payouts::<Test>::iter_prefix(E5).count(), 0);
		assert_eq!(EscrowTotal::<Test>::get(), 0);
	});
}

#[test]
fn a_chunk_the_ledger_refuses_waits_for_a_later_block() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let outputs = expected_outputs();
		DISTRIBUTION_FAILURE.set(Some(PayoutFailure::BlockFull));
		initialize(6);
		// Halving reaches a chunk without outputs: the Treasury payment and the split settle.
		assert_eq!(SettlementHalted::<Test>::get(), None);
		assert!(distributed().is_empty());
		DISTRIBUTION_FAILURE.set(None);
		settle_until_done(7);
		assert_eq!(distributed(), outputs);
	});
}

#[test]
fn unreadable_block_usage_leaves_settlement_for_a_later_block() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let outputs = expected_outputs();
		USAGE_UNREADABLE.set(true);
		initialize(6);
		assert_eq!(Settled::<Test>::get(), 0);
		USAGE_UNREADABLE.set(false);
		settle_until_done(7);
		assert_eq!(distributed(), outputs);
	});
}

fn lock_owner(epoch: CardanoEpoch, byte: u8, to: Option<u8>) {
	LOCK_OWNERS.with_borrow_mut(|owners| {
		owners.insert(
			(epoch, credential(byte)),
			LockOwner { credential: credential(byte), reward_address: to.map(address) },
		);
	});
}

#[test]
fn escrow_from_the_same_epoch_settles_to_the_address_of_its_owners_locks() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		// Delegator 4 has no lock for pool 2, but its locks elsewhere all name 0x05.
		lock_owner(E5, 4, Some(0x05));
		let want = expected();
		let to_05 = want[1].1 + want[4].1;
		initialize(6);

		let outputs = distributed();
		assert_eq!(outputs.len(), 4);
		assert_eq!(outputs[1], (address(0x05), to_05));
		assert_eq!(EscrowTotal::<Test>::get(), 0);
		assert_eq!(Escrow::<Test>::get(credential(4)), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn earlier_escrow_is_released_in_a_later_epoch_without_changing_s() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let escrowed = expected()[4].1;
		initialize(6);
		assert_eq!(Escrow::<Test>::get(credential(4)), escrowed);

		// Epoch 6: one block of operator 1, closed in block 8; delegator 4 now has a lock.
		produced_by(6, Some(Producer::Operator(operator(1))));
		finalize(7);
		produced_by(7, Some(Producer::Permissioned));
		finalize(8);
		let e6 = CardanoEpoch(6);
		let pool = SnapshotPool {
			terms: TERMS_1,
			reward_address: address(0x90),
			operator_stake: 1,
			total_stake: 1,
		};
		SNAPSHOT.with_borrow_mut(|pools| {
			pools.insert((e6, operator(1)), (pool, Vec::new()));
		});
		lock_owner(e6, 4, Some(0x44));
		RELEASED.set(Drawn::<Test>::get());
		LEDGER_POOL.set(EscrowTotal::<Test>::get() + RELEASED.get() - Settled::<Test>::get());
		let settled_before = Settled::<Test>::get();
		let epoch_6 =
			Buckets::<Test>::get(e6, operator(1)) + ClosedEpochs::<Test>::get(e6).unwrap().treasury;
		DISTRIBUTIONS.with_borrow_mut(Vec::clear);

		initialize(9);
		assert!(distributed().contains(&(address(0x44), escrowed)));
		assert_eq!(Settled::<Test>::get(), settled_before + epoch_6);
		assert_eq!(EscrowTotal::<Test>::get(), 0);
		assert_eq!(Escrow::<Test>::get(credential(4)), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn an_owner_whose_locks_name_different_addresses_keeps_its_escrow() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		lock_owner(E5, 4, None);
		let escrowed = expected()[4].1;
		initialize(6);
		assert_eq!(Escrow::<Test>::get(credential(4)), escrowed);
		assert_eq!(EscrowTotal::<Test>::get(), escrowed);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn payouts_settle_in_byte_order_of_the_address_when_addresses_differ_late() {
	epoch_5_closed().execute_with(|| {
		// Addresses differing from the first byte, in the last byte only, or in the last two,
		// recorded in scrambled order.
		let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
		let addresses: Vec<RewardAddress> = (0..60u16)
			.map(|i| {
				seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
				let mut bytes = [0xAB; 32];
				match i % 3 {
					0 => bytes = [(seed >> 56) as u8; 32],
					1 => bytes[31] = (seed >> 40) as u8,
					_ => {
						bytes[30] = (seed >> 24) as u8;
						bytes[31] = (seed >> 16) as u8;
					},
				}
				RewardAddress(bytes)
			})
			.collect();
		let locks = addresses
			.iter()
			.zip(0u16..)
			.map(|(address, index)| SnapshotPayee {
				payee: RewardPayee::Lock { utxo: UtxoId { transaction: [7; 32], index } },
				stake: 10,
				reward_address: Some(*address),
			})
			.collect();
		let pool = |reward_address, total_stake| SnapshotPool {
			terms: TERMS_1,
			reward_address,
			operator_stake: 100,
			total_stake,
		};
		record_pool(1, pool(address(0x00), 100 + 10 * 60), locks);
		record_pool(2, pool(RewardAddress([0xAB; 32]), 100), vec![]);
		set_budget(3);

		let mut block = 6;
		while Settlement::<Test>::get().is_some() {
			initialize(block);
			block += 1;
			assert!(block < 100);
		}
		assert_eq!(SettlementHalted::<Test>::get(), None);

		let calls = DISTRIBUTIONS.with_borrow(Vec::clone);
		assert!(calls.len() > 1);
		let paid: Vec<[u8; 32]> = calls.iter().flatten().map(|output| output.address.0).collect();
		let mut expected: Vec<[u8; 32]> = addresses.iter().map(|address| address.0).collect();
		expected.extend([[0x00; 32], [0xAB; 32]]);
		expected.sort();
		expected.dedup();
		assert_eq!(paid, expected);
	});
}

fn settle_until_done(mut block: u64) {
	while Settlement::<Test>::get().is_some() {
		initialize(block);
		assert_eq!(SettlementHalted::<Test>::get(), None, "block {block}");
		assert_eq!(BlockRewards::do_try_state(), Ok(()), "block {block}");
		block += 1;
		assert!(block < 100);
	}
}

#[test]
fn a_halt_in_the_middle_of_a_split_undoes_the_step_and_a_resume_credits_once() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let set_pool_1 = |lock_address| {
			SNAPSHOT.with_borrow_mut(|pools| {
				pools.get_mut(&(E5, operator(1))).unwrap().1 =
					vec![delegator(3, 250, Some(0x01)), lock(7, 250, lock_address)];
			})
		};
		// The operator and the delegator are credited before the lock fails.
		set_pool_1(None);
		let pool_1 = bucket(1);
		initialize(6);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(bucket(1), pool_1);
		assert_eq!(Payouts::<Test>::iter_prefix(E5).count(), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));

		set_pool_1(Some(0x01));
		assert_ok!(BlockRewards::clear_halt(RuntimeOrigin::root(), [0; 32]));
		settle_until_done(7);
		let (operator_1, shares) = split_pool(pool_1, TERMS_1, 500, &[250, 250]).unwrap();
		let outputs = distributed();
		assert!(outputs.contains(&(address(0x90), operator_1)));
		assert!(outputs.contains(&(address(0x01), shares[0] + shares[1])));
	});
}

#[test]
fn payees_that_change_between_summing_and_crediting_halt_and_keep_the_bucket() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		set_budget(1);
		let mut block = 6;
		while !matches!(
			Settlement::<Test>::get().map(|cursor| cursor.entry),
			Some(SettlementEntry::Split { stage: SplitStage::Crediting { .. }, .. })
		) {
			initialize(block);
			block += 1;
			assert!(block < 40);
		}
		let pool = Settlement::<Test>::get().unwrap();
		let SettlementEntry::Split { operator: crediting, .. } = pool.entry else { unreachable!() };
		let before = Buckets::<Test>::get(E5, crediting);
		let original = SNAPSHOT.with_borrow(|pools| pools[&(E5, crediting)].1.clone());
		SNAPSHOT.with_borrow_mut(|pools| {
			pools.get_mut(&(E5, crediting)).unwrap().1.push(lock(9, 100, Some(0x09)));
		});
		while SettlementHalted::<Test>::get().is_none() {
			initialize(block);
			block += 1;
			assert!(block < 80);
		}
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(Buckets::<Test>::get(E5, crediting), before);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));

		// Earlier pages were credited in earlier blocks, so the mismatch stays until the state is
		// repaired: a cleared halt halts again and changes nothing.
		SNAPSHOT.with_borrow_mut(|pools| pools.get_mut(&(E5, crediting)).unwrap().1 = original);
		let payouts: Vec<_> = Payouts::<Test>::iter_prefix(E5).collect();
		assert_ok!(BlockRewards::clear_halt(RuntimeOrigin::root(), [0; 32]));
		initialize(block);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(Payouts::<Test>::iter_prefix(E5).collect::<Vec<_>>(), payouts);
		assert_eq!(Buckets::<Test>::get(E5, crediting), before);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn escrow_release_pages_through_more_owners_than_the_budget() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		for byte in 0x10..0x1A {
			lock_owner(E5, byte, Some(0x05));
		}
		lock_owner(E5, 4, Some(0x05));
		let want = expected();
		let to_05 = want[1].1 + want[4].1;
		set_budget(2);
		settle_until_done(6);
		assert!(distributed().contains(&(address(0x05), to_05)));
		assert_eq!(EscrowTotal::<Test>::get(), 0);
	});
}

#[test]
fn nonces_derive_from_the_epoch_and_the_address() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		initialize(6);
		let outputs = DISTRIBUTIONS.with_borrow(|calls| calls.concat());
		assert!(outputs.len() > 1);
		for output in &outputs {
			assert_eq!(output.nonce, (*b"block-rewards", E5, output.address).blake2_256());
			assert_ne!(
				output.nonce,
				(*b"block-rewards", CardanoEpoch(6), output.address).blake2_256()
			);
		}
		let mut nonces: Vec<_> = outputs.iter().map(|output| output.nonce).collect();
		nonces.sort();
		nonces.dedup();
		assert_eq!(nonces.len(), outputs.len());
	});
}

#[test]
fn a_total_stake_that_the_rows_do_not_make_up_halts() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		SNAPSHOT
			.with_borrow_mut(|pools| pools.get_mut(&(E5, operator(1))).unwrap().0.total_stake += 1);
		let pool_1 = bucket(1);
		initialize(6);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(bucket(1), pool_1);
		assert_eq!(Payouts::<Test>::iter_prefix(E5).count(), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn a_chunk_too_big_for_what_the_block_left_is_retried_at_half_the_size() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let outputs = expected_outputs();
		// The block leaves room for three of the five ledger entries: the whole chunk is undone,
		// leaving no payout behind, and a smaller one, whose budget the split uses up after the
		// Treasury payment, settles in the same block.
		ENTRY_COST.set(100);
		EXTRINSICS_USE.set(BLOCK_LIMIT - 300);
		initialize(6);
		assert_eq!(payments(), [treasury()]);
		assert!(distributed().is_empty());
		assert_eq!(BLOCK_USED.get(), BLOCK_LIMIT - 200);
		assert_eq!(SettlementHalted::<Test>::get(), None);
		settle_until_done(7);
		assert_eq!(distributed(), outputs);
	});
}

#[test]
fn on_idle_stays_within_the_weight_it_is_given() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		System::set_block_number(6);
		BLOCK_USED.set(0);
		// Too little for the checks: nothing is read or settled.
		assert_eq!(BlockRewards::on_idle(6, Weight::from_parts(1, 0)), Weight::zero());
		assert!(payments().is_empty());
		// Room for one update: the Treasury payment.
		let one = <<Test as frame_system::Config>::DbWeight as Get<RuntimeDbWeight>>::get()
			.reads(1)
			.saturating_add(<() as crate::weights::WeightInfo>::settle(1))
			.saturating_add(MockLedger::treasury_payment_weight())
			.saturating_add(MockLedger::distribution_weight(1));
		let used = BlockRewards::on_idle(6, one);
		assert!(one.all_gte(used));
		assert_eq!(payments(), [treasury()]);
		assert!(distributed().is_empty());
	});
}

#[test]
fn a_long_payout_starts_each_block_from_twice_the_chunk_tried_last() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let locks = (0..60u16)
			.map(|index| SnapshotPayee {
				payee: RewardPayee::Lock { utxo: UtxoId { transaction: [7; 32], index } },
				stake: 10,
				reward_address: Some(RewardAddress([index as u8 + 1; 32])),
			})
			.collect();
		let pool = SnapshotPool {
			terms: TERMS_1,
			reward_address: address(0x90),
			operator_stake: 400,
			total_stake: 1_000,
		};
		record_pool(1, pool, locks);
		// Five ledger entries fit in a block, a twentieth of the budget.
		ENTRY_COST.set(100);
		EXTRINSICS_USE.set(BLOCK_LIMIT - 500);
		let mut refused = Vec::new();
		let mut block = 6;
		while Settlement::<Test>::get().is_some() {
			REFUSED.set(0);
			initialize(block);
			assert_eq!(SettlementHalted::<Test>::get(), None);
			refused.push(REFUSED.get());
			block += 1;
			assert!(block < 100);
		}
		assert!(refused.len() > 10);
		assert!(refused[4..].iter().all(|refused| *refused <= 1), "{refused:?}");
	});
}

#[test]
fn an_unreadable_ledger_pool_leaves_settlement_waiting() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let outputs = expected_outputs();
		LEDGER_POOL_UNREADABLE.set(true);
		initialize(6);
		assert_eq!(Settled::<Test>::get(), 0);
		assert_eq!(SettlementHalted::<Test>::get(), None);
		LEDGER_POOL_UNREADABLE.set(false);
		settle_until_done(7);
		assert_eq!(distributed(), outputs);
	});
}

#[test]
fn full_blocks_leave_settlement_waiting() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		let outputs = expected_outputs();
		EXTRINSICS_USE.set(BLOCK_LIMIT);
		initialize(6);
		assert_eq!(Settled::<Test>::get(), 0);
		assert_eq!(SettlementHalted::<Test>::get(), None);
		EXTRINSICS_USE.set(0);
		settle_until_done(7);
		assert_eq!(distributed(), outputs);
	});
}

#[test]
fn what_settlement_used_is_not_counted_as_the_producers_utilization() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		// The Treasury payment and four outputs.
		ENTRY_COST.set(8);
		initialize(6);
		// The block ends 40 + 30 into the same dimension; the ledger's price update leaves out
		// what settlement used.
		UTILIZATION.set(Utilization { used: 70, max: BLOCK_LIMIT.into() });
		produced_by(6, Some(Producer::Operator(operator(1))));
		assert_eq!(SettlementUsage::<Test>::get(), Some(usage_of_block(40)));
		BlockRewards::note_block_usage(usage_of(UTILIZATION.get()));
		BlockRewards::on_finalize(6);
		let record = records().pop().unwrap();
		assert_eq!(record.utilization, Utilization { used: 30, max: BLOCK_LIMIT.into() });
		assert_eq!(SettlementUsage::<Test>::get(), None);
	});
}

#[test]
fn a_pool_that_falls_short_halts_even_with_nothing_queued() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		settle_until_done(6);
		RELEASED.set(Settled::<Test>::get() + 1);
		LEDGER_POOL.set(0);
		initialize(40);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::LedgerPoolBelowBaseline));
	});
}

#[test]
fn a_snapshot_replaced_after_the_epoch_closed_halts_the_split() {
	epoch_5_closed().execute_with(|| {
		record_both_pools();
		SNAPSHOT_REVISION.set(1);
		initialize(6);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));
		assert_eq!(Payouts::<Test>::iter_prefix(E5).count(), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

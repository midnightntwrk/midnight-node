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
use midnight_primitives_block_production::Producer;
use midnight_primitives_block_rewards::{CardanoEpoch, Utilization};

/// Activated at block 1; epoch 5 (blocks 2 and 3) closes in block 4, which opens epoch 6.
/// `L` covers everything drawn and the ledger pool holds it.
fn epoch_5_closed() -> sp_io::TestExternalities {
	let mut ext = installed();
	ext.execute_with(|| {
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		finalize(3);
		produced_by(6, Some(Producer::Operator(operator(1))));
		finalize(4);
		// Settlement finds no snapshot record unless a test records one.
		EVERY_OPERATOR_A_POOL.set(false);
		RELEASED.set(Drawn::<Test>::get());
		LEDGER_POOL.set(Drawn::<Test>::get());
		System::reset_events();
	});
	ext
}

fn treasury_of(epoch: u64) -> u128 {
	ClosedEpochs::<Test>::get(CardanoEpoch(epoch as u32)).unwrap().treasury
}

fn head() -> Option<SettlementCursor> {
	Settlement::<Test>::get()
}

fn split_entry(epoch: u32, byte: u8) -> Option<SettlementCursor> {
	Some(SettlementCursor {
		cardano_epoch: CardanoEpoch(epoch),
		entry: SettlementEntry::Split {
			operator: operator(byte),
			stage: SplitStage::Summing { after: None, shares: 0, stake: 0 },
		},
	})
}

fn treasury_entry(epoch: u32) -> Option<SettlementCursor> {
	Some(SettlementCursor { cardano_epoch: CardanoEpoch(epoch), entry: SettlementEntry::Treasury })
}

/// Also closes epoch 6 (block 5) and opens epoch 7 (block 6), with `L` covering both.
fn epochs_5_and_6_closed() -> sp_io::TestExternalities {
	let mut ext = epoch_5_closed();
	ext.execute_with(|| {
		finalize(5);
		produced_by(7, Some(Producer::Operator(operator(1))));
		// Epoch 6 accrued under a snapshot, so it closes with one.
		record_sole_pool(6, 1);
		finalize(6);
		RELEASED.set(Drawn::<Test>::get());
		LEDGER_POOL.set(Drawn::<Test>::get());
		System::reset_events();
	});
	ext
}

#[test]
fn a_closed_epoch_joins_the_settlement_queue_with_its_treasury_first() {
	epoch_5_closed().execute_with(|| {
		assert!(treasury_of(5) > 0);
		assert_eq!(head(), treasury_entry(5));
	});
}

#[test]
fn pays_an_epochs_treasury_once_covered() {
	epoch_5_closed().execute_with(|| {
		let amount = treasury_of(5);
		initialize(5);

		assert_eq!(payments(), [amount]);
		assert_eq!(Settled::<Test>::get(), amount);
		assert_eq!(head(), split_entry(5, 1));
		assert!(System::events().iter().any(|record| record.event
			== RuntimeEvent::BlockRewards(Event::TreasuryPaid {
				cardano_epoch: CardanoEpoch(5),
				amount
			})));
		// The test drops the snapshot record, so the pool split halts after the payment.
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SnapshotInconsistent));

		initialize(6);
		assert_eq!(payments().len(), 1);
	});
}

#[test]
fn an_entry_waits_whole_until_the_release_covers_it() {
	epoch_5_closed().execute_with(|| {
		let amount = treasury_of(5);
		RELEASED.set(amount - 1);
		initialize(5);
		assert!(payments().is_empty());
		assert_eq!(head(), treasury_entry(5));
		assert_eq!(SettlementHalted::<Test>::get(), None);

		RELEASED.set(amount);
		initialize(6);
		assert_eq!(payments(), [amount]);
		assert_eq!(head(), split_entry(5, 1));
	});
}

#[test]
fn epochs_settle_in_the_order_they_closed_one_epoch_per_block() {
	epochs_5_and_6_closed().execute_with(|| {
		record_sole_pool(5, 1);
		record_sole_pool(6, 1);
		let (treasury_5, treasury_6) = (treasury_of(5), treasury_of(6));
		assert_eq!(head(), treasury_entry(5));
		initialize(7);
		assert_eq!(payments(), [treasury_5]);
		assert_eq!(head(), treasury_entry(6));
		// Drawn but not settled: buckets, epoch 6's queued Treasury amount and the open epoch.
		let buckets: u128 = Buckets::<Test>::iter_values().sum();
		let open = OpenEpoch::<Test>::get().unwrap().treasury;
		assert_eq!(Drawn::<Test>::get() - Settled::<Test>::get(), buckets + treasury_6 + open);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
		initialize(8);
		assert_eq!(payments(), [treasury_5, treasury_6]);
		assert_eq!(head(), None);
		assert_eq!(ClosedEpochs::<Test>::iter().count(), 0);
		assert_eq!(NextClosed::<Test>::iter().count(), 0);
		assert_eq!(LastClosed::<Test>::get(), None);
		let open = OpenEpoch::<Test>::get().unwrap().treasury;
		let epoch_7_bucket: u128 = Buckets::<Test>::iter_values().sum();
		assert_eq!(Settled::<Test>::get(), Drawn::<Test>::get() - open - epoch_7_bucket);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn a_later_epoch_waits_behind_an_earlier_one() {
	installed().execute_with(|| {
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		// Full blocks leave epoch 6 no Treasury amount; its entry still waits behind epoch 5.
		UTILIZATION.set(Utilization { used: 1, max: 1 });
		produced_by(6, Some(Producer::Operator(operator(1))));
		finalize(3);
		produced_by(7, Some(Producer::Operator(operator(1))));
		finalize(4);
		EVERY_OPERATOR_A_POOL.set(false);
		LEDGER_POOL.set(Drawn::<Test>::get());
		assert!(treasury_of(5) > 0);
		assert_eq!(treasury_of(6), 0);

		RELEASED.set(treasury_of(5) - 1);
		initialize(7);
		initialize(8);
		assert!(payments().is_empty());
		assert_eq!(head(), treasury_entry(5));

		RELEASED.set(treasury_of(5));
		initialize(9);
		assert_eq!(payments(), [treasury_of(5)]);
		assert_eq!(head(), split_entry(5, 1));
	});
}

#[test]
fn an_epoch_without_treasury_is_passed_without_a_payment() {
	installed().execute_with(|| {
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		produced_by(5, Some(Producer::Permissioned));
		finalize(2);
		produced_by(6, Some(Producer::Permissioned));
		finalize(3);
		RELEASED.set(0);
		assert_eq!(head(), treasury_entry(5));
		initialize(4);
		assert!(payments().is_empty());
		assert_eq!(head(), None);
	});
}

#[test]
fn settled_above_released_halts() {
	epoch_5_closed().execute_with(|| {
		Settled::<Test>::put(1);
		RELEASED.set(0);
		initialize(5);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::SettledAboveReleased));
		assert!(payments().is_empty());
	});
}

#[test]
fn a_ledger_pool_below_the_baseline_halts() {
	epoch_5_closed().execute_with(|| {
		LEDGER_POOL.set(LEDGER_POOL.get() - 1);
		initialize(5);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::LedgerPoolBelowBaseline));
		assert!(payments().is_empty());
	});
}

#[test]
fn a_rejected_payment_halts_and_changes_nothing() {
	epoch_5_closed().execute_with(|| {
		PAYOUT_FAILURE.set(Some(PayoutFailure::Rejected));
		initialize(5);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::TreasuryPaymentFailed));
		assert_eq!(Settled::<Test>::get(), 0);
		assert_eq!(head(), treasury_entry(5));
	});
}

#[test]
fn a_full_block_retries_later_without_halting() {
	epoch_5_closed().execute_with(|| {
		PAYOUT_FAILURE.set(Some(PayoutFailure::BlockFull));
		initialize(5);
		assert_eq!(SettlementHalted::<Test>::get(), None);
		assert_eq!(Settled::<Test>::get(), 0);
		assert_eq!(head(), treasury_entry(5));
		PAYOUT_FAILURE.set(None);
		initialize(6);
		assert_eq!(payments(), [treasury_of(5)]);
	});
}

#[test]
fn a_halt_holds_payments_until_cleared() {
	epoch_5_closed().execute_with(|| {
		BlockRewards::halt(HaltReason::LedgerPoolBelowBaseline);
		initialize(5);
		assert!(payments().is_empty());
		assert_ok!(BlockRewards::clear_halt(RuntimeOrigin::root(), [1; 32]));
		initialize(6);
		assert_eq!(payments(), [treasury_of(5)]);
	});
}

/// Epoch 5's Treasury amount paid; its pool split halted for want of a snapshot record.
fn treasury_5_paid() -> sp_io::TestExternalities {
	let mut ext = epoch_5_closed();
	ext.execute_with(|| initialize(5));
	ext
}

#[test]
fn try_state_catches_each_broken_identity() {
	use super::settlement::epoch_5_settled;
	type Case = (&'static str, fn() -> sp_io::TestExternalities, fn(), Result<(), &'static str>);
	let cases: [Case; 7] = [
		(
			"reserve",
			treasury_5_paid,
			|| ReserveOutstanding::<Test>::mutate(|r| *r += 1),
			Err("B_o + D (reserve_outstanding + drawn) differs from the reserve at activation"),
		),
		(
			"settled above drawn",
			treasury_5_paid,
			|| Settled::<Test>::put(Drawn::<Test>::get() + 1),
			Err("S (settled) exceeds D (drawn)"),
		),
		(
			"settled above released",
			treasury_5_paid,
			|| RELEASED.set(Settled::<Test>::get() - 1),
			Err("S (settled) exceeds L (released)"),
		),
		(
			"unsettled",
			treasury_5_paid,
			|| Buckets::<Test>::mutate(CardanoEpoch(5), operator(1), |bucket| *bucket += 1),
			Err("D − S (drawn − settled) differs from buckets, payouts and Treasury amounts"),
		),
		(
			"escrow total",
			epoch_5_settled,
			|| EscrowTotal::<Test>::mutate(|total| *total += 1),
			Err("escrow balances and releases differ from EscrowTotal"),
		),
		(
			"ledger pool",
			epoch_5_settled,
			|| LEDGER_POOL.set(LEDGER_POOL.get() - 1),
			Err("the ledger pool is below baseline + escrow + (L − S)"),
		),
		(
			"ledger pool under its halt",
			epoch_5_settled,
			|| {
				LEDGER_POOL.set(LEDGER_POOL.get() - 1);
				SettlementHalted::<Test>::put(HaltReason::LedgerPoolBelowBaseline);
			},
			Ok(()),
		),
	];
	for (identity, fixture, corrupt, result) in cases {
		fixture().execute_with(|| {
			assert_eq!(BlockRewards::do_try_state(), Ok(()), "{identity}");
			corrupt();
			assert_eq!(BlockRewards::do_try_state(), result, "{identity}");
		});
	}
}

#[test]
fn nothing_is_paid_before_activation() {
	installed().execute_with(|| {
		initialize(2);
		assert!(payments().is_empty());
	});
}

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
use frame_support::traits::Hooks;
use midnight_primitives_block_production::{EpochLabel, Producer};
use midnight_primitives_block_rewards::{BlockDraw, CardanoEpoch, Utilization, block_draw};

const RESERVE: u128 = 6_000_000_000_873_988;

/// Activated at block 1; blocks from 2 on accrue.
fn activated() -> sp_io::TestExternalities {
	let mut ext = installed();
	ext.execute_with(|| {
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		System::reset_events();
	});
	ext
}

fn expected_draw(reserve: u128, utilization: Utilization) -> BlockDraw {
	let parameters = parameters(1);
	block_draw(reserve, parameters.distribution_rate, parameters.fixed_subsidy, utilization)
		.unwrap()
}

#[test]
fn nothing_accrues_before_the_block_after_activation() {
	installed().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(1);
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		finalize(1);
		assert!(records().is_empty());
		assert_eq!(OpenEpoch::<Test>::get(), None);
		finalize(2);
		assert_eq!(records().len(), 1);
	});
}

#[test]
fn nothing_accrues_before_the_reserve_anchor() {
	installed().execute_with(|| {
		RESERVE_ANCHOR_MS.set(Some(60_000));
		produced_by(5, Some(Producer::Operator(operator(1))));
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		assert_eq!(Activation::<Test>::get().unwrap().anchor_ms, 60_000);

		NOW_MS.set(59_999);
		finalize(2);
		assert!(records().is_empty());
		assert_eq!(Drawn::<Test>::get(), 0);

		NOW_MS.set(60_000);
		finalize(3);
		assert_eq!(records().len(), 1);
		assert!(Drawn::<Test>::get() > 0);
	});
}

#[test]
fn an_operator_block_fills_its_bucket_and_the_epoch_treasury() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		let utilization = Utilization { used: 1, max: 2 };
		UTILIZATION.set(utilization);
		finalize(2);

		let draw = BlockDraw { base: 95_890_410, producer: 93_493_149, treasury: 2_397_261 };
		assert_eq!(Buckets::<Test>::get(CardanoEpoch(5), operator(1)), draw.producer);
		assert_eq!(ReserveOutstanding::<Test>::get(), RESERVE - draw.base);
		assert_eq!(Drawn::<Test>::get(), draw.base);
		assert_eq!(
			OpenEpoch::<Test>::get(),
			Some(OpenEpochState {
				cardano_epoch: CardanoEpoch(5),
				parameters_version: 1,
				treasury: draw.treasury,
				blocks: 1,
			})
		);
		assert_eq!(
			records(),
			[BlockRewardRecord {
				midnight_epoch: EpochLabel(5),
				cardano_epoch: CardanoEpoch(5),
				operator: Some(operator(1)),
				parameters_version: 1,
				utilization,
				draw,
			}]
		);
	});
}

#[test]
fn a_permissioned_block_draws_nothing_but_leaves_a_record() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Permissioned));
		finalize(2);
		assert_eq!(ReserveOutstanding::<Test>::get(), RESERVE);
		assert_eq!(Drawn::<Test>::get(), 0);
		assert_eq!(OpenEpoch::<Test>::get().map(|epoch| epoch.blocks), Some(1));
		let [record] = records().try_into().unwrap();
		assert_eq!((record.operator, record.draw), (None, BlockDraw::default()));
	});
}

#[test]
fn each_block_draws_from_what_the_previous_one_left() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		let full = Utilization { used: 1, max: 1 };
		UTILIZATION.set(full);
		finalize(2);
		finalize(3);
		let first = expected_draw(RESERVE, full);
		let second = expected_draw(RESERVE - first.base, full);
		assert!(second.base < first.base);
		assert_eq!(ReserveOutstanding::<Test>::get(), RESERVE - first.base - second.base);
		assert_eq!(
			Buckets::<Test>::get(CardanoEpoch(5), operator(1)),
			first.producer + second.producer
		);
	});
}

#[test]
fn a_new_cardano_epoch_closes_the_open_epoch() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		finalize(3);
		let treasury = OpenEpoch::<Test>::get().unwrap().treasury;
		produced_by(6, Some(Producer::Operator(operator(2))));
		finalize(4);

		let closed =
			ClosedEpoch { parameters_version: 1, blocks: 2, treasury, snapshot_id: Some([5; 32]) };
		assert_eq!(ClosedEpochs::<Test>::get(CardanoEpoch(5)), Some(closed));
		assert!(System::events().iter().any(|record| record.event
			== RuntimeEvent::BlockRewards(Event::EpochClosed {
				cardano_epoch: CardanoEpoch(5),
				epoch: closed
			})));
		let open = OpenEpoch::<Test>::get().unwrap();
		assert_eq!((open.cardano_epoch, open.blocks), (CardanoEpoch(6), 1));
		assert_eq!(Buckets::<Test>::get(CardanoEpoch(6), operator(1)), 0);
		assert!(Buckets::<Test>::get(CardanoEpoch(6), operator(2)) > 0);
	});
}

#[test]
fn midnight_epochs_of_one_cardano_epoch_accrue_together() {
	activated().execute_with(|| {
		produced_in(5, 100, Some(Producer::Operator(operator(1))));
		finalize(2);
		produced_in(6, 100, Some(Producer::Operator(operator(1))));
		finalize(3);

		assert!(ClosedEpochs::<Test>::iter().next().is_none());
		let open = OpenEpoch::<Test>::get().unwrap();
		assert_eq!((open.cardano_epoch, open.blocks), (CardanoEpoch(100), 2));
		let epochs: Vec<_> =
			records().iter().map(|r| (r.midnight_epoch, r.cardano_epoch)).collect();
		assert_eq!(
			epochs,
			[(EpochLabel(5), CardanoEpoch(100)), (EpochLabel(6), CardanoEpoch(100))]
		);
		let [first, second] = records().try_into().unwrap();
		assert_eq!(
			Buckets::<Test>::get(CardanoEpoch(100), operator(1)),
			first.draw.producer + second.draw.producer
		);
	});
}

#[test]
fn an_epoch_keeps_the_parameter_version_it_opened_with() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		let next = RewardParametersV1 { publication_hash: [1; 32], ..parameters(2) };
		assert_ok!(BlockRewards::install_parameters(next));
		finalize(3);
		produced_by(6, Some(Producer::Operator(operator(1))));
		finalize(4);
		let versions: Vec<_> = records().iter().map(|r| r.parameters_version).collect();
		assert_eq!(versions, [1, 1, 2]);
	});
}

#[test]
fn every_drawn_star_is_in_a_bucket_or_a_treasury_amount() {
	activated().execute_with(|| {
		let producers = [
			Some(Producer::Operator(operator(1))),
			Some(Producer::Permissioned),
			Some(Producer::Operator(operator(2))),
			None,
		];
		for block in 2..60u64 {
			produced_by(block / 10, producers[block as usize % producers.len()]);
			UTILIZATION.set(Utilization { used: (block % 7).into(), max: 6 + 1 });
			finalize(block);
			assert_eq!(BlockRewards::do_try_state(), Ok(()), "block {block}");
		}
		let buckets: u128 = Buckets::<Test>::iter_values().sum();
		let closed: u128 = ClosedEpochs::<Test>::iter_values().map(|epoch| epoch.treasury).sum();
		let open = OpenEpoch::<Test>::get().unwrap().treasury;
		assert_eq!(Drawn::<Test>::get(), buckets + closed + open);
		assert_eq!(ReserveOutstanding::<Test>::get() + Drawn::<Test>::get(), RESERVE);
	});
}

#[test]
// Panics only where debug assertions are on; release builds draw nothing.
#[cfg_attr(debug_assertions, should_panic(expected = "Defensive failure"))]
fn a_block_without_utilization_is_a_defensive_failure_and_draws_nothing() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		System::set_block_number(2);
		BlockRewards::on_finalize(2);
		assert_eq!(ReserveOutstanding::<Test>::get(), RESERVE);
		assert_eq!(records()[0].draw, BlockDraw::default());
	});
}

#[test]
fn usage_above_the_limit_counts_as_a_full_block() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		UTILIZATION.set(Utilization { used: 2, max: 1 });
		finalize(2);
		assert_eq!(records()[0].utilization, Utilization { used: 1, max: 1 });
		assert!(ReserveOutstanding::<Test>::get() < RESERVE);
	});
}

#[test]
fn a_block_of_an_operator_missing_from_the_snapshot_draws_nothing() {
	activated().execute_with(|| {
		EVERY_OPERATOR_A_POOL.set(false);
		SNAPSHOT.with_borrow_mut(|pools| {
			pools.insert((CardanoEpoch(5), operator(2)), (sole_pool(), Vec::new()));
		});
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		assert_eq!(records()[0].operator, Some(operator(1)));
		assert_eq!(records()[0].draw, BlockDraw::default());
		assert_eq!(ReserveOutstanding::<Test>::get(), RESERVE);
		assert_eq!(Buckets::<Test>::iter().count(), 0);

		produced_by(5, Some(Producer::Operator(operator(2))));
		finalize(3);
		assert!(records()[1].draw.base > 0);
	});
}

#[test]
fn a_block_of_a_pool_without_stake_draws_nothing() {
	activated().execute_with(|| {
		EVERY_OPERATOR_A_POOL.set(false);
		let unstaked = SnapshotPool { operator_stake: 0, total_stake: 0, ..sole_pool() };
		SNAPSHOT.with_borrow_mut(|pools| {
			pools.insert((CardanoEpoch(5), operator(1)), (unstaked, Vec::new()));
		});
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		assert_eq!(records()[0].draw, BlockDraw::default());
		assert_eq!(Buckets::<Test>::iter().count(), 0);
	});
}

#[test]
fn a_draw_without_a_producer_share_leaves_no_bucket() {
	activated().execute_with(|| {
		ParameterSets::<Test>::mutate(1, |parameters| {
			let Some(RewardParameters::V1(parameters)) = parameters else { unreachable!() };
			parameters.fixed_subsidy = Ratio::new(0, 1);
		});
		UTILIZATION.set(Utilization { used: 0, max: 1 });
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		let draw = records()[0].draw;
		assert_eq!(draw.producer, 0);
		assert!(draw.treasury > 0);
		assert_eq!(Buckets::<Test>::iter().count(), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
fn a_block_without_a_complete_snapshot_draws_nothing() {
	activated().execute_with(|| {
		EVERY_OPERATOR_A_POOL.set(false);
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		assert_eq!(records()[0].draw, BlockDraw::default());
		assert_eq!(Drawn::<Test>::get(), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

#[test]
// Panics only where debug assertions are on; release builds stay in the open epoch.
#[cfg_attr(debug_assertions, should_panic(expected = "Defensive failure"))]
fn a_cardano_epoch_that_goes_back_stays_in_the_open_epoch() {
	activated().execute_with(|| {
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(2);
		produced_by(6, Some(Producer::Operator(operator(1))));
		finalize(3);
		let closed_5 = ClosedEpochs::<Test>::get(CardanoEpoch(5));
		produced_by(5, Some(Producer::Operator(operator(1))));
		finalize(4);
		assert_eq!(OpenEpoch::<Test>::get().unwrap().cardano_epoch, CardanoEpoch(6));
		assert_eq!(ClosedEpochs::<Test>::get(CardanoEpoch(5)), closed_5);
		assert_eq!(NextClosed::<Test>::iter().count(), 0);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));
	});
}

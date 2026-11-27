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

#[test]
fn records_the_baseline_without_what_is_already_released_and_sets_the_counters() {
	installed().execute_with(|| {
		LEDGER_POOL.set(1_234);
		RELEASED.set(300);
		System::set_block_number(7);
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));

		let record =
			ActivationRecord { at: 7, anchor_ms: 0, reserve: 6_000_000_000_873_988, baseline: 934 };
		assert_eq!(Activation::<Test>::get(), Some(record));
		assert_eq!(ReserveOutstanding::<Test>::get(), 6_000_000_000_873_988);
		assert_eq!((Drawn::<Test>::get(), Settled::<Test>::get()), (0, 0));
		assert_eq!(
			last_event(),
			Event::Activated {
				parameters_version: 1,
				anchor_ms: 0,
				reserve: 6_000_000_000_873_988,
				baseline: 934
			}
		);
		assert_eq!(BlockRewards::do_try_state(), Ok(()));

		// The runtime API reports the same counters in one read.
		let state = BlockRewards::state();
		assert_eq!(state.activation, Some(record));
		assert_eq!(state.parameters_version, Some(1));
		assert_eq!(state.reserve_outstanding, 6_000_000_000_873_988);
		assert_eq!((state.drawn, state.settled, state.released), (0, 0, 300));
		assert_eq!((state.halted, state.open_epoch, state.escrow_total), (None, None, 0));
	});
}

#[test]
fn happens_once() {
	installed().execute_with(|| {
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		assert_noop!(
			BlockRewards::activate(RuntimeOrigin::root()),
			Error::<Test>::AlreadyActivated
		);
	});
}

type Precondition =
	(&'static str, fn() -> sp_io::TestExternalities, fn() -> RuntimeOrigin, DispatchError);

#[test]
fn each_precondition_has_its_own_error() {
	let cases: [Precondition; 7] = [
		("governance", installed, || RuntimeOrigin::signed(1), DispatchError::BadOrigin),
		("parameters", new_test_ext, RuntimeOrigin::root, Error::<Test>::NoParameters.into()),
		(
			"the reserve observation",
			installed,
			|| {
				RESERVE_ANCHOR_MS.set(None);
				RuntimeOrigin::root()
			},
			Error::<Test>::ReserveObservationNotLive.into(),
		),
		(
			"a source of the Cardano epoch",
			installed,
			|| {
				CARDANO_EPOCH_SOURCE_LIVE.set(false);
				RuntimeOrigin::root()
			},
			Error::<Test>::CardanoEpochSourceNotLive.into(),
		),
		(
			"a ledger pool that holds the release",
			installed,
			|| {
				LEDGER_POOL.set(299);
				RELEASED.set(300);
				RuntimeOrigin::root()
			},
			Error::<Test>::LedgerPoolBelowReleased.into(),
		),
		(
			"a readable ledger pool",
			installed,
			|| {
				LEDGER_POOL.set(299);
				RELEASED.set(300);
				LEDGER_POOL_UNREADABLE.set(true);
				RuntimeOrigin::root()
			},
			Error::<Test>::LedgerPoolUnreadable.into(),
		),
		(
			"a ledger reserve that is the initial reserve less the release",
			installed,
			|| {
				LEDGER_POOL.set(300);
				RELEASED.set(300);
				RESERVE_POOL.set(Some(6_000_000_000_873_988 - 299));
				RuntimeOrigin::root()
			},
			Error::<Test>::ReservePoolMismatch.into(),
		),
	];
	for (requires, ext, origin, error) in cases {
		ext().execute_with(|| {
			let origin = origin();
			let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
			assert_eq!(BlockRewards::activate(origin), Err(error), "requires {requires}");
			let after = sp_io::storage::root(sp_runtime::StateVersion::V1);
			assert_eq!(after, before, "requires {requires}");
		});
	}
}

#[test]
#[should_panic(expected = "B_o + D")]
fn a_block_that_breaks_the_counters_is_invalid() {
	installed().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(BlockRewards::activate(RuntimeOrigin::root()));
		Drawn::<Test>::put(1);
		finalize(2);
	});
}

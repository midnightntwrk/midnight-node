// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// Copyright (C) Parity Technologies (UK) Ltd. (tests adapted from `pallet-utility`)
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

//! Tests for the Batch Pallet.

#![cfg(test)]

use super::{Call, Error, Event};
use crate::{
	WeightInfo,
	mock::{
		Balances, Batch, RuntimeCall, RuntimeOrigin, System, Test, TestBaseCallFilter,
		example::Call as ExampleCall, new_test_ext,
	},
};
use frame_support::{
	assert_err_ignore_postinfo, assert_noop, assert_ok,
	dispatch::{
		DispatchErrorWithPostInfo, GetDispatchInfo, Pays, PostDispatchInfo, extract_actual_weight,
	},
	storage,
	traits::Contains,
	weights::Weight,
};
use frame_system::Call as SystemCall;
use pallet_balances::Call as BalancesCall;
use sp_runtime::{
	DispatchError, TokenError,
	traits::{BadOrigin, Dispatchable},
};

type BatchCall = Call<Test>;

fn call_transfer(dest: u64, value: u64) -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value })
}

fn call_foobar(err: bool, start_weight: Weight, end_weight: Option<Weight>) -> RuntimeCall {
	RuntimeCall::Example(ExampleCall::foobar { err, start_weight, end_weight })
}

#[test]
fn batch_with_root_works() {
	new_test_ext().execute_with(|| {
		let k = b"a".to_vec();
		let call =
			RuntimeCall::System(SystemCall::set_storage { items: vec![(k.clone(), k.clone())] });
		// The base call filter excludes `System::set_storage`; root must still bypass it.
		assert!(!TestBaseCallFilter::contains(&call));
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		assert_ok!(Batch::batch(
			RuntimeOrigin::root(),
			vec![
				RuntimeCall::Balances(BalancesCall::force_transfer {
					source: 1,
					dest: 2,
					value: 5,
				}),
				RuntimeCall::Balances(BalancesCall::force_transfer {
					source: 1,
					dest: 2,
					value: 5,
				}),
				call,
			]
		));
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(2), 20);
		assert_eq!(storage::unhashed::get_raw(&k), Some(k));
	});
}

#[test]
fn batch_with_signed_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		assert_ok!(Batch::batch(
			RuntimeOrigin::signed(1),
			vec![call_transfer(2, 5), call_transfer(2, 5)]
		));
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(2), 20);
	});
}

#[test]
fn batch_with_signed_filters() {
	new_test_ext().execute_with(|| {
		// `transfer_keep_alive` is rejected by the base call filter for non-root callers.
		assert_ok!(Batch::batch(
			RuntimeOrigin::signed(1),
			vec![RuntimeCall::Balances(BalancesCall::transfer_keep_alive { dest: 2, value: 1 })]
		));
		System::assert_last_event(
			Event::BatchInterrupted {
				index: 0,
				error: frame_system::Error::<Test>::CallFiltered.into(),
			}
			.into(),
		);
	});
}

#[test]
fn batch_early_exit_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		// Second call fails (insufficient balance); third is skipped, first stays.
		assert_ok!(Batch::batch(
			RuntimeOrigin::signed(1),
			vec![call_transfer(2, 5), call_transfer(2, 10), call_transfer(2, 5)]
		));
		assert_eq!(Balances::free_balance(1), 5);
		assert_eq!(Balances::free_balance(2), 15);
	});
}

#[test]
fn batch_handles_weight_refund() {
	new_test_ext().execute_with(|| {
		let start_weight = Weight::from_parts(100, 0);
		let end_weight = Weight::from_parts(75, 0);
		let diff = start_weight - end_weight;
		let batch_len = 4;

		// Full weight when ok
		let inner_call = call_foobar(false, start_weight, None);
		let batch_calls = vec![inner_call; batch_len as usize];
		let call = RuntimeCall::Batch(BatchCall::batch { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);

		// Refund weight when ok
		let inner_call = call_foobar(false, start_weight, Some(end_weight));
		let batch_calls = vec![inner_call; batch_len as usize];
		let call = RuntimeCall::Batch(BatchCall::batch { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight - diff * batch_len);

		// Full weight when err
		let good_call = call_foobar(false, start_weight, None);
		let bad_call = call_foobar(true, start_weight, None);
		let batch_calls = vec![good_call, bad_call];
		let call = RuntimeCall::Batch(BatchCall::batch { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_ok!(result);
		System::assert_last_event(
			Event::BatchInterrupted { index: 1, error: DispatchError::Other("") }.into(),
		);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);

		// Partial batch completion
		let good_call = call_foobar(false, start_weight, Some(end_weight));
		let bad_call = call_foobar(true, start_weight, Some(end_weight));
		let batch_calls = vec![good_call, bad_call.clone(), bad_call];
		let call = RuntimeCall::Batch(BatchCall::batch { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_ok!(result);
		System::assert_last_event(
			Event::BatchInterrupted { index: 1, error: DispatchError::Other("") }.into(),
		);
		assert_eq!(
			extract_actual_weight(&result, &info),
			<Test as crate::Config>::WeightInfo::batch(2) + end_weight * 2,
		);
	});
}

#[test]
fn batch_all_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		assert_ok!(Batch::batch_all(
			RuntimeOrigin::signed(1),
			vec![call_transfer(2, 5), call_transfer(2, 5)]
		));
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(2), 20);
	});
}

#[test]
fn batch_all_revert() {
	new_test_ext().execute_with(|| {
		let call = call_transfer(2, 5);
		let info = call.get_dispatch_info();

		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		let batch_all_calls = RuntimeCall::Batch(BatchCall::batch_all {
			calls: vec![call_transfer(2, 5), call_transfer(2, 10), call_transfer(2, 5)],
		});
		assert_noop!(
			batch_all_calls.dispatch(RuntimeOrigin::signed(1)),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(
						<Test as crate::Config>::WeightInfo::batch_all(2) + info.call_weight * 2
					),
					pays_fee: Pays::Yes,
				},
				error: TokenError::FundsUnavailable.into(),
			}
		);
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
	});
}

#[test]
fn batch_all_handles_weight_refund() {
	new_test_ext().execute_with(|| {
		let start_weight = Weight::from_parts(100, 0);
		let end_weight = Weight::from_parts(75, 0);
		let diff = start_weight - end_weight;
		let batch_len = 4;

		// Full weight when ok
		let inner_call = call_foobar(false, start_weight, None);
		let batch_calls = vec![inner_call; batch_len as usize];
		let call = RuntimeCall::Batch(BatchCall::batch_all { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);

		// Refund weight when ok
		let inner_call = call_foobar(false, start_weight, Some(end_weight));
		let batch_calls = vec![inner_call; batch_len as usize];
		let call = RuntimeCall::Batch(BatchCall::batch_all { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight - diff * batch_len);

		// Full weight when err
		let good_call = call_foobar(false, start_weight, None);
		let bad_call = call_foobar(true, start_weight, None);
		let batch_calls = vec![good_call, bad_call];
		let call = RuntimeCall::Batch(BatchCall::batch_all { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::signed(1));
		assert_err_ignore_postinfo!(result, "The cake is a lie.");
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);
	});
}

#[test]
fn batch_all_does_not_nest() {
	new_test_ext().execute_with(|| {
		let batch_all = RuntimeCall::Batch(BatchCall::batch_all {
			calls: vec![call_transfer(2, 1), call_transfer(2, 1), call_transfer(2, 1)],
		});

		let info = batch_all.get_dispatch_info();

		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		// A nested batch_all call fails the inner filter with `CallFiltered`.
		assert_noop!(
			Batch::batch_all(RuntimeOrigin::signed(1), vec![batch_all.clone()]),
			DispatchErrorWithPostInfo {
				post_info: PostDispatchInfo {
					actual_weight: Some(
						<Test as crate::Config>::WeightInfo::batch_all(1) + info.call_weight
					),
					pays_fee: Pays::Yes,
				},
				error: frame_system::Error::<Test>::CallFiltered.into(),
			}
		);

		// Filter also persists through `batch(batch_all(..))` from a signed origin.
		let batch_nested = RuntimeCall::Batch(BatchCall::batch { calls: vec![batch_all] });
		assert_ok!(Batch::batch_all(RuntimeOrigin::signed(1), vec![batch_nested]));
		System::assert_has_event(
			Event::BatchInterrupted {
				index: 0,
				error: frame_system::Error::<Test>::CallFiltered.into(),
			}
			.into(),
		);
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
	});
}

#[test]
fn batch_limit() {
	new_test_ext().execute_with(|| {
		let calls = vec![RuntimeCall::System(SystemCall::remark { remark: vec![] }); 40_000];
		assert_noop!(
			Batch::batch(RuntimeOrigin::signed(1), calls.clone()),
			Error::<Test>::TooManyCalls,
		);
		assert_noop!(
			Batch::batch_all(RuntimeOrigin::signed(1), calls),
			Error::<Test>::TooManyCalls,
		);
	});
}

#[test]
fn none_origin_does_not_work() {
	new_test_ext().execute_with(|| {
		assert_noop!(Batch::batch(RuntimeOrigin::none(), vec![]), BadOrigin);
		assert_noop!(Batch::batch_all(RuntimeOrigin::none(), vec![]), BadOrigin);
	});
}

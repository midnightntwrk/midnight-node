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

fn call_force_transfer(source: u64, dest: u64, value: u64) -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::force_transfer { source, dest, value })
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
			vec![call_force_transfer(1, 2, 5), call_force_transfer(1, 2, 5), call,]
		));
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(2), 20);
		assert_eq!(storage::unhashed::get_raw(&k), Some(k));
	});
}

#[test]
fn non_root_origin_rejected() {
	new_test_ext().execute_with(|| {
		assert_noop!(Batch::batch(RuntimeOrigin::signed(1), vec![]), BadOrigin);
		assert_noop!(Batch::batch(RuntimeOrigin::none(), vec![]), BadOrigin);
		assert_noop!(Batch::batch_all(RuntimeOrigin::signed(1), vec![]), BadOrigin);
		assert_noop!(Batch::batch_all(RuntimeOrigin::none(), vec![]), BadOrigin);
	});
}

#[test]
fn batch_early_exit_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		// Second call fails (insufficient balance); third is skipped, first stays.
		assert_ok!(Batch::batch(
			RuntimeOrigin::root(),
			vec![
				call_force_transfer(1, 2, 5),
				call_force_transfer(1, 2, 10),
				call_force_transfer(1, 2, 5),
			]
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
		let result = call.dispatch(RuntimeOrigin::root());
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);

		// Refund weight when ok
		let inner_call = call_foobar(false, start_weight, Some(end_weight));
		let batch_calls = vec![inner_call; batch_len as usize];
		let call = RuntimeCall::Batch(BatchCall::batch { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::root());
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight - diff * batch_len);

		// Full weight when err
		let good_call = call_foobar(false, start_weight, None);
		let bad_call = call_foobar(true, start_weight, None);
		let batch_calls = vec![good_call, bad_call];
		let call = RuntimeCall::Batch(BatchCall::batch { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::root());
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
		let result = call.dispatch(RuntimeOrigin::root());
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
			RuntimeOrigin::root(),
			vec![call_force_transfer(1, 2, 5), call_force_transfer(1, 2, 5)]
		));
		assert_eq!(Balances::free_balance(1), 0);
		assert_eq!(Balances::free_balance(2), 20);
	});
}

#[test]
fn batch_all_revert() {
	new_test_ext().execute_with(|| {
		let call = call_force_transfer(1, 2, 5);
		let info = call.get_dispatch_info();

		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::free_balance(2), 10);
		let batch_all_calls = RuntimeCall::Batch(BatchCall::batch_all {
			calls: vec![
				call_force_transfer(1, 2, 5),
				call_force_transfer(1, 2, 10),
				call_force_transfer(1, 2, 5),
			],
		});
		assert_noop!(
			batch_all_calls.dispatch(RuntimeOrigin::root()),
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
		let result = call.dispatch(RuntimeOrigin::root());
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);

		// Refund weight when ok
		let inner_call = call_foobar(false, start_weight, Some(end_weight));
		let batch_calls = vec![inner_call; batch_len as usize];
		let call = RuntimeCall::Batch(BatchCall::batch_all { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::root());
		assert_ok!(result);
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight - diff * batch_len);

		// Full weight when err
		let good_call = call_foobar(false, start_weight, None);
		let bad_call = call_foobar(true, start_weight, None);
		let batch_calls = vec![good_call, bad_call];
		let call = RuntimeCall::Batch(BatchCall::batch_all { calls: batch_calls });
		let info = call.get_dispatch_info();
		let result = call.dispatch(RuntimeOrigin::root());
		assert_err_ignore_postinfo!(result, "The cake is a lie.");
		assert_eq!(extract_actual_weight(&result, &info), info.call_weight);
	});
}

#[test]
fn batch_limit() {
	new_test_ext().execute_with(|| {
		let calls = vec![RuntimeCall::System(SystemCall::remark { remark: vec![] }); 40_000];
		assert_noop!(
			Batch::batch(RuntimeOrigin::root(), calls.clone()),
			Error::<Test>::TooManyCalls,
		);
		assert_noop!(Batch::batch_all(RuntimeOrigin::root(), calls), Error::<Test>::TooManyCalls,);
	});
}

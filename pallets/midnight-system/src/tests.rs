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

//! Tests for the pallet-midnight-system safe mode.

use crate::{
	EnterSafeModeOnFailedMigration, Event, SafeMode, SafeModeFilter,
	mock::{mandatory, *},
};
use frame_support::{
	assert_noop, assert_ok,
	migrations::{FailedMigrationHandler, FailedMigrationHandling},
	traits::Contains,
};
use sp_runtime::DispatchError;

/// A `Normal`-class, non-whitelisted call (stands in for a user transaction).
fn user_call() -> RuntimeCall {
	RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() })
}

/// A `Mandatory`-class call (stands in for an inherent).
fn inherent_call() -> RuntimeCall {
	RuntimeCall::Mandatory(mandatory::Call::noop {})
}

/// A whitelisted governance call (the mock whitelists `MidnightSystem`).
fn whitelisted_call() -> RuntimeCall {
	RuntimeCall::MidnightSystem(crate::Call::enter_safe_mode {})
}

#[test]
fn filter_allows_everything_when_not_in_safe_mode() {
	new_test_ext().execute_with(|| {
		assert!(!SafeMode::<Test>::get());
		assert!(SafeModeFilter::<Test>::contains(&user_call()));
		assert!(SafeModeFilter::<Test>::contains(&inherent_call()));
		assert!(SafeModeFilter::<Test>::contains(&whitelisted_call()));
	});
}

#[test]
fn filter_blocks_non_whitelisted_calls_in_safe_mode() {
	new_test_ext().execute_with(|| {
		SafeMode::<Test>::put(true);
		assert!(!SafeModeFilter::<Test>::contains(&user_call()));
	});
}

#[test]
fn filter_always_allows_mandatory_calls_even_in_safe_mode() {
	new_test_ext().execute_with(|| {
		SafeMode::<Test>::put(true);
		// Inherents must never be blocked, or block production stalls.
		assert!(SafeModeFilter::<Test>::contains(&inherent_call()));
	});
}

#[test]
fn filter_allows_whitelisted_governance_calls_in_safe_mode() {
	new_test_ext().execute_with(|| {
		SafeMode::<Test>::put(true);
		assert!(SafeModeFilter::<Test>::contains(&whitelisted_call()));
	});
}

#[test]
fn failed_migration_enters_safe_mode_and_force_unsticks() {
	new_test_ext().execute_with(|| {
		assert!(!SafeMode::<Test>::get());

		let handling = EnterSafeModeOnFailedMigration::<Test>::failed(Some(3));

		// It must NOT keep the chain stuck; it unsticks so extrinsics resume.
		assert_eq!(handling, FailedMigrationHandling::ForceUnstuck);
		assert!(SafeMode::<Test>::get());
		System::assert_last_event(RuntimeEvent::MidnightSystem(Event::SafeModeEntered {
			migration: Some(3),
		}));
	});
}

#[test]
fn exit_safe_mode_by_root_resumes_normal_operation() {
	new_test_ext().execute_with(|| {
		SafeMode::<Test>::put(true);

		assert_ok!(MidnightSystem::exit_safe_mode(RuntimeOrigin::root()));

		assert!(!SafeMode::<Test>::get());
		System::assert_last_event(RuntimeEvent::MidnightSystem(Event::SafeModeExited));
		// And the filter no longer blocks user calls.
		assert!(SafeModeFilter::<Test>::contains(&user_call()));
	});
}

#[test]
fn enter_safe_mode_by_root_locks_the_chain() {
	new_test_ext().execute_with(|| {
		assert_ok!(MidnightSystem::enter_safe_mode(RuntimeOrigin::root()));

		assert!(SafeMode::<Test>::get());
		System::assert_last_event(RuntimeEvent::MidnightSystem(Event::SafeModeEntered {
			migration: None,
		}));
	});
}

#[test]
fn enter_and_exit_safe_mode_require_root() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			MidnightSystem::enter_safe_mode(RuntimeOrigin::signed(1)),
			DispatchError::BadOrigin
		);
		assert_noop!(
			MidnightSystem::exit_safe_mode(RuntimeOrigin::signed(1)),
			DispatchError::BadOrigin
		);
	});
}

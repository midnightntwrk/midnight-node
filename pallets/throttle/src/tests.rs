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

//! Tests for throttle pallet
//!
//! These tests exercise both the `AccountUsage` storage directly and the
//! `CheckThrottle` TransactionExtension via its `validate()` and `prepare()` methods.

use crate::{AccountUsage, CheckThrottle, UsageStats, mock::*};
use frame_support::assert_ok;
use sp_runtime::{
	traits::{TransactionExtension, TxBaseImplication},
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
};

fn usage(bytes_used: u64, txs_used: u64, window_start: u64) -> UsageStats<Test> {
	UsageStats { bytes_used, txs_used, window_start }
}

/// Calls `CheckThrottle::validate()` for a signed transaction from `who` with the given `len`.
fn validate_signed(
	who: u64,
	len: usize,
) -> Result<
	(sp_runtime::transaction_validity::ValidTransaction, Option<u64>, RuntimeOrigin),
	TransactionValidityError,
> {
	let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
	let info = frame_support::dispatch::DispatchInfo::default();

	CheckThrottle::<Test>::new().validate(
		RuntimeOrigin::signed(who),
		&call,
		&info,
		len,
		(),
		&TxBaseImplication(&call),
		TransactionSource::External,
	)
}

/// Calls `CheckThrottle::validate()` for an unsigned/none origin.
fn validate_unsigned(
	len: usize,
) -> Result<
	(sp_runtime::transaction_validity::ValidTransaction, Option<u64>, RuntimeOrigin),
	TransactionValidityError,
> {
	let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
	let info = frame_support::dispatch::DispatchInfo::default();

	CheckThrottle::<Test>::new().validate(
		RuntimeOrigin::none(),
		&call,
		&info,
		len,
		(),
		&TxBaseImplication(&call),
		TransactionSource::External,
	)
}

/// Runs the full `validate()` → `prepare()` flow for a signed tx from `who`.
fn validate_and_prepare(who: u64, len: usize) {
	let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
	let info = frame_support::dispatch::DispatchInfo::default();

	let (_validity, val, origin) = CheckThrottle::<Test>::new()
		.validate(
			RuntimeOrigin::signed(who),
			&call,
			&info,
			len,
			(),
			&TxBaseImplication(&call),
			TransactionSource::External,
		)
		.expect("validate should succeed");

	assert_ok!(CheckThrottle::<Test>::new().prepare(val, &origin, &call, &info, len));
}

// ---------------------------------------------------------------------------
// Storage tests
// ---------------------------------------------------------------------------

#[test]
fn account_usage_defaults_to_zero() {
	new_test_ext().execute_with(|| {
		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 0);
		assert_eq!(u.txs_used, 0);
		assert_eq!(u.window_start, 0);
	});
}

#[test]
fn account_usage_can_be_set_and_read() {
	new_test_ext().execute_with(|| {
		AccountUsage::<Test>::insert(1u64, usage(500, 2, 10));
		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 500);
		assert_eq!(u.txs_used, 2);
		assert_eq!(u.window_start, 10);
	});
}

#[test]
fn account_usage_is_independent_per_account() {
	new_test_ext().execute_with(|| {
		AccountUsage::<Test>::insert(1u64, usage(100, 1, 5));
		AccountUsage::<Test>::insert(2u64, usage(200, 3, 10));

		assert_eq!(AccountUsage::<Test>::get(1u64), usage(100, 1, 5));
		assert_eq!(AccountUsage::<Test>::get(2u64), usage(200, 3, 10));
		assert_eq!(AccountUsage::<Test>::get(3u64), usage(0, 0, 0));
	});
}

// ---------------------------------------------------------------------------
// TransactionExtension::validate() tests
// ---------------------------------------------------------------------------

#[test]
fn validate_passes_for_fresh_account() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let result = validate_signed(1, 1000);
		assert!(result.is_ok());

		let (_validity, val, _origin) = result.unwrap();
		assert_eq!(val, Some(1u64));
	});
}

#[test]
fn validate_passes_at_exact_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		assert!(validate_signed(1, MaxBytes::get() as usize).is_ok());
	});
}

#[test]
fn validate_rejects_over_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let result = validate_signed(1, MaxBytes::get() as usize + 1);
		assert_eq!(
			result.unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

#[test]
fn validate_rejects_accumulated_over_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// First tx fills 6 MB
		validate_and_prepare(1, 6 * 1024 * 1024);

		// Second tx tries 5 MB more (total 11 MB > 10 MB limit)
		let result = validate_signed(1, 5 * 1024 * 1024);
		assert_eq!(
			result.unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

#[test]
fn validate_passes_accumulated_within_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// First tx: 4 MB
		validate_and_prepare(1, 4 * 1024 * 1024);
		// Second tx: 4 MB more (total 8 MB < 10 MB)
		assert!(validate_signed(1, 4 * 1024 * 1024).is_ok());
	});
}

#[test]
fn validate_passes_at_exact_limit_after_accumulation() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		validate_and_prepare(1, 5 * 1024 * 1024);
		// Exactly at 10 MB total
		assert!(validate_signed(1, 5 * 1024 * 1024).is_ok());
	});
}

#[test]
fn validate_rejects_one_byte_over_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		validate_and_prepare(1, MaxBytes::get() as usize);
		assert_eq!(
			validate_signed(1, 1).unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

// ---------------------------------------------------------------------------
// Transaction count limit tests
// ---------------------------------------------------------------------------

#[test]
fn validate_rejects_when_tx_count_exceeded() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		AccountUsage::<Test>::insert(1u64, usage(0, MaxTxs::get(), 1));

		assert_eq!(
			validate_signed(1, 0).unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

#[test]
fn validate_passes_at_exact_tx_count_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		// One below the limit — the next validate adds 1 to reach exactly MaxTxs
		AccountUsage::<Test>::insert(1u64, usage(0, MaxTxs::get() - 1, 1));

		assert!(validate_signed(1, 0).is_ok());
	});
}

#[test]
fn validate_rejects_one_tx_over_limit() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		// Already at the limit — the next validate would exceed it
		AccountUsage::<Test>::insert(1u64, usage(0, MaxTxs::get(), 1));

		assert_eq!(
			validate_signed(1, 0).unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

#[test]
fn validate_tx_count_resets_after_window_expires() {
	new_test_ext().execute_with(|| {
		AccountUsage::<Test>::insert(1u64, usage(0, MaxTxs::get(), 10));

		// Advance past the window
		System::set_block_number(10 + WindowSize::get() as u64);

		assert!(validate_signed(1, 0).is_ok());
	});
}

#[test]
fn validate_tx_count_does_not_reset_before_window_expires() {
	new_test_ext().execute_with(|| {
		AccountUsage::<Test>::insert(1u64, usage(0, MaxTxs::get(), 10));

		// One block before window expires
		System::set_block_number(10 + WindowSize::get() as u64 - 1);

		assert_eq!(
			validate_signed(1, 0).unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

#[test]
fn prepare_increments_tx_count() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		validate_and_prepare(1, 0);
		validate_and_prepare(1, 0);
		validate_and_prepare(1, 0);

		assert_eq!(AccountUsage::<Test>::get(1u64).txs_used, 3);
	});
}

// ---------------------------------------------------------------------------
// Unsigned/none origin tests
// ---------------------------------------------------------------------------

#[test]
fn validate_skips_throttle_for_unsigned_tx() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let result = validate_unsigned(MaxBytes::get() as usize + 1);
		assert!(result.is_ok());

		let (_validity, val, _origin) = result.unwrap();
		// val should be None — no account tracked
		assert_eq!(val, None);
	});
}

// ---------------------------------------------------------------------------
// Window expiry tests (via validate)
// ---------------------------------------------------------------------------

#[test]
fn validate_resets_after_window_expires() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Fill to the limit
		validate_and_prepare(1, MaxBytes::get() as usize);
		assert!(validate_signed(1, 1).is_err());

		// Advance past the window
		System::set_block_number(1 + WindowSize::get() as u64);

		// Should pass now — window has expired
		assert!(validate_signed(1, MaxBytes::get() as usize).is_ok());
	});
}

#[test]
fn validate_does_not_reset_before_window_expires() {
	new_test_ext().execute_with(|| {
		// Set known initial state: max bytes used, window started at block 10
		AccountUsage::<Test>::insert(1u64, usage(MaxBytes::get(), 1, 10));

		// One block before window expires
		System::set_block_number(10 + WindowSize::get() as u64 - 1);

		assert_eq!(
			validate_signed(1, 1).unwrap_err(),
			TransactionValidityError::Invalid(InvalidTransaction::ExhaustsResources)
		);
	});
}

#[test]
fn validate_resets_at_exact_window_boundary() {
	new_test_ext().execute_with(|| {
		AccountUsage::<Test>::insert(1u64, usage(MaxBytes::get(), 1, 10));

		// Exactly at window boundary
		System::set_block_number(10 + WindowSize::get() as u64);
		assert!(validate_signed(1, 1).is_ok());
	});
}

// ---------------------------------------------------------------------------
// prepare() storage update tests
// ---------------------------------------------------------------------------

#[test]
fn prepare_updates_storage() {
	new_test_ext().execute_with(|| {
		System::set_block_number(5);

		validate_and_prepare(1, 1000);

		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 1000);
		assert_eq!(u.txs_used, 1);
		// window_start stays at 0 (default) because 5 - 0 < window_size
		assert_eq!(u.window_start, 0);
	});
}

#[test]
fn prepare_accumulates_bytes_in_same_window() {
	new_test_ext().execute_with(|| {
		System::set_block_number(5);
		validate_and_prepare(1, 1000);

		System::set_block_number(10);
		validate_and_prepare(1, 2000);

		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 3000);
		assert_eq!(u.txs_used, 2);
		assert_eq!(u.window_start, 0);
	});
}

#[test]
fn prepare_resets_window_when_expired() {
	new_test_ext().execute_with(|| {
		System::set_block_number(5);
		validate_and_prepare(1, 5000);

		// Advance past window
		System::set_block_number(5 + WindowSize::get() as u64);
		validate_and_prepare(1, 100);

		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 100);
		assert_eq!(u.txs_used, 1);
		assert_eq!(u.window_start, 5 + WindowSize::get() as u64);
	});
}

#[test]
fn prepare_does_not_reset_window_before_expiry() {
	new_test_ext().execute_with(|| {
		// Set known initial state
		AccountUsage::<Test>::insert(1u64, usage(5000, 3, 10));

		System::set_block_number(10 + WindowSize::get() as u64 - 1);
		validate_and_prepare(1, 100);

		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 5100);
		assert_eq!(u.txs_used, 4);
		assert_eq!(u.window_start, 10);
	});
}

#[test]
fn prepare_skips_update_for_unsigned_tx() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
		let info = frame_support::dispatch::DispatchInfo::default();

		let (_validity, val, origin) = CheckThrottle::<Test>::new()
			.validate(
				RuntimeOrigin::none(),
				&call,
				&info,
				5000,
				(),
				&TxBaseImplication(&call),
				TransactionSource::External,
			)
			.unwrap();

		assert_eq!(val, None);
		assert_ok!(CheckThrottle::<Test>::new().prepare(val, &origin, &call, &info, 5000));

		// No storage update for unsigned
		assert_eq!(AccountUsage::<Test>::get(1u64), usage(0, 0, 0));
	});
}

// ---------------------------------------------------------------------------
// Multi-account isolation tests
// ---------------------------------------------------------------------------

#[test]
fn throttle_is_independent_per_account() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Fill account 1 to the limit
		validate_and_prepare(1, MaxBytes::get() as usize);
		assert!(validate_signed(1, 1).is_err());

		// Account 2 should still have full allowance
		assert!(validate_signed(2, MaxBytes::get() as usize).is_ok());
	});
}

#[test]
fn multiple_accounts_track_usage_independently() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		validate_and_prepare(1, 3000);
		validate_and_prepare(2, 7000);

		assert_eq!(AccountUsage::<Test>::get(1u64).bytes_used, 3000);
		assert_eq!(AccountUsage::<Test>::get(2u64).bytes_used, 7000);
	});
}

// ---------------------------------------------------------------------------
// Multiple window cycles
// ---------------------------------------------------------------------------

#[test]
fn usage_resets_across_multiple_windows() {
	new_test_ext().execute_with(|| {
		let window = WindowSize::get() as u64;

		// Window 1
		System::set_block_number(1);
		validate_and_prepare(1, MaxBytes::get() as usize);
		assert!(validate_signed(1, 1).is_err());

		// Window 2
		System::set_block_number(1 + window);
		validate_and_prepare(1, MaxBytes::get() as usize);
		assert!(validate_signed(1, 1).is_err());

		// Window 3
		System::set_block_number(1 + 2 * window);
		assert!(validate_signed(1, MaxBytes::get() as usize).is_ok());
	});
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn zero_length_transaction_always_passes() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		validate_and_prepare(1, MaxBytes::get() as usize);
		assert!(validate_signed(1, 0).is_ok());
	});
}

#[test]
fn saturating_add_prevents_overflow() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Set bytes_used near u64::MAX — validate would reject, so call prepare directly
		AccountUsage::<Test>::insert(1u64, usage(u64::MAX - 10, 0, 1));

		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
		let info = frame_support::dispatch::DispatchInfo::default();
		let origin = RuntimeOrigin::signed(1);

		assert_ok!(CheckThrottle::<Test>::new().prepare(Some(1u64), &origin, &call, &info, 100));

		assert_eq!(AccountUsage::<Test>::get(1u64).bytes_used, u64::MAX);
	});
}

#[test]
fn block_number_zero_works() {
	new_test_ext().execute_with(|| {
		// Block 0 is the default
		assert!(validate_signed(1, 1000).is_ok());
		validate_and_prepare(1, 1000);

		let u = AccountUsage::<Test>::get(1u64);
		assert_eq!(u.bytes_used, 1000);
		assert_eq!(u.txs_used, 1);
		assert_eq!(u.window_start, 0);
	});
}

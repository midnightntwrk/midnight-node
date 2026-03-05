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

//! Tests for system-parameters pallet

use crate::{
	Error, Event,
	mock::*,
	pallet::{DEFAULT_TERMS_AND_CONDITIONS_HASH_BYTES, DEFAULT_TERMS_AND_CONDITIONS_URL},
};
use frame_support::{assert_noop, assert_ok};
use sidechain_domain::DParameter;
use sp_core::H256;

const DEFAULT_TERMS_AND_CONDITIONS_HASH: H256 = H256(DEFAULT_TERMS_AND_CONDITIONS_HASH_BYTES);

#[test]
fn update_terms_and_conditions_works_with_root() {
	new_test_ext().execute_with(|| {
		// Set block number to 1 so events are registered
		System::set_block_number(1);

		let hash = H256::from_low_u64_be(123);
		let url = b"https://example.com/terms".to_vec();

		assert_ok!(SystemParameters::update_terms_and_conditions(
			RuntimeOrigin::root(),
			hash,
			url.clone()
		));

		let stored = SystemParameters::terms_and_conditions().expect("Should have terms");
		assert_eq!(stored.hash, hash);
		assert_eq!(stored.url.to_vec(), url);

		// Check event
		System::assert_last_event(
			Event::<Test>::TermsAndConditionsUpdated { hash, url: url.try_into().unwrap() }.into(),
		);
	});
}

#[test]
fn update_terms_and_conditions_fails_with_non_root() {
	new_test_ext().execute_with(|| {
		let hash = H256::from_low_u64_be(123);
		let url = b"https://example.com/terms".to_vec();

		assert_noop!(
			SystemParameters::update_terms_and_conditions(RuntimeOrigin::signed(1), hash, url),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn update_terms_and_conditions_fails_with_url_too_long() {
	new_test_ext().execute_with(|| {
		let hash = H256::from_low_u64_be(123);
		// Create a URL longer than MAX_URL_SIZE (256 bytes)
		let url = vec![b'a'; 300];

		assert_noop!(
			SystemParameters::update_terms_and_conditions(RuntimeOrigin::root(), hash, url),
			Error::<Test>::UrlTooLong
		);
	});
}

#[test]
fn update_d_parameter_works_with_root() {
	new_test_ext().execute_with(|| {
		// Set block number to 1 so events are registered
		System::set_block_number(1);

		let num_permissioned = 10u16;
		let num_registered = 5u16;

		assert_ok!(SystemParameters::update_d_parameter(
			RuntimeOrigin::root(),
			num_permissioned,
			num_registered
		));

		let stored = SystemParameters::get_d_parameter();
		assert_eq!(stored.num_permissioned_candidates, num_permissioned);
		assert_eq!(stored.num_registered_candidates, num_registered);

		// Check event
		System::assert_last_event(
			Event::<Test>::DParameterUpdated {
				num_permissioned_candidates: num_permissioned,
				num_registered_candidates: num_registered,
			}
			.into(),
		);
	});
}

#[test]
fn update_d_parameter_fails_with_non_root() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			SystemParameters::update_d_parameter(RuntimeOrigin::signed(1), 10, 5),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn genesis_config_initializes_terms_and_conditions() {
	let hash = H256::from_low_u64_be(456);
	let url = "https://example.com/genesis-terms".to_string();

	new_test_ext_with_genesis(Some(hash), Some(url.clone()), None).execute_with(|| {
		let stored = SystemParameters::terms_and_conditions().expect("Should have terms");
		assert_eq!(stored.hash, hash);
		assert_eq!(stored.url.to_vec(), url.as_bytes().to_vec());
	});
}

#[test]
fn genesis_config_initializes_d_parameter() {
	let d_param = DParameter::new(15, 10);

	new_test_ext_with_genesis(
		Some(DEFAULT_TERMS_AND_CONDITIONS_HASH),
		Some(DEFAULT_TERMS_AND_CONDITIONS_URL.to_string()),
		Some(d_param.clone()),
	)
	.execute_with(|| {
		let stored = SystemParameters::get_d_parameter();
		assert_eq!(stored.num_permissioned_candidates, 15);
		assert_eq!(stored.num_registered_candidates, 10);
	});
}

#[test]
fn genesis_config_initializes_both_parameters() {
	let hash = H256::from_low_u64_be(789);
	let url = "https://example.com/both".to_string();
	let d_param = DParameter::new(20, 15);

	new_test_ext_with_genesis(Some(hash), Some(url.clone()), Some(d_param.clone())).execute_with(
		|| {
			let terms = SystemParameters::terms_and_conditions().expect("Should have terms");
			assert_eq!(terms.hash, hash);
			assert_eq!(terms.url.to_vec(), url.as_bytes().to_vec());

			let d = SystemParameters::get_d_parameter();
			assert_eq!(d.num_permissioned_candidates, 20);
			assert_eq!(d.num_registered_candidates, 15);
		},
	);
}

#[test]
fn d_parameter_has_default_values_without_genesis() {
	new_test_ext().execute_with(|| {
		let stored = SystemParameters::get_d_parameter();
		// Default DParameter should have 0 values
		assert_eq!(stored.num_permissioned_candidates, 0);
		assert_eq!(stored.num_registered_candidates, 0);
	});
}

#[test]
fn terms_and_conditions_is_none_without_genesis() {
	new_test_ext().execute_with(|| {
		assert!(SystemParameters::terms_and_conditions().is_none());
	});
}

#[test]
fn can_update_terms_multiple_times() {
	new_test_ext().execute_with(|| {
		let hash1 = H256::from_low_u64_be(1);
		let url1 = b"https://example.com/v1".to_vec();
		let hash2 = H256::from_low_u64_be(2);
		let url2 = b"https://example.com/v2".to_vec();

		assert_ok!(SystemParameters::update_terms_and_conditions(
			RuntimeOrigin::root(),
			hash1,
			url1
		));

		assert_ok!(SystemParameters::update_terms_and_conditions(
			RuntimeOrigin::root(),
			hash2,
			url2.clone()
		));

		let stored = SystemParameters::terms_and_conditions().expect("Should have terms");
		assert_eq!(stored.hash, hash2);
		assert_eq!(stored.url.to_vec(), url2);
	});
}

#[test]
fn can_update_d_parameter_multiple_times() {
	new_test_ext().execute_with(|| {
		assert_ok!(SystemParameters::update_d_parameter(RuntimeOrigin::root(), 5, 3));
		assert_ok!(SystemParameters::update_d_parameter(RuntimeOrigin::root(), 10, 8));

		let stored = SystemParameters::get_d_parameter();
		assert_eq!(stored.num_permissioned_candidates, 10);
		assert_eq!(stored.num_registered_candidates, 8);
	});
}

#[test]
fn get_terms_and_conditions_helper_works() {
	new_test_ext().execute_with(|| {
		let hash = H256::from_low_u64_be(999);
		let url = b"https://example.com/helper".to_vec();

		assert_ok!(SystemParameters::update_terms_and_conditions(
			RuntimeOrigin::root(),
			hash,
			url.clone()
		));

		let stored = SystemParameters::get_terms_and_conditions().expect("Should have terms");
		assert_eq!(stored.hash, hash);
		assert_eq!(stored.url.to_vec(), url);
	});
}

#[test]
fn get_d_parameter_helper_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(SystemParameters::update_d_parameter(RuntimeOrigin::root(), 7, 4));

		let stored = SystemParameters::get_d_parameter();
		assert_eq!(stored.num_permissioned_candidates, 7);
		assert_eq!(stored.num_registered_candidates, 4);
	});
}

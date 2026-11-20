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
fn versions_install_in_order_and_keep_their_history() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			BlockRewards::install_parameters(parameters(2)),
			Err(InstallError::VersionNotNext { expected: 1 })
		);
		assert_ok!(BlockRewards::install_parameters(parameters(1)));
		assert_eq!(last_event(), Event::ParametersInstalled { version: 1 });
		let next = RewardParametersV1 { publication_hash: [1; 32], ..parameters(2) };
		assert_ok!(BlockRewards::install_parameters(next.clone()));

		assert_eq!(CurrentParametersVersion::<Test>::get(), Some(2));
		assert_eq!(BlockRewards::current_parameters(), Some(next));
		assert_eq!(BlockRewards::parameters(1), Some(parameters(1)));
	});
}

#[test]
fn a_later_version_keeps_the_initial_reserve() {
	installed().execute_with(|| {
		let next = RewardParametersV1 { initial_reward_reserve_star: 1, ..parameters(2) };
		assert_eq!(BlockRewards::install_parameters(next), Err(InstallError::ReserveChanged));
		assert_eq!(CurrentParametersVersion::<Test>::get(), Some(1));
	});
}

#[test]
fn an_invalid_set_is_not_stored() {
	let cases = [
		(
			"blocks per year must match the chain slot: three-second blocks",
			RewardParametersV1 {
				expected_blocks_per_year: 2 * 5_256_000,
				distribution_rate: Ratio::new(7, 876_000_000),
				..parameters(1)
			},
			ParameterError::BlocksPerYearNotSlotDerived,
		),
		(
			"a rate the inputs do not derive",
			RewardParametersV1 { distribution_rate: Ratio::new(8, 438_000_000), ..parameters(1) },
			ParameterError::RateNotDerived,
		),
	];
	for (case, set, error) in cases {
		new_test_ext().execute_with(|| {
			assert_eq!(
				BlockRewards::install_parameters(set),
				Err(InstallError::Invalid(error)),
				"{case}"
			);
			assert_eq!(CurrentParametersVersion::<Test>::get(), None, "{case}");
			assert!(System::events().is_empty(), "{case}");
		});
	}
}

#[test]
fn genesis_installs_the_first_version() {
	use sp_runtime::BuildStorage;
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	crate::GenesisConfig::<Test> { parameters: Some(parameters(1)), _marker: Default::default() }
		.assimilate_storage(&mut storage)
		.unwrap();
	sp_io::TestExternalities::new(storage).execute_with(|| {
		assert_eq!(CurrentParametersVersion::<Test>::get(), Some(1));
		assert_eq!(BlockRewards::current_parameters(), Some(parameters(1)));
	});
}

#[test]
fn the_dev_network_parameters_are_valid_at_six_second_slots() {
	let parameters: RewardParametersV1 =
		serde_json::from_str(include_str!("../../../../res/dev/block-rewards-parameters.json"))
			.unwrap();
	assert_eq!(parameters.version, 1);
	assert_eq!(parameters.validate(6_000), Ok(()));
}

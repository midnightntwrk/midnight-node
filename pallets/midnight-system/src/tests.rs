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

// grcov-excl-start
use crate::{
	Error,
	mock::{self, RuntimeOrigin, Test},
};
use frame_support::{assert_err, assert_ok};
use midnight_node_ledger::types::active_ledger_bridge as LedgerApi;
use midnight_node_ledger_helpers::{DustPublicKey, Fr, serialize_untagged};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives::{
	MidnightSystemTransactionBridgeExecutor, MidnightSystemTransactionCNightExecutor,
};

fn init_ledger_state() {
	let path_buf = tempfile::tempdir().unwrap().keep();
	let state_key = midnight_node_ledger::latest::storage::init_storage_paritydb_separate(
		&path_buf,
		UndeployedNetwork.genesis_state(),
		1024 * 1024,
	);
	mock::Midnight::initialize_state(UndeployedNetwork.id(), &state_key);
	mock::System::set_block_number(1);
}

fn cnight_tx() -> Vec<u8> {
	let owner = serialize_untagged(&DustPublicKey(Fr::from(7u64))).unwrap();
	let event =
		LedgerApi::construct_cnight_generates_dust_event(1_000, &owner, 0, 0, [0u8; 32]).unwrap();
	LedgerApi::construct_cnight_generates_dust_system_tx(vec![event]).unwrap()
}

fn unlock_to_treasury_tx() -> Vec<u8> {
	LedgerApi::construct_unlock_to_treasury_system_tx(0).unwrap()
}

fn distribute_reserve_tx() -> Vec<u8> {
	LedgerApi::construct_distribute_reserve_system_tx(0).unwrap()
}

fn distribute_night_cardano_bridge_tx() -> Vec<u8> {
	LedgerApi::construct_distribute_night_cardano_bridge_system_tx(0, &[0u8; 32], [0u8; 32])
		.unwrap()
}

#[test]
fn governance_rejects_non_overwrite_parameters() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			mock::MidnightSystem::send_mn_system_transaction(RuntimeOrigin::root(), cnight_tx()),
			Error::<Test>::SystemTransactionNotAllowedForGovernance,
		);
	});
}

#[test]
fn cnight_executor_rejects_bridge_only_tx() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				unlock_to_treasury_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForCNight,
		);
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				distribute_reserve_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForCNight,
		);
	});
}

#[test]
fn cnight_executor_accepts_cnight_generates_dust_update() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				cnight_tx()
			)
		);
	});
}

#[test]
fn bridge_executor_rejects_cnight_generates_dust_update() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				cnight_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForBridge,
		);
	});
}

#[test]
fn bridge_executor_accepts_its_three_allowed_variants() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				unlock_to_treasury_tx()
			)
		);
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				distribute_reserve_tx()
			)
		);
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				distribute_night_cardano_bridge_tx()
			)
		);
	});
}
